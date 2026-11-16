//! Debug-only demo roots (`previews/Demo*.tsx`) for screenshots: made-up accounts, categories and frozen moment (Oct 26, 2026; 19 s left on 30-second codes). Memory only.
use crate::{FixtureClock, shell, store::Surface};
use nsh_core::prelude::*;
use safay_app::{
    controller::Controller,
    core::TotpAlgorithm,
    session::{Clock, Draft},
    storage::MemoryRepository,
};
use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const FROZEN: u64 = 1_792_999_991;
pub const PASSWORD: &str = "demo-password-2026";

/// Wall time can be set so accounts get fixed creation dates; elapsed time stays zero (no auto-lock).
struct DemoClock(AtomicU64);
impl Clock for DemoClock {
    fn wall(&self) -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(self.0.load(Ordering::SeqCst))
    }
    fn elapsed(&self) -> Duration {
        Duration::ZERO
    }
}

pub fn controller(locked: bool) -> Arc<Controller> {
    let clock = Arc::new(DemoClock(AtomicU64::new(FROZEN - 86_400 * 12)));
    let c = Controller::new(Box::new(MemoryRepository::default()), clock.clone()).expect("memory demo");
    c.run(c.epoch(), |s| {
        s.create(PASSWORD, true)?;
        let accounts: [(&str, &str, &str, TotpAlgorithm, u32, u32); 8] = [
            ("GitHub", "alex@example.com", "JBSWY3DPEHPK3PXP", TotpAlgorithm::Sha1, 6, 30),
            ("Google", "alex.chen@gmail.com", "GEZDGNBVGY3TQOJQ", TotpAlgorithm::Sha1, 6, 30),
            ("Apple", "alex@icloud.com", "MZXW6YTBOI", TotpAlgorithm::Sha1, 6, 30),
            ("Cloudflare", "ops@safay.app", "KRSXG5CTMVRXEZLU", TotpAlgorithm::Sha256, 6, 30),
            ("Amazon Web Services", "root", "NBSWY3DPO5XXE3DE", TotpAlgorithm::Sha1, 8, 30),
            ("Microsoft", "alex@outlook.com", "ORSXG5BAOJSWG33S", TotpAlgorithm::Sha1, 6, 30),
            ("Dropbox", "alex@example.com", "MFRGGZDFMZTWQ2LK", TotpAlgorithm::Sha1, 6, 30),
            ("Stripe", "billing@safay.app", "GAYTEMZUGU3DOOBZ", TotpAlgorithm::Sha1, 6, 60),
        ];
        for (i, (issuer, username, secret, algorithm, digits, period)) in accounts.iter().enumerate() {
            if i == 7 {
                clock.0.store(FROZEN, Ordering::SeqCst);
            }
            let mut d = Draft::default();
            d.issuer = (*issuer).into();
            d.username = (*username).into();
            d.secret = (*secret).into();
            d.algorithm = *algorithm;
            d.digits = *digits;
            d.period = *period;
            s.save_account(&d)?;
        }
        let rows = s.projection("", None)?.rows;
        let id = |name: &str| rows.iter().find(|r| r.issuer == name).map(|r| r.id).expect("sample");
        let order: Vec<_> = accounts.iter().map(|a| id(a.0)).collect();
        s.reorder(None, &order)?;
        for (name, members) in [
            ("Favorites", vec!["GitHub", "Google"]),
            ("Work", vec!["Cloudflare", "Amazon Web Services", "Stripe", "GitHub"]),
            ("Personal", vec!["Apple", "Microsoft", "Dropbox"]),
        ] {
            s.category(None, name)?;
            let cat = s.projection("", None)?.categories.last().map(|c| c.id).expect("category");
            for m in members {
                s.membership(cat, id(m), true)?;
            }
        }
        // Captures for dark mode: `SAFAY_DEMO_APPEARANCE=dark`.
        if let Ok(mode) = std::env::var("SAFAY_DEMO_APPEARANCE") {
            let mut p = s.preferences();
            p.appearance = match mode.as_str() {
                "dark" => safay_app::preferences::Appearance::Dark,
                "light" => safay_app::preferences::Appearance::Light,
                _ => p.appearance,
            };
            s.set_preferences(p)?;
        }
        if locked {
            s.lock();
        }
        Ok(())
    })
    .expect("demo samples");
    let _ = FixtureClock;
    c
}

