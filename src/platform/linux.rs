//! Linux system services NSH does not offer: locking on logind's and the desktop's screen-lock notifications
//! (unsupported sessions fail closed on focus loss), whether the desktop has a tray, handing a second launch to the
//! running Safay, and the windows' color-scheme class for their menus.
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
/// Whether a name on the session bus has an owner (a running service or app).
fn bus_name_owned(name: &str) -> bool {
    let Ok(bus) = gio::bus_get_sync(gio::BusType::Session, None::<&gio::Cancellable>) else { return false };
    bus.call_sync(
        Some("org.freedesktop.DBus"),
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
        "NameHasOwner",
        Some(&(name,).to_variant()),
        None,
        gio::DBusCallFlags::NONE,
        500,
        None::<&gio::Cancellable>,
    )
    .ok()
    .and_then(|reply| reply.get::<(bool,)>())
    .is_some_and(|(owned,)| owned)
}

/// Whether the desktop shows tray icons now (a StatusNotifierWatcher runs: Plasma, Waybar, Quickshell, GNOME's
/// AppIndicator extension). Without one, a window hidden in the tray could not be brought back.
pub fn has_tray() -> bool {
    bus_name_owned("org.kde.StatusNotifierWatcher")
}

/// Hands this launch to the Safay already running, as a second launch of a single-instance app does (GApplication's
/// `command-line`, keyed on the app's id): the running one reports it to the app (`desktop::install`). Whether one was
/// running and took it. Only a name another process already owns is joined, so this never takes it from a Safay that
/// is still starting.
pub fn hand_to_running() -> bool {
    if !bus_name_owned(super::APP_ID) {
        return false;
    }
    let app = gio::Application::new(Some(super::APP_ID), gio::ApplicationFlags::HANDLES_COMMAND_LINE);
    if app.register(None::<&gio::Cancellable>).is_err() || !app.is_remote() {
        return false;
    }
    let args: Vec<String> = std::env::args().collect();
    app.run_with_args(&args);
    true
}

/// Gives every window of the app its color scheme's class (`nsh-dark`, `nsh-light`), as each opens and as the scheme
/// changes: NSH styles menus by it, and gives it to a window of the app's own only when the app's environment changes,
/// so a window opened without one (the tray panel) would show light menus in a dark app. The scheme is GTK's
/// `gtk-application-prefer-dark-theme`, which NSH keeps as the app's. Installed once; recheck on each NSH upgrade.
pub fn follow_scheme() {
    glib::MainContext::default().invoke(|| {
        let (Some(app), Some(settings)) =
            (gio::Application::default().and_then(|app| app.downcast::<gtk::Application>().ok()), gtk::Settings::default())
        else {
            return;
        };
        fn apply(window: &gtk::Window, dark: bool) {
            window.remove_css_class(if dark { "nsh-light" } else { "nsh-dark" });
            window.add_css_class(if dark { "nsh-dark" } else { "nsh-light" });
        }
        app.connect_window_added(|_, window| {
            apply(window, gtk::Settings::default().is_some_and(|s| s.is_gtk_application_prefer_dark_theme()));
        });
        let weak = app.downgrade();
        settings.connect_gtk_application_prefer_dark_theme_notify(move |settings| {
            if let Some(app) = weak.upgrade() {
                for window in app.windows() {
                    apply(&window, settings.is_gtk_application_prefer_dark_theme());
                }
            }
        });
    });
}
