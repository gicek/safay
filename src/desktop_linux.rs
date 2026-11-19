//! The Linux desktop integration, on NSH's desktop API (`nsh_core::element::desktop`): the tray icon (a
//! StatusNotifierItem) and its quick panel, the global shortcuts, and desktop notifications. The same functions as
//! macOS's `desktop.rs` for the parts Linux has; there is no Dock.
//!
//! Wayland lets no app place its windows: on Hyprland the panel opens at the top right of the focused monitor
//! (`platform::hyprland`), elsewhere where the compositor puts it. While the icon shows in a tray, closing the main
//! window only hides it: Safay stays there until "Quit".
//!
//! Everything here runs on the engine thread. Windows and handlers are made under the root's owner, so they outlive
//! the window whose button or menu asked for them.
use crate::{
    i18n::{self, Language},
    platform,
};
use nsh_core::{
    desktop::{SizeToContent, WindowAttributes, WindowKind},
    element::desktop::{
        MenuItem, WindowRef, main_window, on_app_quit, open_window, register_global_shortcut, set_tray, tray,
        unregister_all_global_shortcuts, window,
    },
    prelude::*,
};
use safay_app::{controller::Controller, preferences::Preferences};
use std::{cell::RefCell, sync::Arc};

/// The quick panel: as wide as macOS's; square, since the compositor draws the window's own corners and border.
pub const PANEL_WIDTH: f32 = 310.0;
pub const PANEL_RADIUS: f32 = 0.0;
/// The panel window's title: what Hyprland's placement rule matches (`platform::hyprland`).
const PANEL_TITLE: &str = "Safay Panel";

#[derive(Clone, Copy)]
struct Desktop {
    owner: Stored<Owner>,
    controller: Stored<Arc<Controller>>,
    preview: bool,
    /// What the tray follows; `None` until the main window's store has loaded the preferences.
    preferences: Signal<Option<Preferences>>,
    language: Signal<Language>,
}

thread_local! {
    static DESKTOP: std::cell::Cell<Option<Desktop>> = const { std::cell::Cell::new(None) };
    static PANEL: RefCell<Option<WindowRef>> = const { RefCell::new(None) };
    /// The main window was closed to the tray, or started there.
    static HIDDEN: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// The preferences arrived once (the launch's own).
    static LAUNCHED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// When the panel last closed: a click on the tray icon closes it first (outside it), and must not open it again.
    static PANEL_CLOSED: std::cell::Cell<Option<std::time::Instant>> = const { std::cell::Cell::new(None) };
    /// A menu of the panel's is open, or closed a moment ago: the panel's blur is the menu's then. Counted, so an
    /// earlier menu's delayed end leaves a newer one alone.
    static MENU: std::cell::Cell<(bool, u64)> = const { std::cell::Cell::new((false, 0)) };
}

fn desktop() -> Option<Desktop> {
    DESKTOP.with(std::cell::Cell::get)
}

