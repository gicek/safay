//! Safay's NSH UI. The production launcher installs the only real vault controller.
use nsh_core::prelude::*;
use safay_app::controller::Controller;
use std::sync::{Arc, OnceLock};
mod alert;
mod chrome;
mod components;
#[cfg(debug_assertions)]
mod demo;
#[cfg(target_os = "macos")]
mod desktop;
mod i18n;
mod icons;
#[cfg(target_os = "macos")]
mod launcher;
mod pages;
pub mod platform;
#[cfg(target_os = "macos")]
mod scan;
mod shell;
mod store;
use store::{Store, Surface};
/// The development build number, from nsh.json.
pub const BUILD: &str = "7";
static CONTROLLER: OnceLock<(Arc<Controller>, bool)> = OnceLock::new();
/// Running under the app's own launcher, not an NSH test or preview runner.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn hosted() -> bool {
    CONTROLLER.get().is_some()
}
pub fn install_controller(controller: Arc<Controller>, preview: bool) -> Result<(), (Arc<Controller>, bool)> {
    CONTROLLER.set((controller, preview))
}
#[native_module("safay/native")]
fn native(r: &mut Registry) {
    r.assets(assets!());
    r.register::<App>();
    #[cfg(debug_assertions)]
    {
        r.register::<FixtureApp>();
        r.register::<demo::DemoApp>();
        r.register::<demo::DemoLockedApp>();
        r.register::<demo::DemoPanelApp>();
        r.register::<demo::DemoSettingsApp>();
    }
}
/// The default CLI runner is a safe UI harness. Only our launcher can install disk services.
#[component]
pub fn App() -> impl IntoElement {
    if let Some((controller, preview)) = CONTROLLER.get() {
        #[cfg(target_os = "macos")]
        {
            platform::macos::configure(controller.clone());
            desktop::install(controller.clone(), *preview);
        }
        #[cfg(target_os = "linux")]
        if !*preview {
            platform::linux::configure(controller.clone());
        }
        shell::main_window(controller.clone(), *preview).into_element()
    } else {
        view()
            .flex_1()
            .items_center()
            .justify_center()
            .p_8()
            .gap_4()
            .child(text("Safay").text_size(px(28.0)))
            .child(text("NSH UI harness. Launch the local vault with cargo run."))
            .into_element()
    }
}
/// Synthetic, in-memory UI test root. Never registers in a release build.
#[cfg(debug_assertions)]
#[component]
pub fn FixtureApp() -> impl IntoElement {
    let controller =
        Controller::new(Box::new(safay_app::storage::MemoryRepository::default()), Arc::new(FixtureClock)).expect("memory fixture");
    shell::main_window(controller, true)
}

pub(crate) struct FixtureClock;
impl safay_app::session::Clock for FixtureClock {
    fn wall(&self) -> std::time::SystemTime {
        std::time::UNIX_EPOCH + std::time::Duration::from_secs(1234567897)
    }
    fn elapsed(&self) -> std::time::Duration {
        std::time::Duration::ZERO
    }
}

fn surface_root(s: Store, content: impl IntoElement) -> impl IntoElement {
    // The macOS Settings window shows the system's sidebar material behind it (`desktop::open_settings`): its pages paint
    // their own backgrounds, the sidebar none.
    let see_through = s.surface == Surface::Settings && chrome::look() == chrome::Look::Mac;
    view()
        .flex_1()
        .font_family("system-ui")
        .font_weight(FontWeight::NORMAL)
        .text_size(px(13.0))
        .bg(move || if see_through { nsh_base::theme().background.opacity(0.0) } else { nsh_base::theme().background })
        .text_color(move || nsh_base::theme().foreground)
        .on_press(move |_: &()| s.controller.get().activity())
        .child(content)
}

/// The quick panel's content over `controller`, with a store of its own.
fn quick_shell(controller: Arc<Controller>, fixture: bool) -> impl IntoElement {
    let s = Store::new(controller, fixture, Surface::Panel);
    effect(move || untracked(|| s.start()));
    surface_root(s, pages::quick::page(s))
}

