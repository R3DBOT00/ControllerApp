use crate::hardware::{Command, State};
use std::{sync::mpsc::Sender, thread, time::Duration};
pub const BUS: &str = "io.github.controllerapp.ControllerApp";
pub const PATH: &str = "/io/github/controllerapp/ControllerApp";
struct Control {
    sender: Sender<Command>,
    state: State,
}
#[zbus::interface(name = "io.github.controllerapp.ControllerApp")]
impl Control {
    fn show(&self) {
        let _ = self.sender.send(Command::Show(true));
    }
    fn hide(&self) {
        let _ = self.sender.send(Command::Show(false));
    }
    fn toggle(&self) {
        let _ = self.sender.send(Command::Toggle);
    }
    fn quit(&self) {
        let _ = self.sender.send(Command::Stop);
    }
    fn fullscreen(&self, value: bool) {
        if let Ok(mut s) = self.state.lock() {
            s.kwin_monitor = true;
        }
        let _ = self.sender.send(Command::Fullscreen(value));
    }
    fn status(&self) -> String {
        let s = self.state.lock().unwrap();
        format!(
            "connected={} visible={} mapped={} enabled={} fullscreen={} ticks={} events={} device={:?} accessibility={} kwin={} controller_active={} desktop_input_devices={}",
            s.view.connected,
            s.view.visible,
            s.mapped,
            s.view.enabled,
            s.view.game,
            s.ticks,
            s.events,
            s.device,
            s.focus_monitor,
            s.kwin_monitor,
            s.view.controller_active,
            s.desktop_input_devices
        )
    }
}
pub struct BusGuard {
    pub connection: zbus::blocking::Connection,
    script: bool,
}
pub fn start(
    sender: Sender<Command>,
    state: State,
) -> Result<BusGuard, Box<dyn std::error::Error>> {
    let connection = zbus::blocking::connection::Builder::session()?
        .name(BUS)?
        .serve_at(
            PATH,
            Control {
                sender,
                state: state.clone(),
            },
        )?
        .build()?;
    let script = load_kwin(&connection)
        .inspect_err(|e| eprintln!("KWin monitoring unavailable: {e}"))
        .is_ok();
    let layout_connection = connection.clone();
    thread::Builder::new()
        .name("keyboard-layout".into())
        .spawn(move || {
            let Ok(proxy) = zbus::blocking::Proxy::new(
                &layout_connection,
                "org.kde.keyboard",
                "/Layouts",
                "org.kde.KeyboardLayouts",
            ) else {
                return;
            };
            let Ok(signals) = proxy.receive_signal("layoutChanged") else {
                return;
            };
            if let Ok(group) = proxy.call::<_, _, u32>("getLayout", &())
                && let Ok(mut s) = state.lock()
            {
                s.layout_group = i32::try_from(group).ok();
            }
            for signal in signals {
                if let Ok((group,)) = signal.body().deserialize::<(u32,)>()
                    && let Ok(mut s) = state.lock()
                {
                    s.layout_group = i32::try_from(group).ok();
                }
            }
        })?;
    Ok(BusGuard { connection, script })
}
fn load_kwin(connection: &zbus::blocking::Connection) -> Result<(), Box<dyn std::error::Error>> {
    let runtime = std::env::var("XDG_RUNTIME_DIR")?;
    let script_path = std::path::Path::new(&runtime).join("controllerapp-fullscreen.js");
    use std::{io::Write, os::unix::fs::OpenOptionsExt};
    // Runtime dir is private to this login; refuse pre-existing symlinks.
    if script_path
        .symlink_metadata()
        .is_ok_and(|m| m.file_type().is_symlink())
    {
        return Err("script path is a symlink".into());
    }
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&script_path)?;
    file.write_all(include_bytes!("../packaging/kwin.js"))?;
    let proxy = zbus::blocking::Proxy::new(
        connection,
        "org.kde.KWin",
        "/Scripting",
        "org.kde.kwin.Scripting",
    )?;
    let _: bool = proxy
        .call("unloadScript", &("controllerapp-fullscreen",))
        .unwrap_or(false);
    let id: i32 = proxy.call(
        "loadScript",
        &(
            script_path.to_string_lossy().as_ref(),
            "controllerapp-fullscreen",
        ),
    )?;
    if id < 0 {
        return Err("KWin rejected the fullscreen script".into());
    }
    let run = zbus::blocking::Proxy::new(
        connection,
        "org.kde.KWin",
        format!("/Scripting/Script{id}"),
        "org.kde.kwin.Script",
    )?;
    run.call::<_, _, ()>("run", &())?;
    Ok(())
}
impl Drop for BusGuard {
    fn drop(&mut self) {
        if self.script
            && let Ok(p) = zbus::blocking::Proxy::new(
                &self.connection,
                "org.kde.KWin",
                "/Scripting",
                "org.kde.kwin.Scripting",
            )
        {
            let _: Result<bool, _> = p.call("unloadScript", &("controllerapp-fullscreen",));
        }
    }
}
pub fn remote(method: &str) -> Result<String, Box<dyn std::error::Error>> {
    let conn = zbus::blocking::Connection::session()?;
    let p = zbus::blocking::Proxy::new(&conn, BUS, PATH, BUS)?;
    if method == "Status" {
        Ok(p.call(method, &())?)
    } else {
        p.call::<_, _, ()>(method, &())?;
        Ok(String::new())
    }
}
pub fn accessibility(sender: Sender<Command>, shared: State) {
    thread::Builder::new()
        .name("editable-focus".into())
        .spawn(move || {
            loop {
                let result = futures_lite::future::block_on(async {
                    use atspi::{
                        AccessibilityConnection, State as AState,
                        events::object::StateChangedEvent, proxy::accessible::AccessibleProxy,
                    };
                    use futures_lite::StreamExt;
                    // GTK/Qt publish focus events only when the session accessibility bus
                    // is enabled. We query roles/states, never application text contents.
                    let session = zbus::Connection::session().await?;
                    let status = zbus::Proxy::new(
                        &session,
                        "org.a11y.Bus",
                        "/org/a11y/bus",
                        "org.a11y.Status",
                    )
                    .await?;
                    status.set_property("IsEnabled", &true).await?;
                    let connection = AccessibilityConnection::new().await?;
                    connection.register_event::<StateChangedEvent>().await?;
                    if let Ok(mut s) = shared.lock() {
                        s.focus_monitor = true;
                    }
                    let stream = connection.event_stream();
                    futures_lite::pin!(stream);
                    let mut focused = None;
                    while let Some(event) = stream.next().await {
                        let Ok(event) = event else {
                            continue;
                        };
                        let Ok(event) = StateChangedEvent::try_from(event) else {
                            continue;
                        };
                        if event.state != AState::Focused {
                            continue;
                        }
                        let identity = event.item.clone();
                        if !event.enabled {
                            if focused.as_ref() == Some(&identity) {
                                focused = None;
                                let _ = sender.send(Command::Focus {
                                    editable: false,
                                    protected: false,
                                });
                            }
                            continue;
                        }
                        let Some(name) = event.item.name().cloned() else {
                            continue;
                        };
                        let proxy = AccessibleProxy::builder(connection.connection())
                            .destination(name)?
                            .path(event.item.path().clone())?
                            .build()
                            .await?;
                        // Query errors are not a focus change and must not kill the monitor.
                        if let Ok(states) = proxy.get_state().await {
                            if focused.as_ref() != Some(&identity) {
                                let _ = sender.send(Command::Focus {
                                    editable: false,
                                    protected: false,
                                });
                            }
                            focused = Some(identity);
                            let protected = proxy
                                .get_role()
                                .await
                                .is_ok_and(|role| role == atspi::Role::PasswordText);
                            let _ = sender.send(Command::Focus {
                                editable: states.contains(AState::Editable),
                                protected,
                            });
                        }
                    }
                    Ok::<(), Box<dyn std::error::Error + Send + Sync>>(())
                });
                if let Ok(mut s) = shared.lock() {
                    s.focus_monitor = false;
                }
                if let Err(e) = result {
                    eprintln!("Accessibility: {e}; retrying in 5s");
                }
                thread::sleep(Duration::from_secs(5));
            }
        })
        .expect("accessibility thread");
}