/// Sets up the tray icon and the main window's close button. Called once, while the app's root renders.
pub fn install(controller: Arc<Controller>, preview: bool) {
    let d = Desktop {
        owner: stored(Owner::current()),
        controller: stored(controller),
        preview,
        preferences: signal(None),
        language: signal(Language::default()),
    };
    DESKTOP.with(|slot| slot.set(Some(d)));
    // The panel's menus in the app's color scheme, with the main window shown or not.
    platform::linux::follow_scheme();

    // While the icon shows in a tray, the close button hides the main window and Safay stays there; otherwise it closes,
    // which quits. A desktop with no tray (GNOME without its AppIndicator extension) would leave a hidden window no way
    // back.
    let main = main_window();
    let shown = move || d.preferences.with(|p| p.is_some_and(|p| p.tray_icon)) && platform::linux::has_tray();
    effect(move || {
        let mut attributes = WindowAttributes::default();
        attributes.close_on_press = Some(!shown());
        main_window().set(attributes);
    });
    main.on_close_press(move |_| {
        if untracked(shown) {
            close_panel();
            HIDDEN.with(|h| h.set(true));
            main_window().hide();
        }
    });

    // The tray icon, its menu in the app's language.
    let state = signal(None::<(bool, Language)>);
    effect(move || {
        let next = d.preferences.get().map(|p| (p.tray_icon, d.language.get()));
        if untracked(|| state.get()) != next {
            state.set(next);
        }
    });
    effect(move || {
        let Some((visible, language)) = state.get() else { return };
        if !visible {
            set_tray(None);
            return;
        }
        let label = |key: &str| i18n::tr(language, key).to_string();
        set_tray(tray().icon(asset!("menubar-template.svg")).tooltip("Safay").on_click(move |_| toggle_panel()).menu([
            MenuItem::new(label("Open Safay")).on_select(open_main),
            MenuItem::new(label("Settings")).on_select(open_settings),
            MenuItem::new(label("Check for Updates")).disabled(!platform::updates_configured()).on_select(platform::request_update_check),
            MenuItem::separator(),
            MenuItem::new(label("Quit")).on_select(platform::quit),
        ]));
    });

    // The system-wide shortcuts, through the desktop portal: registered again only when one of them changes. On
    // Hyprland, whose portal binds no key itself, they are bound to their keys too (`platform::hyprland`).
    let keys = signal(None::<[Option<String>; 3]>);
    effect(move || {
        let next = d
            .preferences
            .get()
            .map(|p| [p.hotkey_panel, p.hotkey_main, p.hotkey_capture].map(|h| h.and_then(platform::hotkeys::keystroke)));
        if untracked(|| keys.get()) != next {
            keys.set(next);
        }
    });
    effect(move || {
        let Some(keys) = keys.get() else { return };
        unregister_all_global_shortcuts();
        let actions: [fn(); 3] = [toggle_panel, open_main, platform::request_capture_from_shortcut];
        for (keystroke, action) in keys.iter().zip(actions) {
            if let Some(keystroke) = keystroke {
                // A keystroke the desktop refuses is not taken; the answer is not awaited.
                drop(register_global_shortcut(keystroke, action));
            }
        }
        platform::hyprland::bind_shortcuts(&keys.into_iter().flatten().collect::<Vec<_>>());
    });
    on_app_quit(|| async { platform::hyprland::release() });

    // Launching Safay again brings the main window forward, out of the tray too (the runner only raises a shown one).
    spawn(async move {
        loop {
            let _ = next_second_instance().await;
            open_main();
        }
    });

    // `safay://` links (the scheme `platform::URL_SCHEME`): any of them brings the main window forward. Nothing in a
    // link is read or acted on beyond that.
    spawn(async move {
        loop {
            let url = next_url_scheme_open().await;
            if url.starts_with(&format!("{}:", platform::URL_SCHEME)) {
                open_main();
            }
        }
    });
}

/// The main window's store loaded or changed the preferences.
pub fn apply(preferences: Preferences, language: Language) {
    let Some(d) = desktop() else { return };
    if d.language.get_untracked() != language {
        d.language.set(language);
    }
    if d.preferences.get_untracked() != Some(preferences) {
        d.preferences.set(Some(preferences));
    }
    // "Start in Tray": the runner made the main window hidden (`main.rs`); it is in the tray from the start.
    if !LAUNCHED.with(|l| l.replace(true)) && preferences.background_launch && preferences.tray_icon && platform::linux::has_tray() {
        HIDDEN.with(|h| h.set(true));
    }
    // Hiding the icon while the main window is in the tray would leave no way back to Safay.
    if !preferences.tray_icon && HIDDEN.with(|h| h.replace(false)) {
        main_window().show();
    }
}

/// A result of capturing a QR code from the shortcut or the tray panel, shown as a desktop notification (macOS shows its
/// own card, tinted by `_tone`). A newer one replaces the one on screen. Only already-localized result text is shown,
/// never secrets.
pub fn notice(_tone: crate::store::Tone, text: String) {
    let Some(d) = desktop() else { return };
    d.owner.get().run(|| {
        let mut notification = nsh_core::notifications::Notification::new("capture", "Safay", text);
        notification.silent = true;
        spawn(async move {
            // A desktop without a notification service shows nothing.
            let _ = show_notification(notification).await;
        });
    });
}

/// Brings the main window forward, showing it if it was closed to the tray.
pub fn open_main() {
    let Some(d) = desktop() else { return };
    d.owner.get().run(|| {
        close_panel();
        HIDDEN.with(|h| h.set(false));
        let main = main_window();
        main.show();
        main.focus();
    });
}

/// Whether this engine's root set up the desktop integration: the app's own launcher does, NSH's test and preview
/// runners do not.
pub fn installed() -> bool {
    desktop().is_some()
}

