//! The phone and tablet interface: one job per screen, all starting from the list. It shares the store, the vault and
//! the sync with the desktop interface and has its own pages and controls (`ui`), drawn with `theme-mobile.json`.
mod add;
mod backup;
mod categories;
mod detail;
mod dialogs;
mod home;
mod lock;
mod menus;
mod onboarding;
mod scan;
mod settings;
pub mod ui;

use crate::{components::tok, store::Store};
use nsh_base::theme;
use nsh_core::prelude::*;
use safay_app::controller::Controller;
use std::sync::Arc;

/// A page over the list.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Page {
    /// "Scan QR Code": the camera.
    Scan,
    /// "Enter Setup Key".
    Manual,
    /// "Manage Categories".
    Categories,
    Settings,
    /// "Recently Deleted", from settings.
    Deleted,
    /// "Add Account": what a scan, a photo or pasted links hold, before it is added.
    Review,
    /// "Resolve Conflict", from the backup card in settings.
    Conflict,
}
/// A dialog of settings or of the add menu.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Dialog {
    AutoLock,
    Language,
    ChangePassword,
    Export,
    BackupPassword,
}

/// What the pages of one window share beside the store.
#[derive(Clone, Copy)]
pub struct Nav {
    pub page: Signal<Option<Page>>,
    /// The add menu over the list is open.
    pub adding: Signal<bool>,
    /// The search field is open at the list's bottom.
    pub searching: Signal<bool>,
    /// The letter the list is narrowed to, from its index: only the accounts filed under it show.
    pub letter: Signal<Option<char>>,
    /// The list's overflow menu is open.
    pub menu: Signal<bool>,
    /// The account whose menu is open, and where it was pressed.
    pub context: Signal<Option<(safay_app::Uuid, Point<Points>)>>,
    /// The account "Add to" is choosing categories for.
    pub add_to: Signal<Option<safay_app::Uuid>>,
    pub dialog: Signal<Option<Dialog>>,
    /// "Choose Location" is open.
    pub choosing: Signal<bool>,
    /// The master password of a backup this vault cannot open is asked for.
    pub restoring: Signal<bool>,
    /// The pages under the one showing, the list excepted.
    pub stack: Signal<Vec<Page>>,
}
impl Nav {
    pub fn push(self, page: Page) {
        self.adding.set(false);
        self.menu.set(false);
        self.context.set(None);
        if let Some(under) = self.page.get_untracked() {
            self.stack.update(|st| st.push(under));
        }
        self.page.set(Some(page));
    }
    /// Closes the search field: its text goes, and the keyboard with it.
    pub fn close_search(self) {
        dismiss_keyboard();
        self.searching.set(false);
    }
    /// Back to the list, whatever is over it.
    pub fn home(self) {
        dismiss_keyboard();
        self.stack.set(Vec::new());
        self.page.set(None);
    }
    /// One step back; false when the list itself is showing. A page over the list goes first: what is open on the list
    /// (its menus, the search, a letter) stays for when it shows again.
    pub fn back(self) -> bool {
        if self.page.get_untracked().is_some() {
            dismiss_keyboard();
            let mut stack = self.stack.get_untracked();
            let under = stack.pop();
            self.stack.set(stack);
            self.page.set(under);
        } else if self.context.get_untracked().is_some() {
            self.context.set(None);
        } else if self.menu.get_untracked() {
            self.menu.set(false);
        } else if self.adding.get_untracked() {
            self.adding.set(false);
        } else if self.searching.get_untracked() {
            self.close_search();
        } else if self.letter.get_untracked().is_some() {
            self.letter.set(None);
        } else {
            return false;
        }
        true
    }
}

