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
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
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
    /// iOS: the list is being reordered: its rows show their drag handles and press nothing else.
    pub reordering: Signal<bool>,
}
impl Nav {
    pub fn push(self, page: Page) {
        self.adding.set(false);
        self.menu.set(false);
        self.context.set(None);
        // One change of the routes: the stack and the page apart would show the native stack a route that is neither
        // the old nor the new (`routes`), and it would push and pop pages for it.
        batch(|| {
            if let Some(under) = self.page.get_untracked() {
                self.stack.update(|st| st.push(under));
            }
            self.page.set(Some(page));
        });
    }
    /// The back the user asks for with a page's back button. iOS's navigation stack animates it.
    pub fn pop(self) {
        self.back();
    }
    /// The pages over the list, bottom to top.
    fn routes(self) -> Vec<Page> {
        let mut routes = self.stack.get();
        routes.extend(self.page.get());
        routes
    }
    /// Keeps the first `depth` pages over the list: the system's back (its button, the swipe) popped the rest.
    fn truncate(self, depth: usize) {
        let mut routes = self.routes();
        if routes.len() <= depth {
            return;
        }
        dismiss_keyboard();
        routes.truncate(depth);
        let page = routes.pop();
        // One change: set apart, the page left would show for a moment as the bottom route, and the page under it
        // would be pushed again.
        batch(|| {
            self.stack.set(routes);
            self.page.set(page);
        });
    }
    /// Closes the search field: its text goes, and the keyboard with it.
    pub fn close_search(self) {
        dismiss_keyboard();
        self.searching.set(false);
    }
    /// Back to the list, whatever is over it.
    pub fn home(self) {
        dismiss_keyboard();
        batch(|| {
            self.stack.set(Vec::new());
            self.page.set(None);
        });
    }
    /// One step back; false when the list itself is showing. A page over the list goes first: what is open on the list
    /// (its menus, the search, a letter) stays for when it shows again.
    pub fn back(self) -> bool {
        if self.page.get_untracked().is_some() {
            dismiss_keyboard();
            let mut stack = self.stack.get_untracked();
            let under = stack.pop();
            batch(|| {
                self.stack.set(stack);
                self.page.set(under);
            });
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
    // iOS takes the system's colors; Android (and a desktop showing this interface) Safay's Material ones.
    let palette = if ui::IOS { include_str!("../../theme-ios.json") } else { include_str!("../../theme-mobile.json") };
    set_tokens(nsh_base::Theme::from_json(palette).expect("bundled theme").into_tokens());
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
        reordering: signal(false),
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
            nav.reordering.set(false);
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
    // iOS covers the open vault while the app is in the background: the app switcher shows no codes.
    #[cfg(target_os = "ios")]
    effect(move || crate::platform::ios::set_private(s.unlocked()));
    #[cfg(debug_assertions)]
    if fixture {
        demo_page(s, nav);
    }
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

fn unlocked(s: Store, nav: Nav) -> AnyElement {
    if ui::IOS {
        return native_stack(s, nav).into_element();
    }
    // One branch per page: a page pushed over another (the review over "Enter Setup Key", "Recently Deleted" over the
    // settings) replaces it, which a single branch for "some page" would not notice.
    let pages = [Page::Scan, Page::Manual, Page::Categories, Page::Settings, Page::Deleted, Page::Review, Page::Conflict];
    pages
        .into_iter()
        .fold(view().flex_1().child(when(move || nav.page.get().is_none(), move || home::page(s, nav))), |root, p| {
            root.child(when(move || nav.page.get() == Some(p), move || readable(page(s, nav, p))))
        })
        .into_element()
}
/// A page over the list.
fn page(s: Store, nav: Nav, page: Page) -> AnyElement {
    match page {
        Page::Manual => add::manual(s, nav).into_element(),
        Page::Scan => scan::page(s, nav).into_element(),
        Page::Categories => categories::page(s, nav).into_element(),
        Page::Settings => settings::page(s, nav).into_element(),
        Page::Deleted => settings::deleted(s, nav).into_element(),
        Page::Review => add::review(s, nav).into_element(),
        Page::Conflict => backup::conflict(s, nav).into_element(),
    }
}
/// A page in a column of a readable width, centered in a wide window ([`ui::PAGE_MAX`]); the camera's page takes the
/// whole window.
fn readable(page: AnyElement) -> impl IntoElement {
    view().flex_1().items_center().child(view().flex_1().w_full().max_w(px(ui::PAGE_MAX)).child(page))
}
/// iOS: the list and the pages over it in the system's navigation stack, which pushes and pops them as the system's
/// own (the swipe from the edge included). A page in the system's header shows its title there, large while its
/// content rests at the top; the others keep their own top bar.
fn native_stack(s: Store, nav: Nav) -> impl IntoElement {
    let routes = computed(move || nav.routes().into_iter().enumerate().collect::<Vec<_>>());
    nsh_base::nav_stack()
        .flex_1()
        .child(
            nsh_base::nav_page()
                .key("list")
                // A wide window draws its own sidebar and header (`home::page`).
                .header_visible(move || !ui::is_wide())
                // The app's name at the bar's start, beside its buttons, as plain text that stays: the title's view, as
                // wide as the window, which the system narrows to the room the buttons leave, whatever they are (a
                // leading item would sit on glass, as a button does).
                .title("Safay")
                .header_title(
                    view().w(move || window().width).child(
                        text("Safay")
                            .text_size(px(28.0))
                            .line_height(px(34.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(move || theme().foreground)
                            .number_of_lines(1),
                    ),
                )
                .header_shadow_visible(false)
                .header_right_items(move || home::header_items(s, nav))
                .bg(move || theme().background)
                .child(home::page(s, nav)),
        )
        .child(each(
            move || routes.get(),
            |route: &(usize, Page)| *route,
            move |route, _| {
                let (depth, kind) = route.get_untracked();
                let screen = nsh_base::nav_page()
                    // The page's own background, not the system's: grouped rows stand on it.
                    .bg(move || theme().background)
                    // The system's back (its button, the swipe) leaves the pages from this one up.
                    .on_dismiss(move || nav.truncate(depth));
                let title = match kind {
                    Page::Settings => "Settings",
                    Page::Manual => "Enter Setup Key",
                    Page::Categories => "Manage Categories",
                    Page::Deleted => "Recently Deleted",
                    Page::Review => "Add Account",
                    Page::Conflict => "Resolve Conflict",
                    // The camera's page is the picture alone, with its own close button.
                    Page::Scan => "",
                };
                let screen = if kind == Page::Scan {
                    screen.header_visible(false)
                } else {
                    screen
                        .title(move || s.label(title))
                        // Settings, a place of its own, has the large title; the pages a task opens have the small one.
                        .header_large_title(kind == Page::Settings)
                        .header_shadow_visible(false)
                };
                let screen = if kind == Page::Categories {
                    screen.header_right_items(move || {
                        vec![
                            nsh_core::element::header::HeaderItem::symbol("plus")
                                .key("new-category")
                                .label(s.label("New Category").to_string())
                                .on_press(move || s.editing.set(Some(crate::store::CategoryEdit::default()))),
                        ]
                    })
                } else {
                    screen
                };
                let content = page(s, nav, kind);
                if kind == Page::Scan { screen.child(content) } else { screen.child(readable(content)) }
            },
        ))
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

/// Development: the demo vault opens on the page, sheet or menu `SAFAY_DEMO_PAGE` names, for screenshots of each
/// (`settings`, `categories`, `create`, `restore`, `deleted`, `manual`, `scan`, `review`, `add`, `more`, `context`, `detail`, `qr`,
/// `add-to`, `backup`, `qr`, `fair-password`, `auto-lock`, `language`, `change-password`, `export`, `search`, `work`).
/// RFC 6238's public test key in Base32 ("12345678901234567890").
#[cfg(debug_assertions)]
fn rfc_6238_key() -> String {
    "GEZDGNBVGY3TQOJQ".repeat(2)
}
#[cfg(debug_assertions)]
fn demo_page(s: Store, nav: Nav) {
    let Ok(page) = std::env::var("SAFAY_DEMO_PAGE") else { return };
    // A fresh vault's onboarding: `create`, `restore`.
    if let Some(stage) = match page.as_str() {
        "create" => Some(crate::store::Stage::Create),
        "restore" => Some(crate::store::Stage::Restore),
        _ => None,
    } {
        effect(move || {
            if s.ready.get() {
                untracked(|| s.stage.set(stage));
            }
        });
        return;
    }
    let done = stored(false);
    effect(move || {
        if done.get() || !s.ready.get() || !s.unlocked() || s.projection.with(|p| p.rows.is_empty()) {
            return;
        }
        done.replace(true);
        let page = page.clone();
        untracked(move || {
            let first = s.projection.with(|p| p.rows.first().map(|r| r.id));
            match page.as_str() {
                "settings" => nav.push(Page::Settings),
                "categories" => nav.push(Page::Categories),
                "deleted" => {
                    nav.push(Page::Settings);
                    nav.push(Page::Deleted)
                }
                "manual" => nav.push(Page::Manual),
                "scan" => nav.push(Page::Scan),
                "review" => s.append_scanned(&format!(
                    "otpauth://totp/Example:alex@example.com?secret={k}\notpauth://totp/Sample:ops@example.com?secret={k}",
                    k = rfc_6238_key()
                )),
                "add" => nav.adding.set(true),
                "more" => nav.menu.set(true),
                "context" => nav.context.set(first.map(|id| (id, Point::new(px(120.0), px(240.0))))),
                "detail" => {
                    if let Some(id) = first {
                        s.open_sheet(crate::store::Sheet::Detail(id))
                    }
                }
                "add-to" => nav.add_to.set(first),
                "auto-lock" => nav.dialog.set(Some(Dialog::AutoLock)),
                "language" => nav.dialog.set(Some(Dialog::Language)),
                "change-password" => nav.dialog.set(Some(Dialog::ChangePassword)),
                "export" => nav.dialog.set(Some(Dialog::Export)),
                "search" => nav.searching.set(true),
                // The alert a fair password asks with ("Password Isn’t Strong Enough").
                "fair-password" => s.confirm_fair(safay_app::Zeroizing::new("correcthorse1".to_owned()), |_| {}),
                "backup" => {
                    nav.push(Page::Settings);
                    nav.choosing.set(true)
                }
                "qr" => {
                    if let Some(id) = first {
                        s.reveal(crate::store::Auth::Qr(id))
                    }
                }
                "work" => {
                    let work = s.projection.with(|p| p.categories.iter().find(|c| c.name == "Work").map(|c| c.id));
                    s.category.set(work);
                    s.refresh();
                }
                _ => {}
            }
        });
    });
}
