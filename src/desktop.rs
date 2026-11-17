//! The app's own windows, tray icon and global shortcuts, on NSH's desktop API (`nsh_core::element::desktop`): the menu
//! bar icon and the quick panel under it, the Settings window, the desktop notice and the system-wide shortcuts. Every
//! window renders from the app's one engine; each still has a `Store` of its own over the shared `Controller`.
//!
//! Everything here runs on the engine thread. Windows and handlers are made under the root's owner, so they outlive
//! the window whose button or menu asked for them. What NSH does not offer stays in `platform::macos` (the Dock
//! policy, system lock events).
use crate::{
    i18n::{self, Language},
    platform,
    store::Tone,
};
use nsh_core::{
    desktop::{SizeToContent, TitleBarOptions, TitleBarStyle, WindowAttributes, WindowCloseCause, WindowKind, WindowLevel, WindowPosition},
    element::desktop::{
        MenuItem, WindowRef, displays, main_window, open_window, register_global_shortcut, set_tray, tray, unregister_all_global_shortcuts,
        window,
    },
    prelude::*,
};
use safay_app::{controller::Controller, preferences::Preferences};
use std::{
    cell::{Cell, RefCell},
    sync::Arc,
    time::{Duration, Instant},
};

/// The menu bar panel: 310 pt wide, 5 pt under the menu bar, 8 pt from the screen's edges, the system's popover radius.
pub const PANEL_WIDTH: f32 = 310.0;
pub const PANEL_RADIUS: f32 = 18.0;
const PANEL_GAP: f32 = 5.0;
const PANEL_EDGE: f32 = 8.0;
/// The desktop notice: a 430×58 capsule, 44 pt under the top of the screen's visible area, for 2.6 seconds.
const TOAST_SIZE: (f32, f32) = (430.0, 58.0);
const TOAST_TOP: f32 = 44.0;
const TOAST_SHOWN: Duration = Duration::from_millis(2600);
const TOAST_FADE: Duration = Duration::from_millis(250);
/// The main window's least content size.
const MAIN_MIN: (f32, f32) = (880.0, 620.0);

#[derive(Clone, Copy)]
struct Desktop {
    owner: Stored<Owner>,
    controller: Stored<Arc<Controller>>,
    preview: bool,
    /// What the tray and the shortcuts follow; `None` until the main window's store has loaded the preferences.
    preferences: Signal<Option<Preferences>>,
    language: Signal<Language>,
    /// Where the menu bar icon was when it was last clicked.
    icon: Signal<Option<Bounds<Points>>>,
    /// Counts desktop notices, so an older one's timer leaves a newer one alone.
    notices: Signal<u64>,
}

thread_local! {
    static DESKTOP: Cell<Option<Desktop>> = const { Cell::new(None) };
    static PANEL: RefCell<Option<WindowRef>> = const { RefCell::new(None) };
    static TOAST: RefCell<Option<WindowRef>> = const { RefCell::new(None) };
    /// When the panel last closed because it lost the keyboard: a click on the icon does that first, and must not
    /// open it again.
    static PANEL_BLURRED: Cell<Option<Instant>> = const { Cell::new(None) };
    static LAUNCHED: Cell<bool> = const { Cell::new(false) };
}

fn desktop() -> Option<Desktop> {
    DESKTOP.with(Cell::get)
}

/// Whether this engine's root set up the desktop integration: the app's own launcher does, NSH's test and preview
/// runners do not.
pub fn installed() -> bool {
    desktop().is_some()
}

/// Sets up the main window, the tray and the shortcuts. Called once, while the app's root renders.
pub fn install(controller: Arc<Controller>, preview: bool) {
    let d = Desktop {
        owner: stored(Owner::current()),
        controller: stored(controller),
        preview,
        preferences: signal(None),
        language: signal(Language::default()),
        icon: signal(None),
        notices: signal(0),
    };
    DESKTOP.with(|slot| slot.set(Some(d)));

    let main = main_window();
    let mut attributes = WindowAttributes::default();
    attributes.min_size = Some(Some(size(px(MAIN_MIN.0), px(MAIN_MIN.1))));
    main.set(attributes);
    // "Only when the main window is open": closing it leaves only the menu bar icon.
    main.on_close_press(|_| platform::macos::apply_dock(false));

    // The menu bar icon, its menu in the app's language.
    let shown = signal(None::<(bool, Language)>);
    effect(move || {
        let next = d.preferences.get().map(|p| (p.tray_icon, d.language.get()));
        if untracked(|| shown.get()) != next {
            shown.set(next);
        }
    });
    effect(move || {
        let Some((visible, language)) = shown.get() else { return };
        if !visible {
            set_tray(None);
            return;
        }
        let label = |key: &str| i18n::tr(language, key).to_string();
        set_tray(
            tray()
                .icon(asset!("menubar-template.svg"))
                .tooltip("Safay")
                .on_click(move |event| {
                    if event.bounds.is_some() {
                        d.icon.set(event.bounds);
                    }
                    toggle_panel();
                })
                .menu([
                    MenuItem::new(label("Open Safay")).on_select(open_main),
                    MenuItem::new(label("Settings…")).on_select(open_settings),
                    MenuItem::separator(),
                    MenuItem::new(label("Quit")).on_select(platform::quit),
                ]),
        );
    });

    // `safay://` links (the scheme `platform::URL_SCHEME`, claimed in `main.rs`): any of them brings the main window
    // forward. Nothing in a link is read or acted on beyond that.
    spawn(async move {
        loop {
            let url = next_url_scheme_open().await;
            if url.starts_with(&format!("{}:", platform::URL_SCHEME)) {
                open_main();
            }
        }
    });

    // The system-wide shortcuts: registered again only when one of them changes.
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
        let actions: [fn(); 3] = [toggle_panel, open_main, platform::request_capture];
        for (keystroke, action) in keys.into_iter().zip(actions) {
            if let Some(keystroke) = keystroke {
                // A keystroke another app holds is not taken; the answer is not awaited.
                drop(register_global_shortcut(&keystroke, action));
            }
        }
    });
}

