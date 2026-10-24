// gnome-fences — PecoFence-style desktop organizer for GNOME.
//
// One process owns every fence window. The fences@ther12k shell extension
// (separate component) spawns us, stacks our windows below ordinary windows,
// and pushes geometry back over D-Bus.

pub mod dbus;
pub mod dialogs;
pub mod fence;
pub mod items;
pub mod model;
pub mod settings;

use gtk4::prelude::*;
use gtk4::{self as gtk, glib};
use model::State;
use std::cell::RefCell;
use std::rc::Rc;

const APP_ID: &str = "io.github.ther12k.Fences";

const CSS: &str = r#"
window.fence {
    background: transparent;
    border-radius: 14px;
}
.fence-header {
    min-height: 30px;
    border-radius: 14px 14px 0 0;
    padding: 2px 4px 2px 8px;
    font-weight: 700;
    color: rgba(255, 255, 255, 0.92);
}
.fence-body {
    border-radius: 0 0 14px 14px;
    background: rgba(30, 32, 38, 0.72);
}
.fence-item {
    border-radius: 10px;
    padding: 4px;
}
.fence-item:hover {
    background: rgba(255, 255, 255, 0.08);
}
.accent-0 .fence-header { background: rgba( 62,  96, 170, 0.85); }
.accent-1 .fence-header { background: rgba( 62, 140, 110, 0.85); }
.accent-2 .fence-header { background: rgba(140,  80, 150, 0.85); }
.accent-3 .fence-header { background: rgba(170, 110,  50, 0.85); }
.accent-4 .fence-header { background: rgba( 60, 120, 140, 0.85); }
.accent-5 .fence-header { background: rgba(150,  60,  70, 0.85); }
.accent-6 .fence-header { background: rgba( 90,  90, 100, 0.85); }
.accent-7 .fence-header { background: rgba(120, 105,  50, 0.85); }
.accent-0 .fence-body { border: 1px solid rgba( 62,  96, 170, 0.7); }
.accent-1 .fence-body { border: 1px solid rgba( 62, 140, 110, 0.7); }
.accent-2 .fence-body { border: 1px solid rgba(140,  80, 150, 0.7); }
.accent-3 .fence-body { border: 1px solid rgba(170, 110,  50, 0.7); }
.accent-4 .fence-body { border: 1px solid rgba( 60, 120, 140, 0.7); }
.accent-5 .fence-body { border: 1px solid rgba(150,  60,  70, 0.7); }
.accent-6 .fence-body { border: 1px solid rgba( 90,  90, 100, 0.7); }
.accent-7 .fence-body { border: 1px solid rgba(120, 105,  50, 0.7); }
"#;

fn main() -> glib::ExitCode {
    let app = gtk::Application::builder()
        .application_id(APP_ID)
        .build();

    app.connect_activate(activate);
    app.set_accels_for_action("win.undo", &["<Primary>z"]);
    app.run()
}

fn activate(app: &gtk::Application) {
    std::mem::forget(app.hold());

    load_css(app);

    let state = Rc::new(State {
        layout: RefCell::new(model::load()),
        undo: RefCell::new(Vec::new()),
        windows: RefCell::new(Vec::new()),
        app: app.clone(),
    });

    if state.layout.borrow().groups.is_empty() {
        state
            .layout
            .borrow_mut()
            .groups
            .push(model::Group::new("Fences", 0));
    }

    let groups = state.layout.borrow().groups.clone();
    for group in groups {
        let w = fence::build_fence(&state, &group);
        state.windows.borrow_mut().push(w.clone());
        w.window.present();
    }

    state.save();
    dbus::start(&state);
    dbus::place_all(&state);

    let state_c = state.clone();
    let app_c = app.clone();
    glib::unix_signal_add_local(
        15, // SIGTERM
        move || {
            state_c.save();
            app_c.quit();
            glib::ControlFlow::Continue
        },
    );
}

fn load_css(app: &gtk::Application) {
    let provider = gtk::CssProvider::new();
    provider.load_from_data(CSS);
    if let Some(display) = app
        .active_window()
        .map(|w| gtk4::prelude::WidgetExt::display(&w))
        .or_else(gtk::gdk::Display::default)
    {
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
}

pub fn undo_and_rebuild(state: &model::SharedState) {
    let snapshot = state.undo.borrow_mut().pop();
    let Some(snapshot) = snapshot else { return };
    *state.layout.borrow_mut() = snapshot;
    state.save();
    rebuild_all(state);
}

pub fn restore_backup_and_rebuild(state: &model::SharedState) -> bool {
    match model::restore_backup() {
        Some(layout) => {
            state.push_undo();
            *state.layout.borrow_mut() = layout;
            state.save();
            rebuild_all(state);
            true
        }
        None => false,
    }
}

fn rebuild_all(state: &model::SharedState) {
    for w in state.windows.borrow_mut().drain(..) {
        w.window.destroy();
    }
    let groups = state.layout.borrow().groups.clone();
    for group in groups {
        let w = fence::build_fence(state, &group);
        state.windows.borrow_mut().push(w.clone());
        w.window.present();
    }
    dbus::place_all(state);
}
