// D-Bus wiring.
//
// App side: owns io.github.ther12k.Fences on the session bus and serves
// SaveGeometry(id, x, y, w, h) — the shell extension is the only component
// that knows real window positions on Wayland, so it pushes them to us.
//
// Extension side: we call StackAll() and Place(id, x, y, w, h) on
// io.github.ther12k.Fences.Integration. Place retries: right after
// presenting, the shell may not have mapped our window yet.

use crate::model::SharedState;
use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use std::cell::RefCell;
use std::time::Duration;

pub const APP_BUS: &str = "io.github.ther12k.Fences";
pub const APP_PATH: &str = "/io/github/ther12k/Fences/app";
pub const APP_IFACE: &str = "io.github.ther12k.Fences.App";

pub const EXT_BUS: &str = "io.github.ther12k.Fences.Integration";
pub const EXT_PATH: &str = "/io/github/ther12k/Fences/integration";
pub const EXT_IFACE: &str = "io.github.ther12k.Fences.Integration";

const APP_IFACE_XML: &str = r#"
<node>
  <interface name="io.github.ther12k.Fences.App">
    <method name="SaveGeometry">
      <arg name="id" type="s" direction="in"/>
      <arg name="x" type="i" direction="in"/>
      <arg name="y" type="i" direction="in"/>
      <arg name="w" type="i" direction="in"/>
      <arg name="h" type="i" direction="in"/>
    </method>
  </interface>
</node>"#;

thread_local! {
    static EXT_PROXY: RefCell<Option<gio::DBusProxy>> = const { RefCell::new(None) };
}

pub fn start(state: &SharedState) {
    let node = gio::DBusNodeInfo::for_xml(APP_IFACE_XML).expect("valid interface xml");
    let iface = node.lookup_interface(APP_IFACE).expect("interface present");

    let state_c = state.clone();
    let vtable = gio::DBusInterfaceVTable::builder(
        move |_conn, _sender, _path, _iface, method, params, invocation| {
            if method == "SaveGeometry" {
                if let Some(p) = params {
                    if let Ok((id, x, y, w, h)) = p.get::<(String, i32, i32, i32, i32)>() {
                        let mut layout = state_c.layout.borrow_mut();
                        if let Some(g) = layout.group_mut(&id) {
                            g.x = x;
                            g.y = y;
                            g.w = w;
                            g.h = h;
                        }
                        drop(layout);
                        state_c.save();
                    }
                }
            }
            invocation.return_value(None);
        },
    )
    .build();

    gio::bus_own_name(
        gio::BusType::Session,
        APP_BUS,
        gio::BusNameOwnerFlags::NONE,
        move |conn, _name| {
            let _ = conn.register_object(APP_PATH, &iface, &vtable);
        },
        |_conn, _name| {
            // name acquired: start talking to the extension
            connect_ext_proxy();
        },
        move |_conn, _name| {
            // name lost
        },
    );
}

fn connect_ext_proxy() {
    let fut = gio::DBusProxy::for_bus_future(
        gio::BusType::Session,
        gio::DBusProxyFlags::NONE,
        None,
        EXT_BUS,
        EXT_PATH,
        EXT_IFACE,
    );
    glib::spawn_future_local(async move {
        match fut.await {
            Ok(proxy) => EXT_PROXY.with(|c| *c.borrow_mut() = Some(proxy)),
            Err(e) => eprintln!(
                "gnome-fences: no integration extension on the bus ({e}); \
                 windows will run without below-stacking or saved positions"
            ),
        }
    });
}

/// Ask the extension to stack our existing windows below everything.
pub fn stack_all() {
    EXT_PROXY.with(|c| {
        if let Some(proxy) = &*c.borrow() {
            let proxy = proxy.clone();
            glib::spawn_future_local(async move {
                let _ = proxy
                    .call("StackAll", None, gio::DBusCallFlags::NONE, 2000, None::<&gio::Cancellable>)
                    .await;
            });
        }
    });
}

/// Place one fence window at its stored geometry, retrying while the window
/// is still being mapped by the shell.
pub fn stack_and_place(_state: &SharedState, group: &crate::model::Group) {
    let id = group.id.clone();
    let (x, y, w, h) = (group.x, group.y, group.w, group.h);
    glib::spawn_future_local(async move {
        for _ in 0..6 {
            let proxy_opt = EXT_PROXY.with(|c| c.borrow().clone());
            let Some(proxy) = proxy_opt else { return };
            let args = (id.clone(), x, y, w, h).to_variant();
            let res = proxy
                .call("Place", Some(&args), gio::DBusCallFlags::NONE, 2000, None::<&gio::Cancellable>)
                .await;
            if let Ok(v) = res {
                if let Ok((placed,)) = v.get::<(bool,)>() {
                    if placed {
                        return;
                    }
                }
            }
            glib::timeout_future(Duration::from_millis(400)).await;
        }
    });
}

/// Restore geometry for every fence (called once at startup).
pub fn place_all(state: &SharedState) {
    let groups = state.layout.borrow().groups.clone();
    for g in groups {
        stack_and_place(state, &g);
    }
    stack_all();
}
