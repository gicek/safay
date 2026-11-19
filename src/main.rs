//! Production launcher: an independent vault, no implicit access from NSH's generated runners.
#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]
#[cfg(target_os = "linux")]
use nsh_platform_linux as native;
#[cfg(target_os = "macos")]
use nsh_platform_macos as native;
#[cfg(target_os = "windows")]
use nsh_platform_windows as native;
use safay_app::{controller::Controller, session::SystemClock, storage::DiskRepository};
use std::sync::Arc;

fn main() -> impl std::process::Termination {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let preview = cfg!(debug_assertions) && args == ["--preview"];
    // Development check of the signed bundle's CloudKit access: no vault is opened, only the account state and whether a
    // backup record exists are printed.
    #[cfg(all(debug_assertions, target_os = "macos"))]
    if args == ["--icloud-check"] {
        std::process::exit(safay::platform::icloud::check())
    }
    #[cfg(debug_assertions)]
    if args == ["--update-check"] {
        std::process::exit(safay::platform::updates::command_line_check())
    }
    if !args.is_empty() && !preview {
        eprintln!("Unknown option.");
        std::process::exit(2)
    }
    let repo: Box<dyn safay_app::storage::Repository> = if preview {
        Box::new(safay_app::storage::MemoryRepository::default())
    } else {
        let Some(directory) = safay::platform::data_directory() else {
            eprintln!("Cannot locate the app's private data directory.");
            std::process::exit(1)
        };
        // After an update the new app starts while the old one is still quitting: wait briefly for its instance lock.
        let mut attempts = 0;
        loop {
            match DiskRepository::open(directory.clone()) {
                Ok(repo) => break Box::new(repo),
                // Linux: Safay is running (the launcher or its autostart entry started it again). It is told, and
                // brings its main window back, out of the tray too; this launch ends.
                #[cfg(target_os = "linux")]
                Err(_) if safay::platform::linux::hand_to_running() => std::process::exit(0),
                Err(_) if attempts < 50 => {
                    attempts += 1;
                    std::thread::sleep(std::time::Duration::from_millis(100));
                }
                Err(_) => {
                    eprintln!("Cannot open Safay storage, or another instance is already running.");
                    std::process::exit(1)
                }
            }
        }
    };
    // Debug builds: `--preview` with `SAFAY_PREVIEW_DEMO=<accounts>` opens the synthetic demo vault, unlocked, in memory.
    #[cfg(debug_assertions)]
    let demo = std::env::var("SAFAY_PREVIEW_DEMO").ok().filter(|_| preview).map(|n| safay::demo_controller(n.parse().unwrap_or(8)));
    #[cfg(not(debug_assertions))]
    let demo: Option<Arc<Controller>> = None;
    let controller = match demo.map(Ok).unwrap_or_else(|| Controller::new(repo, Arc::new(SystemClock::default()))) {
        Ok(c) => c,
        Err(_) => {
            eprintln!("Cannot read Safay storage safely.");
            std::process::exit(1)
        }
    };
    safay::platform::install_network(preview);
    // Linux: "Start in Tray" makes the main window start hidden, with only the tray icon, read before the window is made
    // so it never flashes (macOS hides it once the preferences load, `desktop::apply`).
    #[cfg(target_os = "linux")]
    let start_hidden = controller.run(controller.epoch(), |s| Ok(s.preferences())).is_ok_and(|p| p.background_launch && p.tray_icon)
        && safay::platform::linux::has_tray();
    // AppKit's traffic-light size, read here on the main thread for the title bar the engine thread sets up.
    #[cfg(target_os = "macos")]
    safay::platform::macos::traffic_light_height();
    #[cfg(target_os = "macos")]
    let _observers = safay::platform::macos::observe(controller.clone());
    #[cfg(target_os = "macos")]
    safay::platform::automation::install(controller.clone());
    #[cfg(target_os = "windows")]
    safay::platform::windows::observe(controller.clone());
    if safay::install_controller(controller.clone(), preview).is_err() {
        std::process::exit(1)
    }
    native::set_ui_sets(vec![nsh_base::registry(), safay::__nsh_native_module()]);
    let theme = nsh_base::Theme::from_json(include_str!("../theme.json")).expect("bundled theme");
    let code = native::run_app(
        native::AppSource::Rust({
            let root = native::Root::of::<safay::App>()
                .app_id(safay::platform::APP_ID)
                .url_schemes([safay::platform::URL_SCHEME])
                .tokens(theme.into_tokens())
                .capabilities(safay::platform::capabilities());
            #[cfg(target_os = "linux")]
            let root = if start_hidden {
                let mut window = nsh_core::desktop::MainWindowOptions::default();
                window.default_visible = Some(false);
                root.main_window(window)
            } else {
                root
            };
            root
        }),
        native::AppOptions {
            title: "Safay".into(),
            app_id: safay::platform::APP_ID.into(),
            size: (980, 680),
            #[cfg(target_os = "macos")]
            keep_alive_on_close: true,
            // Linux: the runner writes the app's desktop entry (named after its id, opening `safay://` links). The
            // desktop portal knows an app outside a sandbox only by that entry: without it, it refuses the global
            // shortcuts ("An app id is required").
            #[cfg(target_os = "linux")]
            schemes: vec![safay::platform::URL_SCHEME.into()],
            // Launching Safay again (the launcher, the autostart entry) reaches the running one, which brings its main
            // window back even from the tray (`desktop::install`).
            #[cfg(target_os = "linux")]
            single_instance: true,
            login_item: true,
            cache: native::CacheOptions::off(),
            // The directory the vault opened in above: NSH keeps its own files there too.
            data_dir: safay::platform::data_directory(),
            asset_base: Some(native::AssetBase::Directory(safay::platform::assets_directory())),
            ..Default::default()
        },
    );
    // NSH's runner returns here on every way of quitting (⌘Q, the menus, the system's log out): AppKit's
    // `NSApplicationWillTerminateNotification` never fires, since NSH cancels the termination and leaves the process to
    // its caller. The vault locks, and a downloaded, verified update replaces the app for the next launch.
    controller.request_lock();
    safay::platform::updates::install_on_quit();
    code
}