/// The app's language as the main window last resolved it (for text shown outside a window: Touch ID, notices).
pub fn language() -> Language {
    desktop().map(|d| d.language.get_untracked()).unwrap_or_default()
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
    platform::macos::apply_preferences(preferences);
    // "Background Launch Only": at launch the main window goes away and only the menu bar icon remains.
    if !LAUNCHED.replace(true) && preferences.background_launch && preferences.tray_icon {
        main_window().hide();
        platform::macos::apply_dock(false);
    }
}

/// Brings the main window forward (the tray menu, the panel, a shortcut, the Settings window).
pub fn open_main() {
    let Some(d) = desktop() else { return };
    d.owner.get().run(|| {
        close_panel();
        platform::macos::apply_dock(true);
        main_window().focus();
    });
}

/// The Settings window (⌘,): 760×520, not resizable, the traffic lights
/// over the panes. Opening it again brings the open one forward.
pub fn open_settings() {
    let Some(d) = desktop() else { return };
    d.owner.get().run(|| {
        close_panel();
        let mut bar = TitleBarOptions::default();
        bar.style = TitleBarStyle::Hidden;
        let mut attributes = WindowAttributes::default();
        attributes.title_bar = Some(bar);
        attributes.resizable = Some(false);
        attributes.maximizable = Some(false);
        // The system's sidebar material behind the window, seen through the sidebar;
        // it grays out with the window as AppKit's.
        attributes.backdrop = Some(nsh_core::desktop::Backdrop::Sidebar);
        let (controller, preview) = (d.controller.get(), d.preview);
        open_window(
            window()
                .id("settings")
                .title(move || i18n::tr(d.language.get(), "Settings").to_string())
                .default_size(size(px(760.0), px(520.0)))
                .default_position(WindowPosition::Center)
                .attributes(attributes)
                .content(move || crate::settings_window(controller.clone(), preview)),
        );
    });
}

/// The menu bar icon and the panel shortcut: opens the panel, or closes the open one.
pub fn toggle_panel() {
    let Some(d) = desktop() else { return };
    if PANEL.with(|p| p.borrow().is_some()) {
        close_panel();
        return;
    }
    if PANEL_BLURRED.with(Cell::get).is_some_and(|at| at.elapsed() < Duration::from_millis(300)) {
        return;
    }
    d.owner.get().run(|| {
        // Under the icon, without activating the app: the frontmost app keeps its menu bar and Safay's main window
        // stays where it is. The panel takes the keyboard itself, and closes on Escape or when it loses it.
        let mut attributes = WindowAttributes::default();
        attributes.activate = Some(false);
        attributes.transparent = Some(true);
        attributes.size_to_content = Some(SizeToContent::Height);
        attributes.level = Some(WindowLevel::AboveDock);
        let (controller, preview) = (d.controller.get(), d.preview);
        let opened = open_window(
            window()
                .id("panel")
                .kind(WindowKind::Popup)
                .default_size(size(px(PANEL_WIDTH), px(430.0)))
                .default_position(WindowPosition::At(panel_origin(d.icon.get_untracked())))
                .attributes(attributes)
                .on_open_change(|event| {
                    PANEL.with(|p| p.borrow_mut().take());
                    platform::macos::unwatch_outside_clicks();
                    if event.reason == WindowCloseCause::Blur {
                        PANEL_BLURRED.with(|at| at.set(Some(Instant::now())));
                    }
                })
                .content(move || crate::panel_window(controller.clone(), preview)),
        );
        if let Some(panel) = opened {
            panel.focus();
            PANEL.with(|p| *p.borrow_mut() = Some(panel));
            // A click in another app closes it too: the panel keeps the keyboard then, so no blur tells.
            let (clicked, clicks) = std::sync::mpsc::channel();
            platform::macos::watch_outside_clicks(clicked);
            spawn(async move {
                if blocking(move || clicks.recv().is_ok()).await.unwrap_or(false) {
                    PANEL_BLURRED.with(|at| at.set(Some(Instant::now())));
                    close_panel();
                }
            });
        }
    });
}