/// The menu bar panel's window content (`desktop::toggle_panel`), made each time the panel opens: the search and the
/// highlight start empty and the keyboard goes to the search field. The window follows this view's height, in the
/// same frame as the content changes (the account list is virtualized); the lock screen is 430 pt.
#[cfg(target_os = "macos")]
pub(crate) fn panel_window(controller: Arc<Controller>, preview: bool) -> impl IntoElement {
    let s = Store::new(controller, preview, Surface::Panel);
    effect(move || untracked(|| s.start()));
    s.focus_search();
    let height = move || {
        let p = s.projection.get();
        let height = if !s.ready.get() || (p.exists && !p.unlocked) {
            430.0
        } else if !p.exists {
            300.0
        } else if p.rows.is_empty() {
            38.0 + 18.0 + 1.0 + 250.0 + 1.0 + 42.0
        } else {
            38.0 + 18.0 + 1.0 + p.rows.len().clamp(5, 9) as f32 * pages::quick::ROW + 1.0 + 42.0
        };
        px(height.clamp(320.0, 500.0))
    };
    // The window is transparent: the panel's shape is this view's. It fades in like an AppKit panel window
    // (AppKit's 0.25 s alpha animation), at once with reduced motion.
    let shown = signal(reduce_motion());
    set_timeout(std::time::Duration::from_millis(16), move || shown.set(true));
    view()
        .opacity(move || if shown.get() { 1.0_f32 } else { 0.0 })
        .transition_opacity()
        .duration(250.0)
        .ease_out()
        .w(px(desktop::PANEL_WIDTH))
        .h(height)
        .rounded(px(desktop::PANEL_RADIUS))
        .overflow_hidden()
        .child(surface_root(s, pages::quick::page(s)))
}

/// The macOS Settings window's content (`desktop::open_settings`), with a store of its own.
#[cfg(target_os = "macos")]
pub(crate) fn settings_window(controller: Arc<Controller>, preview: bool) -> impl IntoElement {
    let s = Store::new(controller, preview, Surface::Settings);
    effect(move || untracked(|| s.start()));
    surface_root(s, view().flex_1().child(pages::settings::page(s)).child(pages::sheets::auth_dialog(s)))
}

/// The update window's content (`desktop::open_update`), with a store of its own.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn update_window(controller: Arc<Controller>, preview: bool, version: String, notes: Option<String>) -> impl IntoElement {
    let s = Store::new(controller, preview, Surface::Update);
    effect(move || untracked(|| s.start()));
    surface_root(s, pages::update::page(s, version, notes))
}

