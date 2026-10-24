// One fence panel = one frameless GTK4 window per group, all windows owned by
// a single application process. Stacking/placement is delegated to the shell
// extension; here we own content: header (drag area, collapse, menu), item
// grid, drag-and-drop, actions, dialogs.

use crate::dialogs;
use crate::items::{self, APP_DRAG};
use crate::model::{Group, ItemKind, SharedState, State};
use gtk4::prelude::*;
use gtk4::{self as gtk, gdk, gio, glib};
use std::rc::Rc;

pub struct FenceWindow {
    pub id: String,
    pub window: gtk::Window,
    pub flow: gtk::FlowBox,
    pub title_label: gtk::Label,
    pub body: gtk::ScrolledWindow,
    pub collapse_btn: gtk::Button,
    pub menu_btn: gtk::MenuButton,
    pub header: gtk::Box,
}

pub fn build_fence(state: &SharedState, group: &Group) -> Rc<FenceWindow> {
    let accent_class = format!("accent-{}", group.accent % crate::model::ACCENTS as u8);
    let window = gtk::Window::builder()
        .title(format!("gnome-fences:{}", group.id))
        .decorated(false)
        .resizable(true)
        .default_width(group.w)
        .default_height(group.h)
        .css_classes(["fence", &accent_class])
        .build();

    let root = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .build();

    let handle = gtk::WindowHandle::builder().build();

    let header = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(4)
        .css_classes(["fence-header"])
        .build();

    let title_label = gtk::Label::builder()
        .label(&group.title)
        .hexpand(true)
        .xalign(0.0)
        .margin_start(10)
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .build();
    header.append(&title_label);

    let collapse_btn = gtk::Button::from_icon_name("pan-down-symbolic");
    collapse_btn.set_tooltip_text(Some("Collapse"));
    header.append(&collapse_btn);

    let menu_btn = gtk::MenuButton::builder().icon_name("open-menu-symbolic").build();
    header.append(&menu_btn);

    let body = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .css_classes(["fence-body"])
        .build();

    let flow = gtk::FlowBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .homogeneous(true)
        .min_children_per_line(2)
        .max_children_per_line(4)
        .valign(gtk::Align::Start)
        .build();
    body.set_child(Some(&flow));

    let body_box = gtk::Box::builder().orientation(gtk::Orientation::Vertical).build();
    body_box.append(&body);

    let content = gtk::Box::builder().orientation(gtk::Orientation::Vertical).build();
    content.append(&header);
    content.append(&body_box);
    handle.set_child(Some(&content));
    root.append(&handle);
    window.set_child(Some(&root));

    let fence = Rc::new(FenceWindow {
        id: group.id.clone(),
        window,
        flow,
        title_label,
        body,
        collapse_btn,
        menu_btn,
        header,
    });

    install_actions(state, &fence, group);
    install_drop_target(state, &fence);
    reload_items(state, &fence);
    apply_collapsed(&fence, group.collapsed);

    let state_c = state.clone();
    fence.window.connect_close_request(move |_| {
        state_c.save();
        glib::Propagation::Proceed
    });

    fence
}

pub fn sync_sizes(state: &State) {
    let mut layout = state.layout.borrow_mut();
    for fence in state.windows.borrow().iter() {
        let (w, h) = fence.window.default_size();
        if let Some(g) = layout.group_mut(&fence.id) {
            if w > 0 { g.w = w; }
            if h > 0 { g.h = h; }
        }
    }
}

fn apply_collapsed(fence: &FenceWindow, collapsed: bool) {
    fence.body.set_visible(!collapsed);
    let icon = if collapsed { "pan-end-symbolic" } else { "pan-down-symbolic" };
    fence.collapse_btn.set_icon_name(icon);
}

pub fn refresh_title(fence: &FenceWindow, group: &Group) {
    fence.title_label.set_text(&group.title);
}

pub fn reload_items(state: &SharedState, fence: &Rc<FenceWindow>) {
    while let Some(child) = fence.flow.child_at_index(0) {
        fence.flow.remove(&child);
    }
    let layout = state.layout.borrow();
    let Some(group) = layout.group(&fence.id) else { return };
    let items_snapshot = group.items.clone();
    drop(layout);

    for item in items_snapshot {
        let state_c = state.clone();
        let fence_c = fence.clone();
        let item_menu = item.clone();
        let child = items::build_tile(state, &fence.id, &item, move |widget| {
            show_item_menu(&state_c, &fence_c, &item_menu, widget);
        });
        fence.flow.insert(&child, -1);
    }
}

