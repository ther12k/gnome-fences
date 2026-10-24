# gnome-fences

A **GNOME-first desktop organizer** inspired by PecoFence, built specifically for Ubuntu GNOME (tested on 24.04 / GNOME Shell 46 on Wayland).

Fences sit directly above the wallpaper and below ordinary windows, holding non-destructive references to files, folders, web links, and installed applications.

## Architecture

Separates desktop integration from the application:

```
┌──────────────────────────────────────────┐
│ GNOME Shell 46 Process                   │
│   extension: fences@ther12k (GJS/ESM)    │
│     - Stacking (make_below, stick)       │
│     - Monitor clamping on display change │
│     - Lifecycle management (pid-based)   │
│     - Window placement via D-Bus         │
└──────────────────┬───────────────────────┘
                   │ Session D-Bus
                   │   io.github.ther12k.Fences
                   │   io.github.ther12k.Fences.Integration
┌──────────────────┴───────────────────────┐
│ gnome-fences Application (Rust + GTK4)   │
│   - One process owns all fence windows   │
│   - Header handle, collapse, color theme │
│   - Tile grid + icon rendering           │
│   - DnD (COPY-only, non-destructive)     │
│   - Layout model + atomic JSON save      │
│   - In-memory undo stack (Ctrl+Z)        │
└──────────────────────────────────────────┘
```

## The Non-Negotiable Rules

1. **Removing an item from a group NEVER deletes the original.**
2. **Moving an item between groups NEVER moves the underlying file.**
3. Drag-and-drop always negotiates `GdkDragAction::COPY` with external applications.

## Building

### Requirements
- Ubuntu 24.04 LTS (or any GNOME 45/46 system)
- Rust 1.75+ (`rustc`, `cargo`)
- `libgtk-4-dev` (>= 4.14), `libglib2.0-dev`
- `build-essential`, `pkg-config`

### Compile
```bash
sudo apt install -y libgtk-4-dev libglib2.0-dev build-essential pkg-config
cargo build --release
```

### Install
```bash
# 1. Install the binary
sudo cp target/release/gnome-fences /usr/local/bin/

# 2. Install the companion extension into your user directory
make install-user-ext

# 3. Enable the extension
gnome-extensions enable fences@ther12k
```

Or run standalone for testing (without below-stacking):
```bash
cargo run
```

## Configuration

Layout state is stored at:
- `~/.config/gnome-fences/layout.json`
- `~/.config/gnome-fences/layout.json.bak` (automatic backup rotated before every save)

## License

Apache-2.0