/// The update window: what `version` brings (`notes`, Markdown) and the choice to install it, in a window of its own
/// with the desktop's title bar. Opening it again brings the open one forward.
pub fn open_update(version: String, notes: Option<String>) {
    let Some(d) = desktop() else { return };
    d.owner.get().run(|| {
        close_panel();
        let mut attributes = WindowAttributes::default();
        attributes.min_size = Some(Some(size(px(480.0), px(360.0))));
        attributes.minimizable = Some(false);
        attributes.maximizable = Some(false);
        let (controller, preview) = (d.controller.get(), d.preview);
        open_window(
            window()
                .id("update")
                .title(move || i18n::tr(d.language.get(), "Software Update").to_string())
                .default_size(size(px(560.0), px(440.0)))
                .attributes(attributes)
                .content(move || crate::update_window(controller.clone(), preview, version.clone(), notes.clone())),
        );
    });
}

/// Settings: a dialog over the main window on Linux (`supports::SETTINGS_WINDOW` is off).
pub fn open_settings() {
    platform::request_sheet(crate::store::Sheet::Settings);
    open_main();
}

/// The tray icon: opens the quick panel, or closes the open one.
pub fn toggle_panel() {
    let Some(d) = desktop() else { return };
    if PANEL.with(|p| p.borrow().is_some()) {
        close_panel();
        return;
    }
    if PANEL_CLOSED.with(std::cell::Cell::get).is_some_and(|at| at.elapsed() < std::time::Duration::from_millis(300)) {
        return;
    }
    // At the focused monitor's top right on Hyprland; elsewhere where the compositor puts it.
    platform::hyprland::place_at_top_right(PANEL_TITLE, f64::from(PANEL_WIDTH));
    d.owner.get().run(|| {
        // A popup: no frame, closed by Escape and when it loses the focus. It follows its content's height. On Wayland a
        // menu the panel opens takes the keyboard from its window, which would close the panel with the menu: the blur
        // that closes it is the panel's own (`panel_blurred`), which a menu's leaves alone.
        let mut attributes = WindowAttributes::default();
        attributes.size_to_content = Some(SizeToContent::Height);
        attributes.close_on_blur = Some(false);
        let (controller, preview) = (d.controller.get(), d.preview);
        let opened = open_window(
            window()
                .id("panel")
                .kind(WindowKind::Popup)
                .title(PANEL_TITLE)
                .default_size(size(px(PANEL_WIDTH), px(430.0)))
                .attributes(attributes)
                .on_open_change(|_| {
                    PANEL.with(|p| p.borrow_mut().take());
                    PANEL_CLOSED.with(|at| at.set(Some(std::time::Instant::now())));
                    // A menu open as the panel closed never says it ended.
                    MENU.with(|m| m.set((false, m.get().1 + 1)));
                    platform::hyprland::close_on_outside_click(PANEL_TITLE, false);
                })
                .content(move || crate::panel_window(controller.clone(), preview)),
        );
        if let Some(panel) = opened {
            platform::hyprland::close_on_outside_click(PANEL_TITLE, true);
            PANEL.with(|p| *p.borrow_mut() = Some(panel));
        }
    });
}

/// The shortcut editor records (`true`) or stops: while it records, the keys this app bound on Hyprland are off, so a
/// keystroke one of them holds (the shortcut being recorded again) reaches the editor instead of the compositor.
pub fn pause_shortcuts(paused: bool) {
    if paused {
        platform::hyprland::unbind_shortcuts();
    } else {
        platform::hyprland::rebind_shortcuts();
    }
}

/// A menu of the panel opens (`true`) or closes: while it is open, the panel losing the keyboard to it keeps it open.
/// Once it closes the panel takes the keyboard back, and a blur in the next moment is still the menu's: a compositor whose
/// focus follows the pointer gives the keyboard to the window under it as the menu goes, which would close the panel the
/// user just chose in.
pub fn menu_open(open: bool) {
    let Some(panel) = PANEL.with(|p| p.borrow().clone()) else { return };
    // A click on the menu, which may reach past the panel, is not one outside it.
    platform::hyprland::close_on_outside_click(PANEL_TITLE, !open);
    let turn = MENU.with(|m| {
        let turn = m.get().1 + 1;
        m.set((true, turn));
        turn
    });
    if open {
        return;
    }
    panel.focus();
    let Some(d) = desktop() else { return };
    d.owner.get().run(|| {
        set_timeout(std::time::Duration::from_millis(400), move || {
            MENU.with(|m| {
                if m.get().1 == turn {
                    m.set((false, turn));
                }
            })
        });
    });
}

/// The panel lost the keyboard: it closes, unless a menu of its own took it.
pub fn panel_blurred() {
    if !MENU.with(|m| m.get().0) {
        close_panel();
    }
}

/// Closes the panel (its own buttons that leave it, opening another window).
pub fn close_panel() {
    if let Some(panel) = PANEL.with(|p| p.borrow_mut().take()) {
        panel.close();
    }
}
