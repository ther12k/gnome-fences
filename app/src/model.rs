// GTK application state adapter wrapping gnome-fences-core.

pub use gnome_fences_core::*;

use std::cell::RefCell;
use std::rc::Rc;

pub struct State {
    pub layout: RefCell<Layout>,
    pub undo: RefCell<Vec<Layout>>,
    pub windows: RefCell<Vec<Rc<crate::fence::FenceWindow>>>,
    pub app: gtk4::Application,
}

pub type SharedState = Rc<State>;

impl State {
    pub fn push_undo(&self) {
        let mut undo = self.undo.borrow_mut();
        undo.push(self.layout.borrow().clone());
        if undo.len() > UNDO_DEPTH {
            undo.remove(0);
        }
    }

    pub fn save(&self) {
        crate::fence::sync_sizes(self);
        gnome_fences_core::save(&self.layout.borrow());
    }
}

pub fn make_file_item(file: &gtk4::gio::File) -> Option<Item> {
    use gtk4::prelude::*;
    if file.uri_scheme().as_deref() != Some("file") {
        return None;
    }
    let p = file.path()?;
    Some(Item::new_file(&p))
}