#[cfg(test)]
mod surface_tests {
    use super::*;
    use nsh_engine::EngineConfig;
    use nsh_testing::{Headless, MemoryStorage};
    use std::time::{Duration, SystemTime};
    static TWIN: OnceLock<Arc<Controller>> = OnceLock::new();
    fn controller() -> Arc<Controller> {
        TWIN.get_or_init(|| Controller::new(Box::new(safay_app::storage::MemoryRepository::default()), Arc::new(FixtureClock)).unwrap())
            .clone()
    }
    #[component]
    fn TwinMain() -> impl IntoElement {
        shell::main_window(controller(), true)
    }
    #[component]
    fn TwinPanel() -> impl IntoElement {
        quick_shell(controller(), true)
    }
    fn open(root: nsh_engine::Root, width: f32, height: f32) -> Headless {
        let mut config = EngineConfig::new(root);
        config.viewport = nsh_core::Size::new(px(width), px(height));
        config.storage = Some(MemoryStorage::new(std::iter::empty::<(String, String)>()));
        config.clipboard = Some(Arc::new(nsh_core::clipboard::MemoryClipboard::default()));
        config.clock = nsh_core::time::Clock::manual(SystemTime::UNIX_EPOCH);
        config.network = None;
        config.data_dir = None;
        config.cache = nsh_engine::CacheOptions::off();
        Headless::start(config).unwrap()
    }
    static SYNC: OnceLock<Arc<Controller>> = OnceLock::new();
    fn sync_controller() -> Arc<Controller> {
        SYNC.get_or_init(|| Controller::new(Box::new(safay_app::storage::MemoryRepository::default()), Arc::new(FixtureClock)).unwrap())
            .clone()
    }
    #[component]
    fn SyncMain() -> impl IntoElement {
        shell::main_window(sync_controller(), true)
    }
    #[test]
    fn linking_a_folder_backs_up_at_once_and_shows_history() {
        let dir = tempfile::tempdir().unwrap();
        let mut main = open(nsh_engine::Root::of::<SyncMain>(), 980.0, 680.0);
        main.wait_for("Start fresh").unwrap();
        main.press("role:button[Start fresh]").unwrap();
        main.type_text("id:new-password", "P8!nQ2@vT7#sL4$m").unwrap();
        main.type_text("id:confirm-password", "P8!nQ2@vT7#sL4$m").unwrap();
        main.press("role:button[Create Vault]").unwrap();
        main.wait_for_within("No 2FA accounts yet", Duration::from_secs(20)).unwrap();
        main.press("role:button[Backup & Sync]").unwrap();
        main.wait_for("Backup is off").unwrap();
        // The folder picker is native; link exactly as its result does, then the main window syncs by itself.
        let c = sync_controller();
        safay_app::sync::link(&c, c.epoch(), safay_app::sync::Location::Folder { path: dir.path().join("Safay") }).unwrap();
        c.touch();
        main.wait_for_within("Uploaded", Duration::from_secs(10)).unwrap();
        assert!(dir.path().join("Safay/vault.safayvault").is_file());
        main.wait_for("Safay").unwrap();
    }
    #[cfg(target_os = "macos")]
    static WINDOWS: OnceLock<Arc<Controller>> = OnceLock::new();
    /// The main window with the desktop integration the app's own launcher installs.
    #[cfg(target_os = "macos")]
    #[component]
    fn DesktopMain() -> impl IntoElement {
        let controller = WINDOWS
            .get_or_init(|| Controller::new(Box::new(safay_app::storage::MemoryRepository::default()), Arc::new(FixtureClock)).unwrap())
            .clone();
        desktop::install(controller.clone(), true);
        shell::main_window(controller, true)
    }
    #[cfg(target_os = "macos")]
    #[test]
    fn settings_open_in_a_window_of_their_own_and_close() {
        let mut config = EngineConfig::new(nsh_engine::Root::of::<DesktopMain>());
        config.viewport = nsh_core::Size::new(px(980.0), px(680.0));
        config.storage = Some(MemoryStorage::new(std::iter::empty::<(String, String)>()));
        config.clipboard = Some(Arc::new(nsh_core::clipboard::MemoryClipboard::default()));
        config.clock = nsh_core::time::Clock::manual(SystemTime::UNIX_EPOCH);
        config.network = None;
        config.data_dir = None;
        config.cache = nsh_engine::CacheOptions::off();
        config.environment.os = nsh_core::platform::Os::MacOs;
        config.environment.idiom = nsh_core::platform::Idiom::Desktop;
        let mut app = Headless::start(config).unwrap();
        app.wait_for("Start fresh").unwrap();
        assert!(app.window_state("settings").unwrap().is_none());
        app.press_key("mod+,").unwrap();
        app.wait_for("Appearance").unwrap();
        let state = app.window_state("settings").unwrap().expect("the Settings window");
        assert_eq!((state.content_bounds.size.width, state.content_bounds.size.height), (px(760.0), px(520.0)));
        // Asking again brings the open window forward instead of opening a second one.
        app.press_key("mod+,").unwrap();
        assert!(app.window_state("settings").unwrap().is_some());
        app.press_window_close("settings").unwrap();
        assert!(app.window_state("settings").unwrap().is_none());
        assert!(!app.screen().texts().contains(&"Appearance"));
    }
    #[test]
    fn two_engines_share_one_vault_and_lock_clears_both() {
        let mut main = open(nsh_engine::Root::of::<TwinMain>(), 980.0, 680.0);
        main.wait_for("Start fresh").unwrap();
        main.press("role:button[Start fresh]").unwrap();
        main.type_text("id:new-password", "P8!nQ2@vT7#sL4$m").unwrap();
        main.type_text("id:confirm-password", "P8!nQ2@vT7#sL4$m").unwrap();
        main.press("role:button[Create Vault]").unwrap();
        main.wait_for_within("No 2FA accounts yet", Duration::from_secs(20)).unwrap();
        main.press("role:button[Add]").unwrap();
        main.press("role:button[Paste Links]").unwrap();
        main.type_text("id:import-text", "otpauth://totp/Shared:fixture?secret=JBSWY3DPEHPK3PXP&issuer=Shared").unwrap();
        main.press("role:button[Save Account]").unwrap();
        main.wait_for("Shared").unwrap();
        let mut panel = open(nsh_engine::Root::of::<TwinPanel>(), 310.0, 430.0);
        panel.wait_for("Shared").unwrap();
        panel.press("role:button[Copy code for Shared]").unwrap();
        panel.wait_for("Copied").unwrap();
        main.press("role:button[Lock]").unwrap();
        main.wait_for("Safay is locked").unwrap();
        panel.wait_for("Safay is locked").unwrap();
        assert!(!main.screen().texts().contains(&"Shared"));
        assert!(!panel.screen().texts().contains(&"Shared"));
        panel.type_text("id:password", "P8!nQ2@vT7#sL4$m").unwrap();
        panel.submit("id:password").unwrap();
        panel.wait_for_within("Shared", Duration::from_secs(20)).unwrap();
        main.wait_for_within("Shared", Duration::from_secs(5)).unwrap();
        // Closing the panel keeps the session: the panel stays alive between openings.
        drop(panel);
        main.run_for(Duration::from_millis(200)).unwrap();
        assert!(main.screen().texts().contains(&"Shared"));
    }
}