fn show_item_menu(state: &SharedState, fence: &Rc<FenceWindow>, item: &crate::model::Item, at: &gtk::Widget) {
    let pop = gtk::Popover::new();
    pop.set_parent(at);
    pop.set_position(gtk::PositionType::Bottom);

    let box_ = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(2)
        .margin_top(6)
        .margin_bottom(6)
        .margin_start(6)
        .margin_end(6)
        .build();

    let mk_btn = |label: &str| -> gtk::Button {
        gtk::Button::builder()
            .label(label)
            .has_frame(false)
            .halign(gtk::Align::Fill)
            .build()
    };

    let open_item = item.clone();
    let at_c = at.clone();
    let pop_c = pop.clone();
    let b_open = mk_btn("Open");
    b_open.connect_clicked(move |_| {
        pop_c.popdown();
        if let Err(e) = items::open(&open_item) {
            dialogs::error(&at_c, &e);
        }
    });
    box_.append(&b_open);

    let item_c = item.clone();
    let state_c = state.clone();
    let fence_c = fence.clone();
    let pop_c = pop.clone();
    let b_rename = mk_btn("Rename…");
    b_rename.connect_clicked(move |_| {
        pop_c.popdown();
        if let Some(new_label) = dialogs::prompt_text(&fence_c.window, "Rename item", "Label", &item_c.label) {
            state_c.push_undo();
            if let Some(g) = state_c.layout.borrow_mut().group_mut(&fence_c.id) {
                if let Some(it) = g.items.iter_mut().find(|i| i.id == item_c.id) {
                    it.label = new_label;
                }
            }
            state_c.save();
            reload_items(&state_c, &fence_c);
        }
    });
    box_.append(&b_rename);

    let item_c = item.clone();
    let state_c = state.clone();
    let fence_c = fence.clone();
    let pop_c = pop.clone();
    let b_remove = mk_btn("Remove from group");
    b_remove.connect_clicked(move |_| {
        pop_c.popdown();
        state_c.push_undo();
        if let Some(g) = state_c.layout.borrow_mut().group_mut(&fence_c.id) {
            g.items.retain(|i| i.id != item_c.id);
        }
        state_c.save();
        reload_items(&state_c, &fence_c);
    });
    box_.append(&b_remove);

    pop.set_child(Some(&box_));
    pop.popup();
}

fn install_drop_target(state: &SharedState, fence: &Rc<FenceWindow>) {
    let dt = gtk::DropTarget::new(glib::Type::INVALID, gdk::DragAction::COPY);
    dt.set_types(&[gio::File::static_type(), glib::Type::STRING]);

    let state_c = state.clone();
    let fence_c = fence.clone();
    dt.connect_drop(move |_dt, value, _x, _y| {
        let state = &state_c;
        let fence = &fence_c;

        let payload = APP_DRAG.with(|c| c.borrow().clone());
        if let (Some((src_group, src_item)), Ok(_text)) = (payload, value.get::<String>()) {
            APP_DRAG.with(|c| *c.borrow_mut() = None);
            if src_group != fence.id {
                state.push_undo();
                let moved = state
                    .layout
                    .borrow_mut()
                    .group_mut(&src_group)
                    .and_then(|g| g.items.iter().position(|i| i.id == src_item).map(|p| g.items.remove(p)));
                if let Some(item) = moved {
                    if let Some(g) = state.layout.borrow_mut().group_mut(&fence.id) {
                        g.items.push(item);
                    }
                    state.save();
                    reload_items(state, fence);
                    if let Some(src) = state.windows.borrow().iter().find(|w| w.id == src_group) {
                        reload_items(state, src);
                    }
                }
                return true;
            }
            return true;
        }

        if let Ok(file) = value.get::<gio::File>() {
            let mut added = false;
            state.push_undo();
            let mut layout = state.layout.borrow_mut();
            if let Some(g) = layout.group_mut(&fence.id) {
                if let Some(item) = crate::model::make_file_item(&file) {
                    g.items.push(item);
                    added = true;
                }
            }
            drop(layout);
            if added {
                state.save();
                reload_items(state, fence);
            }
            return true;
        }

        if let Ok(text) = value.get::<String>() {
            let mut added = false;
            state.push_undo();
            let mut layout = state.layout.borrow_mut();
            for line in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
                if line.starts_with("http://") || line.starts_with("https://") {
                    if let Some(g) = layout.group_mut(&fence.id) {
                        g.items.push(crate::model::Item::new_url(line, None));
                        added = true;
                    }
                } else if let Some(file) = line.strip_prefix("file://") {
                    let decoded = urldecode(file);
                    if let Some(g) = layout.group_mut(&fence.id) {
                        g.items.push(crate::model::Item {
                            id: crate::model::new_id(),
                            label: std::path::Path::new(&decoded)
                                .file_name()
                                .map(|n| n.to_string_lossy().into_owned())
                                .unwrap_or_else(|| decoded.clone()),
                            r#ref: decoded,
                            kind: ItemKind::File,
                        });
                        added = true;
                    }
                }
            }
            drop(layout);
            if added {
                state.save();
                reload_items(state, fence);
            }
            return true;
        }

        false
    });
    fence.flow.add_controller(dt);
}

