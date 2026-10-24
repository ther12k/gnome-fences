// Small modal dialogs: text prompt, error box, application chooser.

use gtk4::prelude::*;
use gtk4::{self as gtk, gio, glib};
use std::cell::RefCell;
use std::rc::Rc;

pub fn prompt_text(
    parent: &gtk::Window,
    title: &str,
    heading: &str,
    initial: &str,
) -> Option<String> {
    let dialog = gtk::Dialog::with_buttons(
        Some(title),
        Some(parent),
        gtk::DialogFlags::MODAL | gtk::DialogFlags::DESTROY_WITH_PARENT,
        &[
            ("Add", gtk::ResponseType::Accept),
            ("Cancel", gtk::ResponseType::Cancel),
        ],
    );
    dialog.set_default_response(gtk::ResponseType::Accept);

    let entry = gtk::Entry::builder()
        .text(initial)
        .activates_default(true)
        .margin_top(12)
        .margin_bottom(12)
        .margin_start(12)
        .margin_end(12)
        .build();

    let heading_label = gtk::Label::builder()
        .label(heading)
        .xalign(0.0)
        .margin_top(12)
        .margin_start(12)
        .margin_end(12)
        .build();

    let content = dialog.content_area();
    content.append(&heading_label);
    content.append(&entry);

    let loop_ = glib::MainLoop::new(None, false);
    let result = Rc::new(RefCell::new(None));
    let loop_c = loop_.clone();
    let res_c = result.clone();
    dialog.connect_response(move |d, resp| {
        if resp == gtk::ResponseType::Accept {
            *res_c.borrow_mut() = Some(entry.text().to_string());
        }
        d.close();
        loop_c.quit();
    });
    dialog.show();
    loop_.run();
    let res = result.borrow().clone();
    res
}

pub fn error(parent: &impl IsA<gtk::Widget>, message: &str) {
    let win = parent.root().and_then(|r| r.downcast::<gtk::Window>().ok());
    let dialog = gtk::Dialog::with_buttons(
        Some("gnome-fences"),
        win.as_ref(),
        gtk::DialogFlags::MODAL | gtk::DialogFlags::DESTROY_WITH_PARENT,
        &[("Close", gtk::ResponseType::Close)],
    );
    let label = gtk::Label::builder()
        .label(message)
        .wrap(true)
        .max_width_chars(48)
        .margin_top(12)
        .margin_bottom(12)
        .margin_start(16)
        .margin_end(16)
        .build();
    dialog.content_area().append(&label);
    let loop_ = glib::MainLoop::new(None, false);
    let loop_c = loop_.clone();
    dialog.connect_response(move |d, _| {
        d.close();
        loop_c.quit();
    });
    dialog.show();
    loop_.run();
}

pub fn choose_app(parent: &gtk::Window) -> Option<(String, String)> {
    let dialog = gtk::Dialog::with_buttons(
        Some("Add application"),
        Some(parent),
        gtk::DialogFlags::MODAL | gtk::DialogFlags::DESTROY_WITH_PARENT,
        &[("Cancel", gtk::ResponseType::Cancel)],
    );
    dialog.set_default_size(420, 480);

    let scrolled = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .margin_top(8)
        .margin_bottom(8)
        .build();
    let list = gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .css_classes(["boxed-list"])
        .build();

    let choice = Rc::new(RefCell::new(None));
    let loop_ = glib::MainLoop::new(None, false);

    let mut apps: Vec<gio::DesktopAppInfo> = gio::AppInfo::all()
        .into_iter()
        .filter_map(|a| a.downcast::<gio::DesktopAppInfo>().ok())
        .filter(|a| a.should_show())
        .collect();
    apps.sort_by_key(|a| a.display_name().to_string());

    let loop_c = loop_.clone();
    let dialog_c = dialog.clone();
    for app in apps {
        let Some(id_g) = app.id() else { continue };
        let id = id_g.to_string();
        let name = app.display_name().to_string();
        let row = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(10)
            .margin_top(6)
            .margin_bottom(6)
            .margin_start(10)
            .margin_end(10)
            .build();
        let icon = gtk::Image::builder().pixel_size(24).build();
        if let Some(gicon) = app.icon() {
            icon.set_from_gicon(&gicon);
        }
        row.append(&icon);
        row.append(&gtk::Label::builder().label(&name).hexpand(true).xalign(0.0).build());
        let btn = gtk::Button::with_label("Add");
        let id_c = id.clone();
        let name_c = name.clone();
        let choice_c = choice.clone();
        let loop_cc = loop_c.clone();
        let dialog_cc = dialog_c.clone();
        btn.connect_clicked(move |_| {
            *choice_c.borrow_mut() = Some((id_c.clone(), name_c.clone()));
            dialog_cc.close();
            loop_cc.quit();
        });
        row.append(&btn);
        list.append(&row);
    }

    scrolled.set_child(Some(&list));
    dialog.content_area().append(&scrolled);
    let loop_c2 = loop_.clone();
    dialog.connect_response(move |d, _| {
        d.close();
        loop_c2.quit();
    });
    dialog.show();
    loop_.run();
    let chosen = choice.borrow().clone();
    chosen
}
