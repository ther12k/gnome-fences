# Acceptance Test Plan — gnome-fences Milestone 1

Every test below must pass before Milestone 2 (automatic classification, live folders, peek overlay) begins.

| # | Area | Action | Expected outcome |
|---|------|--------|------------------|
| 1 | Stacking | Open a browser or terminal over a fence panel | Fence stays beneath the active window; never jumps forward unless dragged by header |
| 2 | Wallpaper-above | Click on empty desktop outside any fence | Clicks register on desktop; GNOME right-click menu appears normally |
| 3 | Input | Click fence header, items, menus, collapse button | All respond immediately; no clicks leak through to the desktop underneath |
| 4 | Alt-Tab | Press Alt+Tab while fences are visible | Fence panels do NOT appear in the switcher; `Settings` window DOES appear when open |
| 5 | Non-destructive Drop | Drag `/home/user/Documents/report.pdf` from Nautilus into a fence | Reference tile appears; `ls -la ~/Documents/report.pdf` shows the source file is identical and unmoved |
| 6 | Non-destructive Remove | Right-click the `report.pdf` tile, click "Remove from group" | Tile disappears from the fence; `~/Documents/report.pdf` still exists on disk |
| 7 | Cross-fence transfer | Drag a tile from Fence A into Fence B | Tile moves from A to B; underlying target remains unmoved |
| 8 | Multi-monitor | Disconnect secondary display or change fractional scale | Panels are clamped back into visible monitor geometry via `clampToMonitors` |
| 9 | Crash recovery | `pkill -9 gnome-fences` | Extension detects exit within 2 seconds and respawns the app; layout restored from `~/.config/gnome-fences/layout.json` |
| 10 | Extension disable | `gnome-extensions disable fences@ther12k` | Extension disconnects signals, kills the child process via SIGTERM, layout is saved atomically, desktop returns to clean state |
| 11 | DING coexistence | Run with Ubuntu Desktop Icons NG enabled | Fences and desktop icons coexist; no click deadzones on empty desktop |
| 12 | Undo | Press Ctrl+Z or choose Menu → Undo | Last item add/remove/fence creation is reverted from the in-memory snapshot ring |
