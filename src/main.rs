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
    let controller = match Controller::new(repo, Arc::new(SystemClock::default())) {
        Ok(c) => c,
        Err(_) => {
            eprintln!("Cannot read Safay storage safely.");
            std::process::exit(1)
        }
    };
    safay::platform::install_network(preview);
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
        native::AppSource::Rust(
            native::Root::of::<safay::App>()
                .app_id(safay::platform::APP_ID)
                .url_schemes([safay::platform::URL_SCHEME])
                .tokens(theme.into_tokens())
                .capabilities(safay::platform::capabilities()),
        ),
        native::AppOptions {
            title: "Safay".into(),
            app_id: safay::platform::APP_ID.into(),
            size: (980, 680),
            #[cfg(target_os = "macos")]
            keep_alive_on_close: true,
            login_item: true,
            cache: native::CacheOptions::off(),
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