/// Closes the panel (its own buttons that leave it, opening another window).
pub fn close_panel() {
    if let Some(panel) = PANEL.with(|p| p.borrow_mut().take()) {
        platform::macos::unwatch_outside_clicks();
        panel.close();
    }
}

/// The panel's top left corner: centered under the icon and kept inside its display. Before the icon was ever clicked
/// (a shortcut opened the panel first) NSH does not say where it is: the top right of the primary display, where the
/// menu bar's icons are.
fn panel_origin(icon: Option<Bounds<Points>>) -> Point<Points> {
    let displays = displays();
    let inside = |b: &Bounds<Points>, p: Point<Points>| {
        p.x >= b.origin.x && p.x < b.origin.x + b.size.width && p.y >= b.origin.y && p.y < b.origin.y + b.size.height
    };
    let width = px(PANEL_WIDTH);
    let (edge, gap) = (px(PANEL_EDGE), px(PANEL_GAP));
    match icon {
        Some(icon) => {
            let area = displays.iter().find(|d| inside(&d.bounds, icon.origin)).or(displays.first()).map(|d| d.visible_bounds);
            let mut x = icon.origin.x + (icon.size.width - width) / 2.0;
            if let Some(area) = area {
                x = x.max(area.origin.x + edge).min(area.origin.x + area.size.width - width - edge);
            }
            point(x, icon.origin.y + icon.size.height + gap)
        }
        None => match displays.iter().find(|d| d.primary).or(displays.first()) {
            Some(display) => {
                let area = display.visible_bounds;
                point(area.origin.x + area.size.width - width - edge, area.origin.y + gap)
            }
            None => point(px(0.0), px(0.0)),
        },
    }
}

/// A notice on the desktop: results of capturing a QR code from the
/// shortcut or the menu bar, shown without activating Safay or taking input. A newer notice replaces the one on screen.
/// Only already-localized result text is shown, never secrets.
pub fn notice(tone: Tone, text: String) {
    let Some(d) = desktop() else { return };
    d.owner.get().run(|| {
        let turn = d.notices.get_untracked() + 1;
        d.notices.set(turn);
        if let Some(old) = TOAST.with(|t| t.borrow_mut().take()) {
            old.close();
        }
        // An overlay: transparent, click-through, never focused, above the Dock, on every space. It fades in and, before
        // it closes, out (0.25 s), at once with reduced motion.
        let mut attributes = WindowAttributes::default();
        attributes.shadow = Some(true);
        let message = (tone, text);
        let shown = signal(reduce_motion());
        set_timeout(Duration::from_millis(16), move || shown.set(true));
        set_timeout(TOAST_SHOWN.saturating_sub(TOAST_FADE), move || shown.set(false));
        let opened = open_window(
            window()
                .id(format!("notice-{turn}"))
                .kind(WindowKind::Overlay)
                .default_size(size(px(TOAST_SIZE.0), px(TOAST_SIZE.1)))
                .default_position(WindowPosition::At(notice_origin()))
                .attributes(attributes)
                .content(move || {
                    view()
                        .flex_1()
                        .opacity(move || if shown.get() { 1.0_f32 } else { 0.0 })
                        .transition_opacity()
                        .duration(TOAST_FADE.as_millis() as f32)
                        .child(crate::components::desktop_toast(Some(message.clone())))
                }),
        );
        TOAST.with(|t| *t.borrow_mut() = opened);
        set_timeout(TOAST_SHOWN, move || {
            if d.notices.get_untracked() == turn
                && let Some(shown) = TOAST.with(|t| t.borrow_mut().take())
            {
                shown.close();
            }
        });
    });
}

/// Centered near the top of the visible area of the display under the pointer (the primary one when unknown).
fn notice_origin() -> Point<Points> {
    let displays = displays();
    let primary = displays.iter().find(|d| d.primary).or(displays.first());
    let pointer = primary.and_then(|p| {
        // AppKit's screen coordinates start at the primary display's bottom left; NSH's at its top left.
        platform::macos::pointer().map(|(x, y)| point(px(x as f32), p.bounds.size.height - px(y as f32)))
    });
    let under = pointer.and_then(|at| {
        displays.iter().find(|d| {
            let b = d.bounds;
            at.x >= b.origin.x && at.x < b.origin.x + b.size.width && at.y >= b.origin.y && at.y < b.origin.y + b.size.height
        })
    });
    match under.or(primary) {
        Some(display) => {
            let area = display.visible_bounds;
            point(area.origin.x + (area.size.width - px(TOAST_SIZE.0)) / 2.0, area.origin.y + px(TOAST_TOP))
        }
        None => point(px(0.0), px(TOAST_TOP)),
    }
}