fn urldecode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(b) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(b);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn add_files_from_chooser(state: &SharedState, fence: &Rc<FenceWindow>, folders: bool) {
    let chooser = gtk::FileChooserNative::builder()
        .title(if folders { "Add folder" } else { "Add file" })
        .transient_for(&fence.window)
        .modal(true)
        .action(if folders {
            gtk::FileChooserAction::SelectFolder
        } else {
            gtk::FileChooserAction::Open
        })
        .select_multiple(true)
        .build();
    chooser.set_accept_label(Some("Add"));

    let state_c = state.clone();
    let fence_c = fence.clone();
    chooser.connect_response(move |ch, resp| {
        if resp == gtk::ResponseType::Accept {
            let mut added = false;
            state_c.push_undo();
            let mut layout = state_c.layout.borrow_mut();
            if let Some(g) = layout.group_mut(&fence_c.id) {
                let list = ch.files();
                let n = list.n_items();
                for i in 0..n {
                    if let Some(file) = list.item(i).and_then(|o| o.downcast::<gio::File>().ok()) {
                        if let Some(item) = crate::model::make_file_item(&file) {
                            g.items.push(item);
                            added = true;
                        }
                    }
                }
            }
            drop(layout);
            if added {
                state_c.save();
                reload_items(&state_c, &fence_c);
            }
        }
    });
    chooser.show();
}

fn add_action<F: Fn(&gio::SimpleAction, Option<&glib::Variant>) + 'static>(
    actions: &gio::SimpleActionGroup,
    name: &str,
    f: F,
) {
    let a = gio::SimpleAction::new(name, None);
    a.connect_activate(f);
    actions.add_action(&a);
}

