//! logind and desktop screen-lock notifications. Unsupported sessions fail closed on focus loss.
use nsh_platform_linux::gtk4::{self as gtk, gio, glib, prelude::*};
use safay_app::controller::Controller;
use std::{cell::RefCell, sync::Arc, time::Duration};
thread_local! {static SUBSCRIPTIONS:RefCell<Vec<gio::SignalSubscription>>=const{RefCell::new(Vec::new())};}
fn keep(subscription: gio::SignalSubscription) {
    SUBSCRIPTIONS.with(|list| list.borrow_mut().push(subscription));
}
pub fn configure(controller: Arc<Controller>) {
    glib::MainContext::default().invoke(move || {
        glib::MainContext::default().spawn_local(async move {
            let mut reliable = false;
            if let Ok(bus) = gio::bus_get_future(gio::BusType::System).await {
                reliable = bus
                    .call_future(
                        Some("org.freedesktop.DBus"),
                        "/org/freedesktop/DBus",
                        "org.freedesktop.DBus",
                        "GetNameOwner",
                        Some(&("org.freedesktop.login1",).to_variant()),
                        None,
                        gio::DBusCallFlags::NONE,
                        2000,
                    )
                    .await
                    .is_ok();
                for (interface, member) in [
                    ("org.freedesktop.login1.Session", "Lock"),
                    ("org.freedesktop.login1.Manager", "PrepareForSleep"),
                    ("org.freedesktop.DBus.Properties", "PropertiesChanged"),
                ] {
                    let c = controller.clone();
                    keep(bus.subscribe_to_signal(
                        Some("org.freedesktop.login1"),
                        Some(interface),
                        Some(member),
                        None,
                        None,
                        gio::DBusSignalFlags::NONE,
                        move |signal| {
                            let lock = match signal.signal_name {
                                "Lock" => true,
                                "PrepareForSleep" => signal.parameters.get::<(bool,)>().is_some_and(|v| v.0),
                                "PropertiesChanged" => signal
                                    .parameters
                                    .get::<(String, std::collections::HashMap<String, glib::Variant>, Vec<String>)>()
                                    .is_some_and(|(_, values, _)| {
                                        values.get("LockedHint").and_then(|value| value.get::<bool>()).unwrap_or(false)
                                    }),
                                _ => false,
                            };
                            if lock {
                                c.request_lock();
                            }
                        },
                    ));
                }
            }
            if let Ok(bus) = gio::bus_get_future(gio::BusType::Session).await {
                for interface in ["org.freedesktop.ScreenSaver", "org.gnome.ScreenSaver"] {
                    let c = controller.clone();
                    keep(bus.subscribe_to_signal(
                        None,
                        Some(interface),
                        Some("ActiveChanged"),
                        None,
                        None,
                        gio::DBusSignalFlags::NONE,
                        move |signal| {
                            if signal.parameters.get::<(bool,)>().is_some_and(|v| v.0) {
                                c.request_lock();
                            }
                        },
                    ));
                }
            }
            if !reliable {
                foreground_fallback(controller);
            }
            if let Some(app) = gio::Application::default().and_then(|app| app.downcast::<gtk::Application>().ok()) {
                for window in app.windows() {
                    window.set_size_request(800, 600);
                }
            }
        });
    });
}
fn foreground_fallback(controller: Arc<Controller>) {
    let mut active = true;
    glib::timeout_add_local(Duration::from_millis(100), move || {
        let current = gio::Application::default()
            .and_then(|app| app.downcast::<gtk::Application>().ok())
            .is_some_and(|app| app.windows().iter().any(|window| window.is_active()));
        if active && !current {
            controller.request_lock();
        }
        active = current;
        glib::ControlFlow::Continue
    });
}
