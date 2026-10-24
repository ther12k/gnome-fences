// D-Bus wiring via zbus.
//
// App side: serves io.github.ther12k.Fences.App at /io/github/ther12k/Fences/app
// SaveGeometry(id, x, y, w, h) — the shell extension pushes positions on Wayland.
// Receives via async_channel and dispatches into the GTK main loop via glib::spawn_future_local.
//
// Extension side: calls StackAll() and Place(id, x, y, w, h) on
// io.github.ther12k.Fences.Integration.

use crate::model::SharedState;
use gtk4::glib;
use std::time::Duration;
use zbus::{connection::Builder, interface, proxy, Connection};

#[proxy(
    default_service = "io.github.ther12k.Fences.Integration",
    default_path = "/io/github/ther12k/Fences/integration",
    interface = "io.github.ther12k.Fences.Integration"
)]
pub trait Integration {
    fn stack_all(&self) -> zbus::Result<()>;
    fn place(&self, fence_id: &str, x: i32, y: i32, w: i32, h: i32) -> zbus::Result<bool>;
    fn quit(&self) -> zbus::Result<()>;
}

struct GeometryMsg {
    id: String,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
}

struct AppService {
    tx: async_channel::Sender<GeometryMsg>,
}

#[interface(name = "io.github.ther12k.Fences.App")]
impl AppService {
    async fn save_geometry(&self, id: String, x: i32, y: i32, w: i32, h: i32) {
        let _ = self.tx.send(GeometryMsg { id, x, y, w, h }).await;
    }
}

pub fn start(state: &SharedState) {
    let (tx, rx) = async_channel::unbounded::<GeometryMsg>();
    let service = AppService { tx };

    // Poll receiver on the GTK main thread via glib
    let state_c = state.clone();
    glib::spawn_future_local(async move {
        while let Ok(msg) = rx.recv().await {
            let mut layout = state_c.layout.borrow_mut();
            if let Some(g) = layout.group_mut(&msg.id) {
                g.x = msg.x;
                g.y = msg.y;
                g.w = msg.w;
                g.h = msg.h;
            }
            drop(layout);
            state_c.save();
        }
    });

    std::thread::spawn(move || {
        let fut = async {
            let conn = Builder::session()?
                .name("io.github.ther12k.Fences.App")?
                .serve_at("/io/github/ther12k/Fences/app", service)?
                .build()
                .await;
            match conn {
                Ok(_c) => {
                    eprintln!("gnome-fences: D-Bus service registered at io.github.ther12k.Fences.App");
                    futures_lite::future::pending::<()>().await;
                    Ok::<(), zbus::Error>(())
                }
                Err(e) => {
                    eprintln!("gnome-fences: failed to register D-Bus service: {e}");
                    Err(e)
                }
            }
        };
        let _ = async_io::block_on(fut);
    });
}

pub fn stack_all() {
    std::thread::spawn(|| {
        let fut = async {
            let conn = Connection::session().await?;
            let proxy = IntegrationProxy::new(&conn).await?;
            proxy.stack_all().await?;
            Ok::<(), zbus::Error>(())
        };
        let _ = async_io::block_on(fut);
    });
}

pub fn stack_and_place(_state: &SharedState, group: &crate::model::Group) {
    let id = group.id.clone();
    let (x, y, w, h) = (group.x, group.y, group.w, group.h);
    std::thread::spawn(move || {
        let fut = async {
            let conn = Connection::session().await?;
            let proxy = IntegrationProxy::new(&conn).await?;
            for _ in 0..6 {
                if let Ok(true) = proxy.place(&id, x, y, w, h).await {
                    return Ok(());
                }
                async_io::Timer::after(Duration::from_millis(400)).await;
            }
            Ok::<(), zbus::Error>(())
        };
        let _ = async_io::block_on(fut);
    });
}

pub fn place_all(state: &SharedState) {
    let groups = state.layout.borrow().groups.clone();
    for g in groups {
        stack_and_place(state, &g);
    }
    stack_all();
}
