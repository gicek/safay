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
            // No least window size on Linux: tiling compositors give a window the tile they choose and do not honor
            // one, so content laid out wider than the tile is cut off at its right edge. The layout narrows instead
            // (the sidebar folds away under 640 pt, dialogs fit the window).
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
/// Hands this launch to the Safay already running, as a second launch of a single-instance app does (GApplication's
/// `command-line`, keyed on the app's id): the running one reports it to the app (`desktop::install`). Whether one
/// was running and took it; when none is, the name is not kept, so the caller can wait for the instance lock.
pub fn hand_to_running() -> bool {
    let app = gio::Application::new(Some(super::APP_ID), gio::ApplicationFlags::HANDLES_COMMAND_LINE);
    if app.register(None::<&gio::Cancellable>).is_err() || !app.is_remote() {
        return false;
    }
    let args: Vec<String> = std::env::args().collect();
    app.run_with_args(&args);
    true
}

/// Gives the window titled `title` the app's color scheme for its menus: NSH styles a menu dark under `.nsh-dark`, a class
/// it gives a window of the app's own only when the app's environment changes, not as the window opens. The tray panel
/// opened while the main window is in the tray saw no change, and its menus came out light in a dark app. The scheme is
/// GTK's own `gtk-application-prefer-dark-theme`, which NSH keeps as the app's, and the class follows it while the window
/// lives. The window is made on the main thread after it is asked for: it is looked for there for a moment.
pub fn follow_scheme(title: &'static str) {
    glib::MainContext::default().invoke(move || {
        let mut tries = 0;
        glib::timeout_add_local(Duration::from_millis(30), move || {
            tries += 1;
            let window = gio::Application::default()
                .and_then(|app| app.downcast::<gtk::Application>().ok())
                .and_then(|app| app.windows().into_iter().find(|w| w.title().as_deref() == Some(title)));
            let Some(window) = window else {
                return if tries < 40 { glib::ControlFlow::Continue } else { glib::ControlFlow::Break };
            };
            let Some(settings) = gtk::Settings::default() else { return glib::ControlFlow::Break };
            let apply = |window: &gtk::Window, settings: &gtk::Settings| {
                let dark = settings.is_gtk_application_prefer_dark_theme();
                window.remove_css_class(if dark { "nsh-light" } else { "nsh-dark" });
                window.add_css_class(if dark { "nsh-dark" } else { "nsh-light" });
            };
            apply(&window, &settings);
            let weak = window.downgrade();
            let handler = settings.connect_gtk_application_prefer_dark_theme_notify(move |settings| {
                if let Some(window) = weak.upgrade() {
                    apply(&window, settings);
                }
            });
            // The handler goes with the window.
            let handler = std::cell::Cell::new(Some(handler));
            window.connect_destroy(move |_| {
                if let (Some(handler), Some(settings)) = (handler.take(), gtk::Settings::default()) {
                    settings.disconnect(handler);
                }
            });
            glib::ControlFlow::Break
        });
    });
}