pub fn root(controller: Arc<Controller>, fixture: bool) -> impl IntoElement {
    set_tokens(nsh_base::Theme::from_json(include_str!("../../theme-mobile.json")).expect("bundled theme").into_tokens());
    let s = Store::new(controller, fixture, crate::store::Surface::Main);
    effect(move || untracked(|| s.start()));
    let nav = Nav {
        page: signal(None),
        adding: signal(false),
        searching: signal(false),
        letter: signal(None),
        menu: signal(false),
        context: signal(None),
        add_to: signal(None),
        dialog: signal(None),
        stack: signal(Vec::new()),
        choosing: signal(false),
        restoring: signal(false),
    };
    // Locking leaves no page over the list for the next unlock.
    effect(move || {
        if !s.unlocked() {
            nav.page.set(None);
            nav.adding.set(false);
            nav.searching.set(false);
            nav.letter.set(None);
            nav.menu.set(false);
            nav.context.set(None);
            nav.add_to.set(None);
            nav.dialog.set(None);
            nav.stack.set(Vec::new());
            nav.choosing.set(false);
            nav.restoring.set(false);
        }
    });
    // A backup file was chosen: one that needs no password is imported at once, another asks for its password.
    effect(move || {
        if !s.has_file() || !s.unlocked() {
            return;
        }
        untracked(|| match s.file_kind.get() {
            safay_app::core::import::FileKind::Encrypted => nav.dialog.set(Some(Dialog::BackupPassword)),
            safay_app::core::import::FileKind::Unsupported => {
                s.remove_file();
                s.show_toast(crate::store::Tone::Error, s.label("Not a supported backup file"));
            }
            _ => s.import_file(),
        });
    });
    // The file's password is asked only while there is a file: an import that worked lets it go (with the sheet it
    // closes), and so does "Cancel".
    effect(move || {
        if !s.has_file() && untracked(|| nav.dialog.get()) == Some(Dialog::BackupPassword) {
            untracked(|| nav.dialog.set(None));
        }
    });
    // Leaving "Add Account" by any way (its back button, the back key, locking) leaves what it held: the review below
    // would otherwise open again at the next refresh.
    let reviewed = stored(false);
    effect(move || {
        let reviewing = nav.page.get() == Some(Page::Review);
        if reviewed.replace(reviewing) && !reviewing {
            untracked(|| s.scanned.set(Default::default()));
        }
    });
    // What the store reports in `status` shows in the restore dialog and on the conflict page; anywhere else on the
    // unlocked phone it has no place of its own: a toast.
    effect(move || {
        let Some(status) = s.status.get() else { return };
        if !s.unlocked() || untracked(|| nav.restoring.get() || nav.page.get() == Some(Page::Conflict)) {
            return;
        }
        untracked(|| {
            s.status.set(None);
            s.show_toast(status.tone, status.text);
        });
    });
    // Codes read from photos or the camera wait in the scanned list: they are reviewed before they are added.
    effect(move || {
        if s.unlocked() && !s.scanned.with(|t| t.trim().is_empty()) && untracked(|| nav.page.get() != Some(Page::Review)) {
            untracked(|| nav.push(Page::Review));
        }
    });
    // The unlocked vault stays out of screenshots, recordings and the recent apps' pictures, except in development
    // packages (`scripts/android.py` marks them; `nsh package` builds the crate without debug assertions).
    #[cfg(target_os = "android")]
    effect(move || {
        let unlocked = s.unlocked();
        if option_env!("SAFAY_ALLOW_SCREENSHOTS").is_none() {
            crate::platform::android::set_secure(unlocked);
        }
    });
    // Dialogs and sheets are modals of their own: the back key closes them through their `on_cancel` and never comes
    // here while one is up.
    on_back_press(move || {
        if s.unlocked() {
            return nav.back();
        }
        // Onboarding: a step back, as its top bar takes.
        if !s.projection.with_untracked(|p| p.exists) {
            return onboarding::back(s);
        }
        false
    });
    view()
        .flex_1()
        .font_family("system-ui")
        .font_weight(FontWeight::NORMAL)
        .text_size(px(14.0))
        .bg(move || theme().background)
        .text_color(move || theme().foreground)
        .on_press(move |_: &()| s.controller.get().activity())
        .child(when(move || !s.ready.get(), || view().flex_1()).otherwise(move || {
            when(move || s.unlocked(), move || unlocked(s, nav)).otherwise(move || {
                when(move || s.projection.with(|p| p.exists), move || lock::page(s)).otherwise(move || onboarding::page(s))
            })
        }))
        .child(when(move || s.unlocked(), move || view().child(detail::sheet(s)).child(menus::add_to(s, nav))))
        .child(when(move || s.unlocked(), move || backup::dialogs(s, nav)))
        .child(backup::setup_sheet(s))
        .child(dialogs::all(s))
        .child(dialogs::of(s, nav))
        // Above the add button while the list shows.
        .child(toast(s, move || s.unlocked() && nav.page.get().is_none()))
}

fn unlocked(s: Store, nav: Nav) -> impl IntoElement {
    view()
        .flex_1()
        .child(when(move || nav.page.get().is_none(), move || home::page(s, nav)))
        .child(when(move || nav.page.get() == Some(Page::Manual), move || add::manual(s, nav)))
        .child(when(move || nav.page.get() == Some(Page::Scan), move || scan::page(s, nav)))
        .child(when(move || nav.page.get() == Some(Page::Categories), move || categories::page(s, nav)))
        .child(when(move || nav.page.get() == Some(Page::Settings), move || settings::page(s, nav)))
        .child(when(move || nav.page.get() == Some(Page::Deleted), move || settings::deleted(s, nav)))
        .child(when(move || nav.page.get() == Some(Page::Review), move || add::review(s, nav)))
        .child(when(move || nav.page.get() == Some(Page::Conflict), move || backup::conflict(s, nav)))
}

/// Runs `then` each time the store finishes `done` from now on, while the calling page or dialog is built.
fn on_done(s: Store, done: crate::store::Done, then: impl Fn() + 'static) {
    let seen = s.done.with_untracked(|d| d.map(|(n, _)| n));
    effect(move || {
        if let Some((n, finished)) = s.done.get()
            && Some(n) != seen
            && finished == done
        {
            untracked(&then);
        }
    });
}

/// The calling page or dialog shows `s.error` itself while it is built: errors go to it alone, not to a toast too.
fn shows_errors(s: Store) {
    s.inline_errors.update(|n| *n += 1);
    cleanup(move || s.inline_errors.update(|n| *n = n.saturating_sub(1)));
}

/// One line at the bottom about what just happened, above the navigation bar; it takes no input.
fn toast(s: Store, lifted: impl Fn() -> bool + Copy + 'static) -> impl IntoElement {
    when(
        move || s.toast.get().is_some(),
        move || {
            view()
                .absolute()
                .left(px(0.0))
                .right(px(0.0))
                .bottom(move || px(if lifted() { ui::FAB + ui::MARGIN } else { 0.0 }))
                .pb_safe_area_plus(px(16.0))
                .px(px(ui::MARGIN))
                .items_center()
                .pointer_events(nsh_core::PointerEvents::None)
                .child(
                    view()
                        .min_h(px(ui::TOUCH))
                        .max_w(px(480.0))
                        .px(px(16.0))
                        .py(px(12.0))
                        .justify_center()
                        .rounded(px(12.0))
                        .bg(move || tok("glass"))
                        .shadow_lg()
                        .child(
                            ui::body(text(move || s.toast.get().map(|t| t.text).unwrap_or_default()))
                                .text_color(move || tok("glass-foreground")),
                        ),
                )
        },
    )
}
