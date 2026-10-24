// Layout model, atomic persistence with backup rotation, undo snapshots.
//
// The two non-negotiable rules of the project live here implicitly:
// items are *references* (kind + a path/uri/desktop-id string), so nothing
// in this module can ever move or delete a user file.

use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ItemKind {
    File,
    Folder,
    Url,
    App,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Item {
    pub id: String,
    pub label: String,
    pub r#ref: String,
    pub kind: ItemKind,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Group {
    pub id: String,
    pub title: String,
    pub accent: u8,
    pub collapsed: bool,
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub items: Vec<Item>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct Layout {
    pub version: u32,
    pub groups: Vec<Group>,
}

pub const LAYOUT_VERSION: u32 = 1;
pub const ACCENTS: usize = 8;
pub const UNDO_DEPTH: usize = 50;

pub fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

pub fn config_dir() -> PathBuf {
    let base = std::env::var("XDG_CONFIG_HOME")
        .ok()
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
            PathBuf::from(home).join(".config")
        });
    base.join("gnome-fences")
}

pub fn layout_path(dir: &Path) -> PathBuf {
    dir.join("layout.json")
}

pub fn backup_path(dir: &Path) -> PathBuf {
    dir.join("layout.json.bak")
}

impl Item {
    pub fn new_file(path: &Path) -> Item {
        let label = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.to_string_lossy().into_owned());
        let kind = if path.is_dir() {
            ItemKind::Folder
        } else {
            ItemKind::File
        };
        Item {
            id: new_id(),
            label,
            r#ref: path.to_string_lossy().into_owned(),
            kind,
        }
    }

    pub fn new_url(url: &str, label: Option<String>) -> Item {
        Item {
            id: new_id(),
            label: label.unwrap_or_else(|| url.to_string()),
            r#ref: url.to_string(),
            kind: ItemKind::Url,
        }
    }

    pub fn new_app(id: &str, label: String) -> Item {
        Item {
            id: new_id(),
            label,
            r#ref: id.to_string(),
            kind: ItemKind::App,
        }
    }
}

impl Group {
    pub fn new(title: &str, cascade: usize) -> Group {
        Group {
            id: new_id(),
            title: title.to_string(),
            accent: (cascade % ACCENTS) as u8,
            collapsed: false,
            x: 80 + (cascade as i32 % 6) * 40,
            y: 80 + (cascade as i32 % 6) * 40,
            w: 340,
            h: 420,
            items: Vec::new(),
        }
    }
}

impl Layout {
    pub fn group_mut(&mut self, id: &str) -> Option<&mut Group> {
        self.groups.iter_mut().find(|g| g.id == id)
    }

    pub fn group(&self, id: &str) -> Option<&Group> {
        self.groups.iter().find(|g| g.id == id)
    }
}

pub fn load_from_dir(dir: &Path) -> Layout {
    for path in [layout_path(dir), backup_path(dir)] {
        if let Ok(text) = std::fs::read_to_string(&path) {
            match serde_json::from_str::<Layout>(&text) {
                Ok(l) if l.version == LAYOUT_VERSION => return l,
                _ => eprintln!("gnome-fences: unusable layout at {}, trying next", path.display()),
            }
        }
    }
    Layout { version: LAYOUT_VERSION, groups: vec![Group::new("Fences", 0)] }
}

pub fn load() -> Layout {
    load_from_dir(&config_dir())
}

/// Atomic save: previous file becomes the .bak, new content lands via tmp+rename.
pub fn save_to_dir(layout: &Layout, dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let path = layout_path(dir);
    if path.exists() {
        let _ = std::fs::copy(&path, backup_path(dir));
    }
    let tmp = dir.join(".layout.json.tmp");
    let json = serde_json::to_string_pretty(layout)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    std::fs::write(&tmp, json)?;
    std::fs::rename(&tmp, &path)?;
    Ok(())
}

pub fn save(layout: &Layout) {
    let _ = save_to_dir(layout, &config_dir());
}

pub fn restore_backup_from_dir(dir: &Path) -> Option<Layout> {
    let text = std::fs::read_to_string(backup_path(dir)).ok()?;
    serde_json::from_str(&text).ok()
}

pub fn restore_backup() -> Option<Layout> {
    restore_backup_from_dir(&config_dir())
}

/// In-memory undo ring.
pub struct UndoRing {
    snapshots: RefCell<Vec<Layout>>,
}

impl Default for UndoRing {
    fn default() -> Self {
        Self { snapshots: RefCell::new(Vec::new()) }
    }
}

impl UndoRing {
    pub fn push(&self, layout: &Layout) {
        let mut s = self.snapshots.borrow_mut();
        s.push(layout.clone());
        if s.len() > UNDO_DEPTH {
            s.remove(0);
        }
    }

    pub fn pop(&self) -> Option<Layout> {
        self.snapshots.borrow_mut().pop()
    }

    pub fn len(&self) -> usize {
        self.snapshots.borrow().len()
    }

    pub fn is_empty(&self) -> bool {
        self.snapshots.borrow().is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_roundtrip_serialization() {
        let mut layout = Layout { version: 1, groups: vec![] };
        let mut g = Group::new("Work", 0);
        g.items.push(Item::new_file(Path::new("/tmp/doc.pdf")));
        g.items.push(Item::new_url("https://example.com", Some("Example".into())));
        layout.groups.push(g);

        let json = serde_json::to_string_pretty(&layout).unwrap();
        let parsed: Layout = serde_json::from_str(&json).unwrap();
        assert_eq!(layout, parsed);
    }

    #[test]
    fn test_atomic_save_and_backup_rotation() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();

        let l1 = Layout { version: 1, groups: vec![Group::new("First", 0)] };
        save_to_dir(&l1, dir).unwrap();
        assert!(layout_path(dir).exists());
        assert!(!backup_path(dir).exists());

        let l2 = Layout { version: 1, groups: vec![Group::new("Second", 1)] };
        save_to_dir(&l2, dir).unwrap();
        assert!(backup_path(dir).exists());

        let loaded = load_from_dir(dir);
        assert_eq!(loaded.groups[0].title, "Second");

        let backup = restore_backup_from_dir(dir).unwrap();
        assert_eq!(backup.groups[0].title, "First");
    }

    #[test]
    fn test_undo_ring() {
        let ring = UndoRing::default();
        let l1 = Layout { version: 1, groups: vec![Group::new("A", 0)] };
        let l2 = Layout { version: 1, groups: vec![Group::new("B", 1)] };

        ring.push(&l1);
        ring.push(&l2);
        assert_eq!(ring.len(), 2);

        assert_eq!(ring.pop().unwrap().groups[0].title, "B");
        assert_eq!(ring.pop().unwrap().groups[0].title, "A");
        assert!(ring.pop().is_none());
    }

    #[test]
    fn test_non_destructive_invariant() {
        let item = Item::new_file(Path::new("/etc/hosts"));
        assert_eq!(item.kind, ItemKind::File);
        assert_eq!(item.r#ref, "/etc/hosts");
        assert_eq!(item.label, "hosts");
        drop(item);
        assert!(Path::new("/etc/hosts").exists());
    }
}