/// The main window over the demo vault, unlocked.
#[component]
pub fn DemoApp() -> impl IntoElement {
    shell::main_window(controller(false), true)
}
/// The main window over the demo vault, locked.
#[component]
pub fn DemoLockedApp() -> impl IntoElement {
    let c = controller(false);
    c.request_lock();
    shell::main_window(c, true)
}
/// The menu bar panel's content over the demo vault.
#[component]
pub fn DemoPanelApp() -> impl IntoElement {
    crate::quick_shell(controller(false), true)
}
/// The macOS Settings window's content at its 760×520 size.
#[component]
pub fn DemoSettingsApp() -> impl IntoElement {
    let s = crate::store::Store::new(controller(false), true, Surface::Settings);
    effect(move || untracked(|| s.start()));
    // As the Settings window: content under a transparent title bar, over the sidebar material.
    effect(move || {
        crate::shell::hide_title_bar(None);
        let mut attributes = nsh_core::desktop::WindowAttributes::default();
        attributes.backdrop = Some(nsh_core::desktop::Backdrop::Sidebar);
        nsh_core::element::desktop::main_window().set(attributes);
    });
    if let Ok(scene) = std::env::var("SAFAY_DEMO_SCENE") {
        use crate::store::SettingsPage as P;
        s.settings_page.set(match scene.as_str() {
            "settings-security" => P::Security,
            "settings-shortcuts" => P::Shortcuts,
            "settings-advanced" => P::Advanced,
            "settings-about" => P::About,
            _ => P::General,
        });
    }
    crate::surface_root(s, view().flex_1().child(crate::pages::settings::page(s)).child(crate::pages::sheets::auth_dialog(s)))
}

/// Opens one page for a capture: `SAFAY_DEMO_SCENE` = add-scan | add-key | add-text | export | import |
/// backup | detail | change-password | list | category. Debug demo builds only.
pub fn scene(s: crate::store::Store) {
    use crate::store::{AddTab, Sheet};
    let Ok(scene) = std::env::var("SAFAY_DEMO_SCENE") else { return };
    set_timeout(Duration::from_millis(900), move || {
        let first = |name: &str| s.projection.with(|p| p.rows.iter().find(|r| r.issuer == name).map(|r| r.id));
        match scene.as_str() {
            "add-scan" => s.open_sheet(Sheet::Add),
            "add-key" | "add-text" => {
                s.open_sheet(Sheet::Add);
                s.add_tab.set(if scene == "add-key" { AddTab::Key } else { AddTab::Text });
            }
            "export" => s.open_sheet(Sheet::ExportImport),
            "import" => {
                s.open_sheet(Sheet::ExportImport);
                s.import_tab.set(true);
            }
            "backup" => s.open_sheet(Sheet::Backup),
            "backup-choose" => {
                s.open_sheet(Sheet::Backup);
                s.backup_page.set(crate::store::BackupPage::Choose);
            }
            "backup-dropbox" => {
                s.open_sheet(Sheet::Backup);
                s.open_cloud_setup(safay_app::cloud::Provider::Dropbox);
            }
            "backup-webdav" | "backup-nextcloud" => {
                s.open_sheet(Sheet::Backup);
                s.open_cloud_setup(if scene == "backup-webdav" {
                    safay_app::cloud::Provider::Webdav
                } else {
                    safay_app::cloud::Provider::Nextcloud
                });
            }
            "change-password" => s.open_sheet(Sheet::ChangePassword),
            "detail" => {
                if let Some(id) = first("GitHub") {
                    s.open_sheet(Sheet::Detail(id))
                }
            }
            "list" => s.update_preferences(|p| p.cards = false),
            "category" => {
                let id = s.projection.with(|p| p.categories.iter().find(|c| c.name == "Work").map(|c| c.id));
                s.category.set(id);
                s.refresh();
            }
            _ => {}
        }
    });
}
