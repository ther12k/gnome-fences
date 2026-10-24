// Item tiles: icon + label, open action, drag source, per-item menu.

use crate::model::{Item, ItemKind};
use gtk4::prelude::*;
use gtk4::{self as gtk, gdk, gio, glib};

/// In-app drag payload: (source fence id, item id). Same-process DnD, so we
/// can carry identity in memory instead of inventing a mimetype; the drag
/// still advertises plain text so other apps can accept the label if dropped.
thread_local! {
    pub static APP_DRAG: std::cell::RefCell<Option<(String, String)>> =
        const { std::cell::RefCell::new(None) };
}

pub fn icon_for_item(item: &Item) -> gio::Icon {
    match item.kind {
        ItemKind::Url => gio::ThemedIcon::new("text-html").upcast(),
        ItemKind::App => {
            let fallback: gio::Icon = gio::ThemedIcon::new("application-x-executable").upcast();
            gio::DesktopAppInfo::new(&item.r#ref)
                .and_then(|dai| dai.icon())
                .unwrap_or(fallback)
        }
        ItemKind::File | ItemKind::Folder => {
            let file = gio::File::for_path(&item.r#ref);
            let info = file.query_info(
                "standard::icon",
                gio::FileQueryInfoFlags::NONE,
                None::<&gio::Cancellable>,
            );
            if let Ok(info) = info {
                if let Some(icon) = info.icon() {
                    return icon;
                }
            }
            if item.kind == ItemKind::Folder {
                gio::ThemedIcon::new("folder").upcast()
            } else {
                gio::ThemedIcon::new("text-x-generic").upcast()
            }
        }
    }
}

/// Open through the appropriate application. Folders open in the file
/// manager, files in their default app, URLs in the browser, .desktop ids
/// via DesktopAppInfo. Returns an error message on failure.
pub fn open(item: &Item) -> Result<(), String> {
    match item.kind {
        ItemKind::App => {
            let dai = gio::DesktopAppInfo::new(&item.r#ref)
                .ok_or_else(|| format!("No installed application with id {}", item.r#ref))?;
            dai.launch(&[], None::<&gio::AppLaunchContext>)
                .map_err(|e| format!("Could not launch {}: {e}", item.label))
        }
        ItemKind::File | ItemKind::Folder | ItemKind::Url => {
            let uri = match item.kind {
                ItemKind::Url => item.r#ref.clone(),
                _ => gio::File::for_path(&item.r#ref).uri(),
            };
            gio::AppInfo::launch_default_for_uri(&uri, None::<&gio::AppLaunchContext>)
                .map_err(|e| format!("No application for {}: {e}", item.label))
        }
    }
}

pub fn build_tile(
    state: &crate::model::SharedState,
    group_id: &str,
    item: &Item,
    on_menu: impl Fn(&gtk::Widget) + 'static,
) -> gtk::FlowBoxChild {
    let tile = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(4)
        .margin_top(8)
        .margin_bottom(8)
        .margin_start(4)
        .margin_end(4)
        .css_classes(vec!["fence-item".into()])
        .build();

    let img = gtk::Image::builder().pixel_size(36).build();
    img.set_from_gicon(Some(&icon_for_item(item)));
    tile.append(&img);

    let label = gtk::Label::builder()
        .label(&item.label)
        .wrap(true)
        .wrap_mode(gtk::pango::WrapMode::WordChar)
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .lines(2)
        .justify(gtk::Justification::Center)
        .build();
    tile.append(&label);

    // Drag source: COPY only. We take references, never moves — accepting
    // DragAction::MOVE would tell source apps (e.g. Nautilus) to relocate
    // the file.
    let ds = gtk::DragSource::new();
    ds.set_actions(gdk::DragAction::COPY);
    let payload = (group_id.to_string(), item.id.clone());
    let bytes = glib::Bytes::from(item.label.as_bytes());
    ds.set_content(Some(&gdk::ContentProvider::new_for_bytes("text/plain", &bytes)));
    ds.connect_drag_begin(move |_, _| {
        APP_DRAG.with(|c| *c.borrow_mut() = Some(payload.clone()));
    });
    ds.connect_drag_cancel(move |_, _, _| {
        APP_DRAG.with(|c| *c.borrow_mut() = None);
        false
    });
    ds.connect_drag_end(move |_, _, _| {
        APP_DRAG.with(|c| *c.borrow_mut() = None);
    });
    tile.add_controller(ds);

    // Open on double-click (primary button only).
    let click = gtk::GestureClick::new();
    click.set_button(gdk::BUTTON_PRIMARY);
    let item_c = item.clone();
    let tile_for_open = tile.clone();
    click.connect_released(move |_, n, _, _| {
        if n == 2 {
            if let Err(e) = open(&item_c) {
                crate::dialogs::error(&tile_for_open, &e);
            }
        }
    });
    // Right-click opens the item menu (Open / Rename / Remove reference).
    let right = gtk::GestureClick::new();
    right.set_button(gdk::BUTTON_SECONDARY);
    let tile_menu = tile.clone();
    right.connect_released(move |_, _, _, _| on_menu(&tile_menu));
    tile.add_controller(click);
    tile.add_controller(right);

    let child = gtk::FlowBoxChild::new();
    child.set_child(Some(&tile));
    child
}