fn install_actions(state: &SharedState, fence: &Rc<FenceWindow>, _group: &Group) {
    let actions = gio::SimpleActionGroup::new();
    fence.window.insert_action_group("win", Some(&actions));

    let st = state.clone();
    let fc = fence.clone();
    add_action(&actions, "add-file", move |_, _| add_files_from_chooser(&st, &fc, false));

    let st = state.clone();
    let fc = fence.clone();
    add_action(&actions, "add-folder", move |_, _| add_files_from_chooser(&st, &fc, true));

    let st = state.clone();
    let fc = fence.clone();
    add_action(&actions, "add-link", move |_, _| {
        let Some(url) = dialogs::prompt_text(&fc.window, "Add link", "URL", "https://") else { return };
        if !(url.starts_with("http://") || url.starts_with("https://")) {
            dialogs::error(&fc.window, "Only http:// and https:// links are supported.");
            return;
        }
        let initial = url.clone();
        let label = dialogs::prompt_text(&fc.window, "Add link", "Name", &initial);
        st.push_undo();
        if let Some(g) = st.layout.borrow_mut().group_mut(&fc.id) {
            g.items.push(crate::model::Item::new_url(&url, label));
        }
        st.save();
        reload_items(&st, &fc);
    });

    let st = state.clone();
    let fc = fence.clone();
    add_action(&actions, "add-app", move |_, _| {
        if let Some((id, name)) = dialogs::choose_app(&fc.window) {
            st.push_undo();
            if let Some(g) = st.layout.borrow_mut().group_mut(&fc.id) {
                g.items.push(crate::model::Item::new_app(&id, name));
            }
            st.save();
            reload_items(&st, &fc);
        }
    });

    let st = state.clone();
    let fc = fence.clone();
    add_action(&actions, "rename", move |_, _| {
        let current = st.layout.borrow().group(&fc.id).map(|g| g.title.clone()).unwrap_or_default();
        if let Some(t) = dialogs::prompt_text(&fc.window, "Rename fence", "Title", &current) {
            st.push_undo();
            if let Some(g) = st.layout.borrow_mut().group_mut(&fc.id) {
                g.title = t;
            }
            st.save();
            let layout = st.layout.borrow();
            if let Some(g) = layout.group(&fc.id) {
                refresh_title(&fc, g);
            }
        }
    });

    let st = state.clone();
    let fc = fence.clone();
    let accent_action = gio::SimpleAction::new("accent", Some(glib::VariantTy::INT32));
    accent_action.connect_activate(move |_, param| {
        let Some(v) = param.and_then(|p| p.get::<i32>()) else { return };
        st.push_undo();
        if let Some(g) = st.layout.borrow_mut().group_mut(&fc.id) {
            g.accent = (v.max(0) as u8) % crate::model::ACCENTS as u8;
        }
        st.save();
        let accent = st.layout.borrow().group(&fc.id).map(|g| g.accent).unwrap_or(0);
        for i in 0..crate::model::ACCENTS {
            fc.window.remove_css_class(&format!("accent-{i}"));
        }
        fc.window.add_css_class(&format!("accent-{accent}"));
    });
    actions.add_action(&accent_action);

    let st = state.clone();
    let fc = fence.clone();
    add_action(&actions, "collapse", move |_, _| {
        let collapsed = st.layout.borrow().group(&fc.id).map(|g| g.collapsed).unwrap_or(false);
        if let Some(g) = st.layout.borrow_mut().group_mut(&fc.id) {
            g.collapsed = !collapsed;
        }
        st.save();
        apply_collapsed(&fc, !collapsed);
    });

    let st = state.clone();
    let fc = fence.clone();
    add_action(&actions, "remove", move |_, _| {
        st.push_undo();
        st.layout.borrow_mut().groups.retain(|g| g.id != fc.id);
        st.save();
        let w = fc.window.clone();
        w.destroy();
    });

    let st = state.clone();
    add_action(&actions, "new-fence", move |_, _| {
        st.push_undo();
        let count = st.layout.borrow().groups.len();
        let g = Group::new(&format!("Fence {}", count + 1), count);
        let id = g.id.clone();
        st.layout.borrow_mut().groups.push(g);
        st.save();
        let layout = st.layout.borrow();
        let group = layout.group(&id).cloned().unwrap();
        drop(layout);
        let w = build_fence(&st, &group);
        st.windows.borrow_mut().push(w.clone());
        w.window.present();
        crate::dbus::stack_and_place(&st, &group);
    });

    let st = state.clone();
    add_action(&actions, "undo", move |_, _| crate::undo_and_rebuild(&st));

    let st = state.clone();
    add_action(&actions, "settings", move |_, _| crate::settings::show(&st));

    let st = state.clone();
    add_action(&actions, "quit", move |_, _| {
        st.save();
        st.app.quit();
    });

    let fc_click = fence.clone();
    fence
        .collapse_btn
        .connect_clicked(move |_| {
            let _ = fc_click.window.activate_action("win.collapse", None);
        });

    let header_menu = gio::Menu::new();
    build_menu(&header_menu);
    let pop = gtk::PopoverMenu::builder().menu_model(&header_menu).build();
    fence.menu_btn.set_popover(Some(&pop));

    let right = gtk::GestureClick::new();
    right.set_button(gdk::BUTTON_SECONDARY);
    let pop_c = pop.clone();
    let header_c = fence.header.clone();
    right.connect_released(move |_, _, x, y| {
        let rect = gdk::Rectangle::new(x as i32, y as i32, 1, 1);
        pop_c.set_pointing_to(Some(&rect));
        pop_c.set_parent(&header_c);
        pop_c.popup();
    });
    fence.header.add_controller(right);
}

pub fn build_menu(menu: &gio::Menu) {
    let add_section = gio::Menu::new();
    add_section.append(Some("Add file…"), Some("win.add-file"));
    add_section.append(Some("Add folder…"), Some("win.add-folder"));
    add_section.append(Some("Add link…"), Some("win.add-link"));
    add_section.append(Some("Add application…"), Some("win.add-app"));
    menu.append_section(None, &add_section);

    let fence_section = gio::Menu::new();
    fence_section.append(Some("New fence"), Some("win.new-fence"));
    fence_section.append(Some("Rename fence…"), Some("win.rename"));
    fence_section.append(Some("Collapse"), Some("win.collapse"));
    fence_section.append(Some("Remove fence"), Some("win.remove"));
    menu.append_section(None, &fence_section);

    let accent_menu = gio::Menu::new();
    for i in 0..crate::model::ACCENTS {
        accent_menu.append(Some(&format!("Color {}", i + 1)), Some(&format!("win.accent({i})")));
    }
    let color_section = gio::Menu::new();
    color_section.append_submenu(Some("Accent color"), &accent_menu);
    menu.append_section(None, &color_section);

    let app_section = gio::Menu::new();
    app_section.append(Some("Undo"), Some("win.undo"));
    app_section.append(Some("Settings…"), Some("win.settings"));
    app_section.append(Some("Quit"), Some("win.quit"));
    menu.append_section(None, &app_section);
}
