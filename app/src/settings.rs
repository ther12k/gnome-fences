// Settings window: an ordinary native window, opened only on request.
// Deliberately minimal for the MVP: manage fences, data folder, backup.

use crate::model::SharedState;
use gtk4::prelude::*;
use gtk4::{self as gtk, gio};
use std::cell::RefCell;

thread_local! {
    static SETTINGS_WIN: RefCell<Option<gtk::Window>> = const { RefCell::new(None) };
}

pub fn show(state: &SharedState) {
    let existing = SETTINGS_WIN.with(|c| c.borrow().clone());
    if let Some(win) = existing {
        win.present();
        return;
    }

    let win = gtk::Window::builder()
        .title("Fences — Settings")
        .default_width(420)
        .default_height(300)
        .resizable(false)
        .build();

    let box_ = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(10)
        .margin_top(16)
        .margin_bottom(16)
        .margin_start(16)
        .margin_end(16)
        .build();

    let count = state.layout.borrow().groups.len();
    box_.append(&gtk::Label::builder()
        .label(format!("Fences — {count} group(s) in ~/.config/gnome-fences/layout.json"))
        .wrap(true)
        .xalign(0.0)
        .build());

    let mk = |label: &str| gtk::Button::with_label(label);

    let b_new = mk("New fence");
    let st = state.clone();
    b_new.connect_clicked(move |_| {
        st.push_undo();
        let count = st.layout.borrow().groups.len();
        let g = crate::model::Group::new(&format!("Fence {}", count + 1), count);
        let id = g.id.clone();
        st.layout.borrow_mut().groups.push(g);
        st.save();
        let layout = st.layout.borrow();
        let group = layout.group(&id).cloned().unwrap();
        drop(layout);
        let w = crate::fence::build_fence(&st, &group);
        st.windows.borrow_mut().push(w.clone());
        w.window.present();
        crate::dbus::stack_and_place(&st, &group);
    });
    box_.append(&b_new);

    let b_folder = mk("Open data folder");
    b_folder.connect_clicked(|_| {
        let dir = crate::model::config_dir();
        let _ = std::fs::create_dir_all(&dir);
        let uri = gio::File::for_path(&dir).uri();
        let _ = gio::AppInfo::launch_default_for_uri(&uri, None::<&gio::AppLaunchContext>);
    });
    box_.append(&b_folder);

    let b_backup = mk("Restore last backup");
    let st = state.clone();
    b_backup.connect_clicked(move |_| {
        if crate::restore_backup_and_rebuild(&st) {
            eprintln!("gnome-fences: backup restored");
        } else {
            eprintln!("gnome-fences: no usable backup found");
        }
    });
    box_.append(&b_backup);

    box_.append(&gtk::Label::builder()
        .label("Removing an item from a fence never deletes the original.\nMoving an item between fences never moves the underlying file.")
        .wrap(true)
        .css_classes(["dim-label"])
        .xalign(0.0)
        .build());

    win.set_child(Some(&box_));
    win.connect_close_request(|_| {
        SETTINGS_WIN.with(|c| *c.borrow_mut() = None);
        gtk::glib::Propagation::Proceed
    });

    SETTINGS_WIN.with(|c| *c.borrow_mut() = Some(win.clone()));
    win.show();
}
