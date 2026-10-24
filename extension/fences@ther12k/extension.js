// fences@ther12k — GNOME Shell 46 (ESM)
//
// Integration layer only. Everything visible inside a fence panel belongs to
// the gnome-fences application; this extension only manages how the
// application's windows participate in the desktop:
//   - spawn the app on enable, kill it on disable, respawn on crash
//   - identify windows by process ownership (pid) + title prefix, never by
//     window title or app id alone
//   - stack: above the wallpaper, below ordinary windows, all workspaces,
//     out of alt-tab / taskbar
//   - push window geometry back to the app on monitor changes and on disable
//   - expose a small D-Bus API the app uses to place its windows (clients
//     cannot position themselves on Wayland; only the compositor can)

import GLib from 'gi://GLib';
import Gio from 'gi://Gio';

const APP_BIN = 'gnome-fences';
const BUS_NAME = 'io.github.ther12k.Fences.Integration';
const BUS_PATH = '/io/github/ther12k/Fences/integration';
const APP_BUS_NAME = 'io.github.ther12k.Fences.App';
const APP_BUS_PATH = '/io/github/ther12k/Fences/app';
const APP_IFACE = 'io.github.ther12k.Fences.App';
const TITLE_PREFIX = 'gnome-fences:';

const MAX_RESPAWNS = 3;
const RESPAWN_DELAY_MS = 2000;
const RESPAWN_RESET_SECONDS = 60;

const IntegrationIface = `
<node>
  <interface name="io.github.ther12k.Fences.Integration">
    <method name="StackAll"/>
    <method name="Place">
      <arg name="fence_id" type="s" direction="in"/>
      <arg name="x" type="i" direction="in"/>
      <arg name="y" type="i" direction="in"/>
      <arg name="w" type="i" direction="in"/>
      <arg name="h" type="i" direction="in"/>
      <arg name="placed" type="b" direction="out"/>
    </method>
    <method name="Quit"/>
  </interface>
</node>`;

class Integration {
    constructor() {
        this.enabled = false;
        this.child = null;
        this.busOwnerId = 0;
        this.regId = 0;
        this.windowCreatedId = 0;
        this.monitorsId = 0;
        this.appProxy = null;
        this.respawnCount = 0;
        this.respawnTimeoutId = 0;
        this.uptimeResetId = 0;
        this.ifaceInfo = Gio.DBusNodeInfo.new_for_xml(IntegrationIface);
    }

    // ---- window identification (process ownership, DING-style) ----

    childPid() {
        if (!this.child)
            return -1;
        const id = this.child.get_identifier();
        return id ? parseInt(id, 10) : -1;
    }

    fenceIdOf(metaWindow) {
        const title = metaWindow.get_title();
        if (title && title.startsWith(TITLE_PREFIX))
            return title.slice(TITLE_PREFIX.length);
        return null;
    }

    isOurs(metaWindow) {
        return metaWindow.get_pid() === this.childPid() &&
            this.fenceIdOf(metaWindow) !== null;
    }

    ourWindows() {
        return global.get_window_actors()
            .map(a => a.get_meta_window())
            .filter(mw => this.isOurs(mw));
    }

    // ---- stacking ----

    stackWindow(mw) {
        try { mw.stick(); } catch (e) { log(`fences: stick: ${e}`); }
        // keeps fences out of alt-tab and the taskbar
        try { mw.skip_taskbar = true; } catch (e) { /* property not writable */ }
        try { mw.make_below(); } catch (e) { log(`fences: make_below: ${e}`); }
    }

    stackAll() {
        for (const mw of this.ourWindows())
            this.stackWindow(mw);
    }

    // Clamp the requested rectangle into the nearest monitor (or the primary
    // one when fully offscreen) so panels stay reachable after a monitor is
    // disconnected or the scale changes.
    clampToMonitors(x, y, w, h) {
        const n = global.display.get_n_monitors();
        let target = -1;
        for (let i = 0; i < n; i++) {
            const g = global.display.get_monitor_geometry(i);
            const ix = Math.max(x, g.x) < Math.min(x + w, g.x + g.width);
            const iy = Math.max(y, g.y) < Math.min(y + h, g.y + g.height);
            if (ix && iy) {
                target = i;
                break;
            }
        }
        if (target < 0)
            target = global.display.get_primary_monitor();
        const g = global.display.get_monitor_geometry(target);
        return [
            Math.max(g.x, Math.min(x, g.x + g.width - w)),
            Math.max(g.y, Math.min(y, g.y + g.height - h)),
        ];
    }

    place(fenceId, x, y, w, h) {
        const mw = this.ourWindows().find(m => this.fenceIdOf(m) === fenceId);
        if (!mw)
            return false;
        [x, y] = this.clampToMonitors(x, y, w, h);
        try {
            mw.move_resize_frame(false, x, y, w, h);
        } catch (e) {
            log(`fences: move_resize_frame: ${e}`);
            return false;
        }
        this.stackWindow(mw);
        return true;
    }

    // ---- geometry push (compositor is the only source of window position) ----

    pushGeometries() {
        if (!this.appProxy)
            return;
        for (const mw of this.ourWindows()) {
            const id = this.fenceIdOf(mw);
            if (!id)
                continue;
            const f = mw.get_frame_rect();
            try {
                this.appProxy.call_sync('SaveGeometry',
                    new GLib.Variant('(siiii)', [id, f.x, f.y, f.width, f.height]),
                    Gio.DBusCallFlags.NONE, 1000, null);
            } catch (e) {
                log(`fences: SaveGeometry: ${e}`);
            }
        }
    }

    restoreOffscreen() {
        for (const mw of this.ourWindows()) {
            const f = mw.get_frame_rect();
            let visible = false;
            for (let i = 0; i < global.display.get_n_monitors(); i++) {
                const g = global.display.get_monitor_geometry(i);
                if (f.x + f.width > g.x && f.x < g.x + g.width &&
                    f.y + f.height > g.y && f.y < g.y + g.height) {
                    visible = true;
                    break;
                }
            }
            if (!visible) {
                const [x, y] = this.clampToMonitors(f.x, f.y, f.width, f.height);
                try { mw.move_resize_frame(false, x, y, f.width, f.height); } catch (e) { }
            }
        }
    }

    // ---- application lifecycle ----

    startApp() {
        const path = GLib.find_program_in_path(APP_BIN);
        if (!path) {
            log(`fences: ${APP_BIN} not found in PATH`);
            return;
        }
        try {
            this.child = Gio.Subprocess.newv([path], Gio.SubprocessFlags.NONE);
        } catch (e) {
            log(`fences: spawn failed: ${e}`);
            this.child = null;
            return;
        }
        this.respawnCount = 0;
        if (this.uptimeResetId)
            GLib.source_remove(this.uptimeResetId);
        this.uptimeResetId = GLib.timeout_add_seconds(GLib.PRIORITY_DEFAULT,
            RESPAWN_RESET_SECONDS, () => {
                this.respawnCount = 0;
                this.uptimeResetId = 0;
                return GLib.SOURCE_REMOVE;
            });
        this.child.wait_check_async(null, (proc, res) => {
            let ok = false;
            try {
                ok = proc.wait_check_finish(res);
            } catch (e) {
                log(`fences: app exited: ${e}`);
            }
            if (this.child === proc)
                this.child = null;
            // A clean exit (0) is a deliberate quit; only crash-respawn.
            if (!ok && this.enabled)
                this.scheduleRespawn();
        });
    }

    scheduleRespawn() {
        if (this.respawnCount >= MAX_RESPAWNS) {
            log('fences: too many crashes, giving up until the extension is toggled');
            return;
        }
        this.respawnCount += 1;
        if (this.respawnTimeoutId)
            GLib.source_remove(this.respawnTimeoutId);
        this.respawnTimeoutId = GLib.timeout_add(GLib.PRIORITY_DEFAULT,
            RESPAWN_DELAY_MS, () => {
                this.respawnTimeoutId = 0;
                if (this.enabled && !this.child)
                    this.startApp();
                return GLib.SOURCE_REMOVE;
            });
    }

    stopApp() {
        if (this.uptimeResetId) {
            GLib.source_remove(this.uptimeResetId);
            this.uptimeResetId = 0;
        }
        if (this.respawnTimeoutId) {
            GLib.source_remove(this.respawnTimeoutId);
            this.respawnTimeoutId = 0;
        }
        const child = this.child;
        this.child = null;
        if (child) {
            try { child.send_signal(GLib.ProcessSignal.TERM); } catch (e) { }
        }
    }

    // ---- D-Bus server (methods the app calls into the shell) ----

    ownBus() {
        this.busOwnerId = Gio.bus_own_name(Gio.BusType.SESSION, BUS_NAME,
            Gio.BusNameOwnerFlags.NONE,
            (conn) => this.onBusAcquired(conn),
            null,
            () => log('fences: could not own ' + BUS_NAME));
    }

    onBusAcquired(conn) {
        try {
            this.regId = conn.register_object(BUS_PATH, this.ifaceInfo.interfaces[0], {
                handle_method_call: (conn2, sender, path, iface, method, params, invocation) => {
                    if (method === 'StackAll') {
                        this.stackAll();
                        invocation.return_value(null);
                    } else if (method === 'Place') {
                        const [id, x, y, w, h] = params.deepUnpack();
                        const placed = this.place(id, x, y, w, h);
                        invocation.return_value(new GLib.Variant('(b)', [placed]));
                    } else if (method === 'Quit') {
                        this.stopApp();
                        invocation.return_value(null);
                    } else {
                        invocation.return_gerror(new GLib.Error(Gio.IOErrorEnum,
                            Gio.IOErrorEnum.UNKNOWN_METHOD, 'unknown method'));
                    }
                },
            });
        } catch (e) {
            log(`fences: register_object failed: ${e}`);
        }
    }

    watchAppProxy() {
        Gio.DBusProxy.new_for_bus(Gio.BusType.SESSION, Gio.DBusProxyFlags.NONE, null,
            APP_BUS_NAME, APP_BUS_PATH, APP_IFACE, null, (src, res) => {
                try {
                    this.appProxy = Gio.DBusProxy.new_for_bus_finish(res);
                } catch (e) {
                    // app not up yet; pushGeometries() no-ops until it appears
                    this.appProxy = null;
                }
            });
    }

    // ---- extension lifecycle ----

    enable() {
        this.enabled = true;
        this.ownBus();
        this.watchAppProxy();
        this.startApp();

        this.windowCreatedId = global.display.connect('window-created', (_, mw) => {
            if (this.isOurs(mw))
                this.stackWindow(mw);
        });
        this.monitorsId = global.display.connect('monitors-changed', () => {
            this.stackAll();
            this.restoreOffscreen();
            this.pushGeometries();
        });
    }

    disable() {
        this.enabled = false;
        if (this.windowCreatedId) {
            global.display.disconnect(this.windowCreatedId);
            this.windowCreatedId = 0;
        }
        if (this.monitorsId) {
            global.display.disconnect(this.monitorsId);
            this.monitorsId = 0;
        }
        if (this.busOwnerId) {
            Gio.bus_unown_name(this.busOwnerId);
            this.busOwnerId = 0;
        }
        this.appProxy = null;
        this.stopApp();
    }
}

import { Extension } from 'resource:///org/gnome/shell/extensions/extension.js';

export default class FencesExtension extends Extension {
    enable() {
        this._integration = new Integration();
        this._integration.enable();
    }

    disable() {
        if (this._integration) {
            this._integration.disable();
            this._integration = null;
        }
    }
}
