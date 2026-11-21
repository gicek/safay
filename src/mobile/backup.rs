//! Backup and sync on a phone: the card in settings, the history, choosing a location, connecting a cloud service (a
//! sheet from the bottom), resolving a conflict and, on first run, restoring a backup. The store does the work; these
//! are its pages.
use super::{
    Nav, Page,
    ui::{self, Entry},
};
use crate::{
    components::tok,
    icons::{Icon, icon},
    pages::backup::{LocationIcon, location_mark, oauth_configured},
    store::{BackupPage, Sheet, Stage, Store},
};
use nsh_base::{button, theme};
use nsh_core::prelude::*;
use safay_app::{
    Zeroizing,
    cloud::Provider,
    core::merge::AccountChange,
    sync::{Direction, HistoryEntry, Location, Source, Status, SyncError},
};

/// The cloud services a phone offers, in order: those this build has a client for, then the servers.
fn providers() -> Vec<Provider> {
    [Provider::Dropbox, Provider::Onedrive, Provider::GoogleDrive, Provider::Nextcloud, Provider::Webdav]
        .into_iter()
        .filter(|p| oauth_configured(*p))
        .collect()
}
fn is_oauth(provider: Provider) -> bool {
    matches!(provider, Provider::Dropbox | Provider::Onedrive | Provider::GoogleDrive)
}
fn status(s: Store) -> Status {
    s.projection.with(|p| p.sync.status)
}
const WAITING: Status = Status::Failed(SyncError::ManualWriteRequired);

/// A service's mark, on the plate it draws itself: 40 pt.
fn plate(kind: LocationIcon) -> impl IntoElement {
    view().size(px(40.0)).flex_shrink(0.0).child(location_mark(kind, 40.0))
}

/// Opens the connection sheet for `provider`: linking it as the backup location, or (`restore`) looking for a backup
/// there on first run.
fn open_setup(s: Store, provider: Provider, restore: bool) {
    if restore {
        s.choose_restore_cloud(provider);
    } else {
        s.open_cloud_setup(provider);
        s.sheet.set(Some(Sheet::Backup));
    }
}

// ---- Settings ------------------------------------------------------------------------------------------------------

/// "Sync & Backup" and "Backup History", for settings.
pub fn section(s: Store, nav: Nav) -> impl IntoElement {
    let configured = move || s.projection.with(|p| p.sync.location.is_some());
    view()
        .child(ui::section_title(move || s.label("Sync & Backup")))
        .child(
            view()
                .p(px(20.0))
                .gap(px(12.0))
                .rounded(px(ui::GROUP_RADIUS))
                .bg(move || theme().card)
                .child(when(configured, move || linked(s, nav)).otherwise(move || {
                    view()
                        .gap(px(12.0))
                        .child(ui::dim(ui::body(text(move || {
                            s.label("Backups are end-to-end encrypted with your master password; only you can open them.")
                        }))))
                        .child(
                            view().flex_row().child(
                                ui::filled_button(Some(Icon::Cloud), move || s.label("Set Up"))
                                    .test_id("backup-set-up")
                                    .on_press(move || nav.choosing.set(true)),
                            ),
                        )
                }))
                .child(when(move || s.refused.with(Option::is_some), move || refused(s))),
        )
        .child(when(move || s.projection.with(|p| !p.sync.history.is_empty()), move || history(s)))
}

fn refused(s: Store) -> impl IntoElement {
    view()
        .flex_row()
        .gap(px(10.0))
        .p(px(14.0))
        .rounded(px(16.0))
        .bg(move || tok("urgent").opacity(0.12))
        .child(view().text_color(move || tok("urgent")).child(icon(Icon::Warning).size(px(20.0))))
        .child(ui::body(text(move || {
            let name = s.refused.get().unwrap_or_default();
            s.fmt(
                "{} wasn’t connected: the backup there belongs to another vault, or its master password was changed on another device. To back up this vault there, delete that backup first.",
                &[&name],
            )
        }))
        .flex_1())
}

/// The linked location: its name and status, and what can be done now.
fn linked(s: Store, nav: Nav) -> impl IntoElement {
    let st = move || status(s);
    let location = move || s.projection.with(|p| p.sync.location.clone());
    let mark = move || match location() {
        Some(Location::Cloud { provider, .. }) => LocationIcon::Cloud(provider),
        Some(Location::ICloud { .. }) => LocationIcon::Icloud,
        _ => LocationIcon::Folder,
    };
    let detail = move || match location() {
        Some(Location::Cloud { provider, account, .. }) if is_oauth(provider) => {
            let folder = match provider {
                Provider::GoogleDrive => s.fmt("My Drive › {}", &[option_env!("SAFAY_GOOGLE_FOLDER_NAME").unwrap_or_default()]),
                _ => s.label("Apps › Safay"),
            };
            SharedString::from(format!("{account} · {folder}"))
        }
        Some(other) => other.detail().into(),
        None => SharedString::default(),
    };
    let line = move || match st() {
        Status::Running => s.label("Syncing"),
        Status::Conflict => s.label("Sync Conflict Detected"),
        Status::Unreadable => s.label("Backup Can’t Be Opened"),
        WAITING => s.label("Waiting for your approval"),
        Status::Failed(e) => s.label(crate::store::sync_error_key(e)),
        _ => s
            .projection
            .with(|p| p.sync.last_synced_ms)
            .map(|ms| {
                s.fmt(
                    "Last synced {}",
                    &[&crate::platform::local_time::date_time(std::time::UNIX_EPOCH + std::time::Duration::from_millis(ms.max(0) as u64))],
                )
            })
            .unwrap_or_else(detail),
    };
    let tint = move || match st() {
        Status::Conflict | WAITING => tok("urgent"),
        Status::Unreadable | Status::Failed(_) => theme().destructive,
        _ => theme().muted_foreground,
    };
    let busy = move || s.syncing.get();
    view()
        .gap(px(12.0))
        .child(
            view()
                .flex_row()
                .items_center()
                .gap(px(12.0))
                .child(dynamic(move || plate(mark()).into_element()))
                .child(
                    view()
                        .flex_1()
                        .min_w(px(0.0))
                        .child(ui::title(text(move || location().map(|l| SharedString::from(l.title())).unwrap_or_default())).number_of_lines(1))
                        .child(ui::caption(text(detail)).number_of_lines(1).text_color(move || theme().muted_foreground))
                        .child(ui::caption(text(line)).number_of_lines(2).text_color(tint)),
                )
                .child(when(busy, || crate::components::spinner(20.0))),
        )
        .child(when(
            move || location().is_some_and(|l| matches!(l, Location::Cloud { pinned: true, .. })),
            move || warning_line(s, Icon::ShieldWarningFill, "Certificate not verified"),
        ))
        .child(when(
            move || st() == WAITING,
            move || {
                notice(
                    s,
                    move || {
                        let name = location().map(|l| l.title()).unwrap_or_default();
                        s.fmt("{} can’t check for changes from another device during an upload, so Safay asks before it replaces the backup.", &[&name])
                    },
                    "Upload",
                    move || s.approve_upload(),
                )
            },
        ))
        .child(when(
            move || st() == Status::Conflict,
            move || {
                notice(
                    s,
                    move || s.label("This device and the backup both changed. Compare the accounts on each side, then keep one copy or merge both."),
                    "Resolve Conflict",
                    move || {
                        s.status.set(None);
                        s.prefer_remote.set(Default::default());
                        s.form.update(|f| f.algorithm = 0);
                        s.load_review();
                        nav.push(Page::Conflict);
                    },
                )
            },
        ))
        .child(when(
            move || st() == Status::Unreadable,
            move || {
                notice(
                    s,
                    move || {
                        s.label("The backup here can’t be opened with this vault’s key. It belongs to another vault, or its master password was changed on another device. To use that backup, restore from it and unlock with its master password. This replaces the vault on this device.")
                    },
                    "Restore from Backup",
                    move || {
                        s.status.set(None);
                        s.form.update(|f| f.password = Zeroizing::new(String::new()));
                        nav.restoring.set(true);
                    },
                )
            },
        ))
        .child(when(
            move || !matches!(st(), Status::Conflict | WAITING),
            move || {
                view()
                    .flex_row()
                    .flex_wrap()
                    .gap(px(8.0))
                    .child(when(
                        move || st() != Status::Unreadable,
                        move || {
                            ui::filled_button(None, move || s.label(if busy() { "Syncing" } else { "Back Up Now" }))
                                .test_id("back-up-now")
                                .disabled(move || busy() || !s.unlocked())
                                .on_press(move || s.sync_now(Source::Manual))
                        },
                    ))
                    .child(
                        ui::outlined_button(move || s.label("Change Location"))
                            .test_id("change-location")
                            .disabled(busy)
                            .on_press(move || nav.choosing.set(true)),
                    )
            },
        ))
        .child(
            view().flex_row().child(
                ui::text_button(move || s.label("Turn Off Backup"), true).test_id("turn-off").disabled(busy).on_press(move || s.turn_off_sync()),
            ),
        )
}
fn warning_line(s: Store, glyph: Icon, label: &'static str) -> impl IntoElement {
    view()
        .flex_row()
        .items_center()
        .gap(px(6.0))
        .text_color(move || theme().destructive)
        .child(icon(glyph).size(px(16.0)))
        .child(ui::label(text(move || s.label(label))))
}
/// A state that needs the user: what happened, and the one action that answers it.
fn notice(s: Store, message: impl Fn() -> SharedString + 'static, action: &'static str, press: impl Fn() + 'static) -> impl IntoElement {
    view()
        .gap(px(12.0))
        .child(
            view()
                .flex_row()
                .gap(px(10.0))
                .child(view().pt(px(2.0)).text_color(move || tok("urgent")).child(icon(Icon::Warning).size(px(20.0))))
                .child(ui::body(text(message)).flex_1()),
        )
        .child(ui::filled_button(None, move || s.label(action)).test_id(action).disabled(move || s.syncing.get()).on_press(press))
}

fn source_key(source: Source) -> &'static str {
    match source {
        Source::Auto => "Auto Sync",
        Source::Manual => "Manual Backup",
        Source::Restore => "Restore from Backup",
        Source::Merge => "Merge",
    }
}
/// The last ten records.
fn history(s: Store) -> impl IntoElement {
    let entries = move || s.projection.with(|p| p.sync.history.iter().take(10).cloned().enumerate().collect::<Vec<_>>());
    let count = move || s.projection.with(|p| p.sync.history.len().min(10));
    view().child(ui::section_title(move || s.label("Backup History"))).child(view().gap(px(ui::GROUP_GAP)).child(each(
        entries,
        |(i, e)| format!("{i}:{}", e.at_ms),
        move |item, index| {
            let e = move || item.with(|x| x.1.clone());
            ui::group_corners(
                view().flex_row().items_center().gap(px(14.0)).px(px(16.0)).py(px(12.0)).min_h(px(64.0)).bg(move || theme().card),
                move || index.get(),
                count,
            )
            .child(history_icon(e))
            .child(
                view()
                    .flex_1()
                    .min_w(px(0.0))
                    .child(ui::body_large(text(move || history_title(s, &e()))).number_of_lines(1))
                    .child(ui::dim(ui::caption(text(move || history_detail(s, &e())))).number_of_lines(1)),
            )
            .child(
                view()
                    .items_end()
                    .child(ui::caption(text(move || {
                        SharedString::from(crate::platform::local_time::date_time(
                            std::time::UNIX_EPOCH + std::time::Duration::from_millis(e().at_ms.max(0) as u64),
                        ))
                    })))
                    .child(ui::dim(ui::caption(text(move || {
                        SharedString::from(
                            e().bytes
                                .map(|b| if b < 1024 { format!("{b} B") } else { format!("{:.1} KB", b as f64 / 1024.0) })
                                .unwrap_or_default(),
                        )
                    })))),
            )
        },
    )))
}
fn history_icon(e: impl Fn() -> HistoryEntry + Copy + 'static) -> impl IntoElement {
    let ink = move || if e().failed { theme().destructive } else { theme().muted_foreground };
    view().size(px(32.0)).rounded_full().items_center().justify_center().bg(move || ink().opacity(0.12)).text_color(ink).child(dynamic(
        move || {
            let glyph = if e().failed {
                Icon::Warning
            } else {
                match e().direction {
                    Some(Direction::Upload) => Icon::Cloud,
                    Some(Direction::Download) => Icon::Download,
                    Some(Direction::Unchanged) => Icon::Check,
                    None => Icon::Refresh,
                }
            };
            icon(glyph).size(px(18.0)).into_element()
        },
    ))
}
fn history_title(s: Store, e: &HistoryEntry) -> SharedString {
    if e.failed {
        return s.fmt("{} failed", &[&s.label(source_key(e.source))]);
    }
    s.label(match e.direction {
        Some(Direction::Upload) => "Uploaded",
        Some(Direction::Download) => "Downloaded",
        Some(Direction::Unchanged) => "No changes",
        None => source_key(e.source),
    })
}
fn history_detail(s: Store, e: &HistoryEntry) -> SharedString {
    let mut parts = vec![e.location.clone()];
    if !e.failed {
        parts.push(s.label(source_key(e.source)).to_string());
    }
    if e.direction != Some(Direction::Upload) && !e.failed && e.device != crate::platform::device_name() && e.device != e.location {
        parts.push(s.fmt("From {}", &[&e.device]).to_string());
    }
    parts.retain(|p| !p.is_empty());
    parts.join(" · ").into()
}

// ---- Choosing a location, restoring from an unreadable backup --------------------------------------------------------

/// The dialogs of the card: where to keep the backup, and the master password of a backup this vault cannot open.
pub fn dialogs(s: Store, nav: Nav) -> impl IntoElement {
    view()
        .child(when(move || nav.choosing.get(), move || choose(s, nav)))
        .child(when(move || nav.restoring.get(), move || restore_unreadable(s, nav)))
}

fn choose(s: Store, nav: Nav) -> impl IntoElement {
    let close = move || nav.choosing.set(false);
    let current = move || match s.projection.with(|p| p.sync.location.clone()) {
        Some(Location::Cloud { provider, .. }) => Some(provider),
        _ => None,
    };
    ui::dialog(
        move || s.label("Choose Location"),
        view()
            .mx(px(-24.0))
            .child(
                ui::dim(ui::body(text(move || s.label("Choose where to keep the encrypted backup. Only one location is used at a time."))))
                    .px(px(24.0))
                    .pb(px(8.0)),
            )
            .children(providers().into_iter().map(move |provider| {
                button()
                    .test_id(provider.name())
                    .min_h(px(56.0))
                    .px(px(24.0))
                    .flex_row()
                    .items_center()
                    .gap(px(16.0))
                    .active(|st| st.bg(move || tok("pressed")))
                    .on_press(move || {
                        close();
                        open_setup(s, provider, false)
                    })
                    .child(plate(LocationIcon::Cloud(provider)))
                    .child(ui::body_large(text(provider.name())).flex_1())
                    .child(when(
                        move || current() == Some(provider),
                        || view().text_color(move || theme().primary).child(icon(Icon::Check).size(px(24.0))),
                    ))
            })),
        ui::text_button(move || s.label("Cancel"), false).on_press(close),
        close,
    )
}

fn restore_unreadable(s: Store, nav: Nav) -> impl IntoElement {
    let close = move || {
        if !s.syncing.get() {
            s.form.update(|f| f.password = Zeroizing::new(String::new()));
            nav.restoring.set(false)
        }
    };
    let submit = move || {
        if s.form.with(|f| !f.password.is_empty()) && !s.syncing.get() {
            dismiss_keyboard();
            s.restore_unreadable();
        }
    };
    // Restored: the backup opens with this vault's key now.
    effect(move || {
        if status(s) != Status::Unreadable && !s.syncing.get() {
            untracked(|| nav.restoring.set(false));
        }
    });
    ui::dialog(
        move || s.label("Restore from Backup"),
        view()
            .gap(px(8.0))
            .child(ui::dim(ui::body(text(move || {
                s.label("To use that backup, restore from it and unlock with its master password. This replaces the vault on this device.")
            }))))
            .child(ui::field(
                s,
                "restore-password",
                Entry::Password,
                move || s.label("Existing Master Password"),
                move || s.form.with(|f| f.password.to_string()),
                move |v| {
                    s.status.set(None);
                    s.form.update(|f| f.password = Zeroizing::new(v.to_owned()))
                },
                submit,
                |_| {},
                move || s.status.get().is_some(),
                ReturnKeyType::Done,
                None,
            ))
            .child(ui::field_note(move || s.status.get().map(|st| st.text), || true)),
        view().flex_row().gap(px(4.0)).child(ui::text_button(move || s.label("Cancel"), false).on_press(close)).child(
            ui::text_button(move || s.label("Restore"), false).test_id("restore-go").disabled(move || s.syncing.get()).on_press(submit),
        ),
        close,
    )
}

// ---- Connecting a cloud service --------------------------------------------------------------------------------------

/// The provider being connected, while the connection sheet is up.
fn connecting(s: Store) -> Option<Provider> {
    match (s.sheet.get(), s.backup_page.get()) {
        (Some(Sheet::Backup), BackupPage::Setup(provider)) => Some(provider),
        _ => None,
    }
}

/// The connection sheet: over the list or over first run's restore page.
pub fn setup_sheet(s: Store) -> impl IntoElement {
    // Linked: the store comes back to its overview, and the sheet goes.
    effect(move || {
        if s.sheet.get() == Some(Sheet::Backup) && !matches!(s.backup_page.get(), BackupPage::Setup(_)) {
            untracked(|| s.sheet.set(None));
        }
    });
    let close = move || {
        // After the browser came back, the sign-in finishes: nothing to leave then.
        if s.cloud_form.with(|f| f.working && !f.waiting && !matches!(connecting(s), Some(Provider::Nextcloud | Provider::Webdav))) {
            return;
        }
        dismiss_keyboard();
        s.close_sheet();
    };
    view()
        .child(ui::bottom_sheet(
            move || connecting(s).is_some(),
            close,
            dynamic(move || connecting(s).map(|p| setup(s, p, close).into_element())),
        ))
        .child(when(move || s.cloud_form.with(|f| f.needs_consent) && connecting(s).is_some(), move || insecure(s)))
        .child(when(move || s.cloud_form.with(|f| f.untrusted.is_some()) && connecting(s).is_some(), move || certificate(s)))
}

fn setup(s: Store, provider: Provider, close: impl Fn() + Copy + 'static) -> impl IntoElement {
    let restore = move || s.cloud_restore.get();
    let oauth = is_oauth(provider);
    let ready = move || {
        oauth
            || s.cloud_form.with(|f| {
                !f.address.trim().is_empty() && (provider != Provider::Webdav || (!f.username.trim().is_empty() && !f.password.is_empty()))
            })
    };
    let go = move || {
        if !ready() || s.cloud_form.with(|f| f.working) {
            return;
        }
        dismiss_keyboard();
        if oauth { s.connect_oauth(provider) } else { s.connect_cloud(provider) }
    };
    let waiting = move || s.cloud_form.with(|f| f.waiting);
    // The title row (48), and the action under the scroll view (12 above a 56 pt button, 12 below it).
    const CHROME: f32 = 48.0 + 12.0 + 56.0 + 12.0;
    view()
        .pb(move || ui::sheet_bottom(12.0))
        .child(
            view()
                .flex_row()
                .items_center()
                .pl(px(24.0))
                .pr(px(8.0))
                .child(
                    ui::title_large(text(move || s.fmt(if restore() { "Restore from {}" } else { "Connect to {}" }, &[provider.name()])))
                        .flex_1(),
                )
                .child(ui::icon_button(move || s.label("Close"), || Icon::Close).on_press(close)),
        )
        .child(
            scroll()
                .max_h(move || ui::sheet_room(CHROME))
                .keyboard_dismiss_mode(KeyboardDismissMode::None)
                .keyboard_taps(KeyboardTaps::Handled)
                .child(
                    view()
                        .px(px(24.0))
                        .pt(px(8.0))
                        .gap(px(20.0))
                        .child(pairing(s, provider))
                        .child(when(move || oauth, move || oauth_body(s, provider)).otherwise(move || server_body(s, provider, go)))
                        .child(when(
                            move || s.cloud_form.with(|f| f.error.is_some()),
                            move || {
                                ui::body(text(move || s.cloud_form.with(|f| f.error.clone().unwrap_or_default())))
                                    .text_color(move || theme().destructive)
                            },
                        )),
                ),
        )
        .child(
            view().px(px(24.0)).pt(px(12.0)).child(
                when(waiting, move || {
                    ui::outlined_button(move || s.label("Cancel")).on_press(move || {
                        crate::platform::cancel_web_sign_in();
                        s.cloud_form.update(|f| {
                            f.working = false;
                            f.waiting = false;
                        })
                    })
                })
                .otherwise(move || {
                    ui::primary_button(
                        move || s.label(if oauth { "Sign In" } else { "Connect" }),
                        ready,
                        move || s.cloud_form.with(|f| f.working),
                    )
                    .test_id("connect")
                    .on_press(go)
                }),
            ),
        )
}

/// Safay, a locked link and the service; what is kept there.
fn pairing(s: Store, provider: Provider) -> impl IntoElement {
    let plate56 = |mark: AnyElement| {
        view().size(px(56.0)).rounded(px(18.0)).items_center().justify_center().bg(move || tok("surface-high")).child(mark)
    };
    let dash = || view().w(px(14.0)).h(px(2.0)).bg(move || theme().border);
    view()
        .items_center()
        .gap(px(12.0))
        .child(
            view()
                .flex_row()
                .items_center()
                .gap(px(6.0))
                .child(plate56(crate::components::app_icon(40.0).into_element()))
                .child(dash())
                .child(
                    view()
                        .size(px(22.0))
                        .rounded_full()
                        .items_center()
                        .justify_center()
                        .bg(move || tok("surface-highest"))
                        .text_color(move || theme().muted_foreground)
                        .child(icon(Icon::LockFill).size(px(12.0))),
                )
                .child(dash())
                .child(location_mark(LocationIcon::Cloud(provider), 56.0).into_element()),
        )
        .child(
            ui::title(text(move || {
                s.label(match provider {
                    Provider::Webdav => "Connect to your own server or NAS",
                    Provider::Nextcloud => "Sign in with your Nextcloud account",
                    Provider::Dropbox => "Sign in with your Dropbox account",
                    Provider::Onedrive => "Sign in with your Microsoft account",
                    _ => "Sign in with your Google account",
                })
            }))
            .text_center(),
        )
        .child(
            ui::dim(ui::body(text(move || match (s.cloud_restore.get(), provider == Provider::Webdav) {
                (false, true) => s.label("Only the encrypted vault is stored on your server."),
                (false, false) => s.fmt("Only the encrypted vault is stored in {}.", &[provider.name()]),
                (true, true) => s.label("Safay only reads your encrypted backup from your server."),
                (true, false) => s.fmt("Safay only reads your encrypted backup from {}.", &[provider.name()]),
            })))
            .text_center(),
        )
}

fn oauth_body(s: Store, provider: Provider) -> impl IntoElement {
    let folder = move || match provider {
        Provider::GoogleDrive => s.fmt("My Drive › {}", &[option_env!("SAFAY_GOOGLE_FOLDER_NAME").unwrap_or_default()]),
        _ => s.label("Apps › Safay"),
    };
    let phase = move || {
        s.cloud_form.with(|f| {
            if f.waiting {
                1
            } else if f.checking {
                3
            } else if f.working {
                2
            } else {
                0
            }
        })
    };
    view()
        .gap(px(20.0))
        .child(
            view()
                .flex_row()
                .gap(px(14.0))
                .p(px(16.0))
                .rounded(px(24.0))
                .bg(move || theme().secondary)
                .child(view().text_color(move || theme().primary).child(icon(Icon::FolderFill).size(px(22.0))))
                .child(view().flex_1().gap(px(2.0)).child(ui::label(text(folder))).child(ui::dim(ui::body(text(move || {
                    let key = if s.cloud_restore.get() {
                        "Safay looks for your backup in this folder in your {}."
                    } else {
                        "Your backup is kept in this folder in your {}."
                    };
                    s.fmt(key, &[provider.name()])
                }))))),
        )
        .child(when(
            move || provider == Provider::GoogleDrive && !s.cloud_restore.get(),
            move || {
                ui::dim(ui::caption(text(move || {
                    s.fmt("After the first backup, Safay asks before it uploads changes to {}.", &[provider.name()])
                })))
            },
        ))
        .child(steps(
            vec![
                Box::new(move || s.fmt("Sign in to {} and allow Safay", &[provider.name()])),
                Box::new(move || s.fmt("Connect to your {} account", &[provider.name()])),
                Box::new(move || {
                    s.label(if s.cloud_restore.get() { "Find your backup" } else { "Look for an existing backup, then back up" })
                }),
            ],
            phase,
        ))
}

/// Numbered steps that become progress: done ones ticked, the current one with a spinner, later ones dimmed.
fn steps(labels: Vec<Box<dyn Fn() -> SharedString>>, phase: impl Fn() -> usize + Copy + 'static) -> impl IntoElement {
    view().gap(px(14.0)).children(labels.into_iter().enumerate().map(move |(i, label)| {
        let n = i + 1;
        let done = move || phase() > n;
        let active = move || phase() == n;
        let pending = move || phase() > 0 && phase() < n;
        view()
            .flex_row()
            .items_center()
            .gap(px(14.0))
            .child(view().size(px(24.0)).items_center().justify_center().child(dynamic(move || {
                if done() {
                    view().text_color(move || theme().primary).child(icon(Icon::CheckCircleFill).size(px(22.0))).into_element()
                } else if active() {
                    crate::components::spinner(18.0).into_element()
                } else {
                    view()
                        .size(px(22.0))
                        .rounded_full()
                        .items_center()
                        .justify_center()
                        .border_1()
                        .border_color(move || tok("outline"))
                        .child(ui::caption(text(n.to_string())).text_color(move || theme().muted_foreground))
                        .into_element()
                }
            })))
            .child(ui::body_large(text(label)).flex_1().opacity(move || if pending() { 0.55_f32 } else { 1.0 }))
    }))
}

fn server_body(s: Store, provider: Provider, go: impl Fn() + Copy + 'static) -> impl IntoElement {
    let username = element_ref();
    let password = element_ref();
    let webdav = provider == Provider::Webdav;
    view()
        .gap(px(12.0))
        .child(ui::field(
            s,
            "server-address",
            Entry::Text,
            move || s.label("Server Address"),
            move || s.cloud_form.with(|f| f.address.clone()),
            move |v| {
                s.cloud_form.update(|f| {
                    f.address = v.to_owned();
                    f.insecure = false;
                    f.trusted = None;
                    f.error = None;
                })
            },
            move || if webdav { username.focus() } else { go() },
            |_| {},
            || false,
            if webdav { ReturnKeyType::Next } else { ReturnKeyType::Go },
            None,
        ))
        .child(
            ui::dim(ui::caption(text(move || {
                s.label(if webdav {
                    "The WebDAV address of the folder, for example https://example.com/remote.php/webdav/"
                } else {
                    "For example https://cloud.example.com. You’ll sign in and allow Safay in your browser."
                })
            })))
            .px(px(16.0)),
        )
        .child(when(
            move || webdav,
            move || {
                view()
                    .gap(px(12.0))
                    .child(ui::field(
                        s,
                        "server-username",
                        Entry::Text,
                        move || s.label("Username"),
                        move || s.cloud_form.with(|f| f.username.clone()),
                        move |v| s.cloud_form.update(|f| f.username = v.to_owned()),
                        move || password.focus(),
                        |_| {},
                        || false,
                        ReturnKeyType::Next,
                        Some(username),
                    ))
                    .child(ui::field(
                        s,
                        "server-password",
                        Entry::Password,
                        move || s.label("Password"),
                        move || s.cloud_form.with(|f| f.password.to_string()),
                        move |v| s.cloud_form.update(|f| f.password = Zeroizing::new(v.to_owned())),
                        go,
                        |_| {},
                        || false,
                        ReturnKeyType::Go,
                        Some(password),
                    ))
            },
        ))
        .child(when(
            move || s.cloud_form.with(|f| f.waiting),
            move || {
                view()
                    .gap(px(8.0))
                    .child(
                        view()
                            .flex_row()
                            .items_center()
                            .gap(px(10.0))
                            .child(crate::components::spinner(20.0))
                            .child(ui::title(text(move || s.label("Waiting for you to grant access in the browser")))),
                    )
                    .child(ui::dim(ui::body(text(move || {
                        s.fmt(
                            "Sign in to {} in the browser window that opened and allow Safay. Safay continues automatically.",
                            &[provider.name()],
                        )
                    }))))
            },
        ))
}

fn insecure(s: Store) -> impl IntoElement {
    let cancel = move || s.cloud_form.update(|f| f.needs_consent = false);
    ui::dialog(
        move || s.label("Unencrypted connection"),
        ui::dim(ui::body(text(move || s.label("Your password is sent over HTTP in plain text. Only continue on a network you trust.")))),
        view().flex_row().gap(px(4.0)).child(ui::text_button(move || s.label("Cancel"), false).on_press(cancel)).child(
            ui::text_button(move || s.label("Use Unencrypted Connection"), true).on_press(move || {
                s.cloud_form.update(|f| {
                    f.insecure = true;
                    f.needs_consent = false;
                });
                if let Some(provider) = connecting(s) {
                    s.connect_cloud(provider)
                }
            }),
        ),
        cancel,
    )
}

fn certificate(s: Store) -> impl IntoElement {
    let fingerprint = move || s.cloud_form.with(|f| f.untrusted);
    let formatted = move || {
        fingerprint()
            .map(|f| f.chunks(8).map(|c| c.iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(":")).collect::<Vec<_>>().join("\n"))
            .unwrap_or_default()
    };
    ui::dialog(
        move || s.label("The server’s certificate can’t be verified"),
        view()
            .gap(px(8.0))
            .child(ui::dim(ui::body(text(move || {
                s.label("Continue only if this fingerprint matches the one your server shows. Safay will trust only this certificate.")
            }))))
            .child(ui::label(text(move || s.label("SHA-256 fingerprint"))))
            .child(text(formatted).font_family(crate::components::CODE_FONT.family).text_size(px(12.0)).line_height(px(17.0))),
        view()
            .flex_row()
            .gap(px(4.0))
            .child(ui::text_button(move || s.label("Cancel"), false).on_press(move || s.decline_certificate()))
            .child(ui::text_button(move || s.label("Trust This Certificate"), true).on_press(move || {
                if let (Some(provider), Some(f)) = (connecting(s), fingerprint()) {
                    s.trust_certificate(provider, f)
                }
            })),
        move || s.decline_certificate(),
    )
}

// ---- Resolving a conflict ----------------------------------------------------------------------------------------------

fn change_text(s: Store, changes: &[AccountChange]) -> String {
    changes
        .iter()
        .map(|c| {
            s.label(match c {
                AccountChange::Issuer => "Service Name",
                AccountChange::Username => "Account / Email",
                AccountChange::Secret => "Secret",
                AccountChange::Parameters => "Algorithm",
                AccountChange::Categories => "Categories",
                AccountChange::Website => "Website",
            })
            .to_string()
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// "Resolve Conflict": what differs on each side, then which copy to keep.
pub fn conflict(s: Store, nav: Nav) -> impl IntoElement {
    let review = move || s.review.get();
    let choice = move || s.form.with(|f| f.algorithm as u8);
    // Resolved (or the conflict went): back to settings.
    effect(move || {
        if status(s) != Status::Conflict && !s.syncing.get() {
            untracked(|| {
                nav.back();
            });
        }
    });
    let side = move |title: Box<dyn Fn() -> SharedString>, count: Box<dyn Fn() -> usize>, detail: Box<dyn Fn() -> String>| {
        view()
            .flex_row()
            .items_center()
            .gap(px(16.0))
            .px(px(16.0))
            .py(px(12.0))
            .min_h(px(64.0))
            .bg(move || theme().card)
            .child(
                view()
                    .flex_1()
                    .min_w(px(0.0))
                    .child(ui::body_large(text(title)))
                    .child(ui::dim(ui::caption(text(detail))).number_of_lines(1)),
            )
            .child(ui::dim(ui::body(text(move || s.fmt("Accounts: {}", &[&count().to_string()])))))
    };
    let list = move |title: &'static str, rows: Box<dyn Fn() -> Vec<(String, String)>>| {
        let rows = std::rc::Rc::new(rows);
        let count = {
            let rows = rows.clone();
            move || rows().len()
        };
        let count2 = count.clone();
        when(
            move || count() > 0,
            move || {
                let rows = rows.clone();
                let count = count2.clone();
                view().child(ui::section_title(move || s.fmt(title, &[&count().to_string()]))).child(
                    ui::body(text(move || {
                        rows().into_iter().map(|(i, u)| if u.is_empty() { i } else { format!("{i} — {u}") }).collect::<Vec<_>>().join("\n")
                    }))
                    .px(px(16.0)),
                )
            },
        )
    };
    let changed = move || s.review.with(|r| r.as_ref().map(|r| r.changed.clone()).unwrap_or_default());
    view()
        .flex_1()
        .pt_safe_area()
        .child(ui::top_bar(s, Some("Resolve Conflict"), move || {
            nav.back();
        }))
        .child(
            when(
                move || review().is_none(),
                move || view().flex_1().items_center().justify_center().child(crate::components::spinner(28.0)),
            )
            .otherwise(move || {
                scroll().flex_1().child(
                    view()
                        .px(px(ui::MARGIN))
                        .pb(px(24.0))
                        .child(ui::group(vec![
                            side(
                                Box::new(move || s.label("This device")),
                                Box::new(move || review().map(|r| r.local_count).unwrap_or(0)),
                                Box::new(crate::platform::device_name),
                            )
                            .into_element(),
                            side(
                                Box::new(move || s.label("Backup")),
                                Box::new(move || review().map(|r| r.remote_count).unwrap_or(0)),
                                Box::new(move || review().and_then(|r| r.remote_device).unwrap_or_default()),
                            )
                            .into_element(),
                        ]))
                        .child(list("Only on this device ({})", Box::new(move || review().map(|r| r.only_local).unwrap_or_default())))
                        .child(list("Only in the backup ({})", Box::new(move || review().map(|r| r.only_remote).unwrap_or_default())))
                        .child(when(
                            move || !changed().is_empty(),
                            move || {
                                view()
                                    .child(ui::section_title(move || {
                                        s.fmt("Different on both sides ({})", &[&changed().len().to_string()])
                                    }))
                                    .child(when(
                                        move || choice() == 0,
                                        move || {
                                            ui::dim(ui::caption(text(move || {
                                                s.label("Pick which version Merge Both keeps for each account.")
                                            })))
                                            .px(px(16.0))
                                            .pb(px(8.0))
                                        },
                                    ))
                                    .child(view().gap(px(12.0)).child(each(
                                        changed,
                                        |r| r.0,
                                        move |row, _| {
                                            let id = move || row.with(|r| r.0);
                                            let remote = move || s.prefer_remote.with(|p| p.contains(&id()));
                                            view()
                                                .gap(px(6.0))
                                                .px(px(16.0))
                                                .child(ui::body_large(text(move || {
                                                    row.with(|r| {
                                                        SharedString::from(if r.2.is_empty() {
                                                            r.1.clone()
                                                        } else {
                                                            format!("{} — {}", r.1, r.2)
                                                        })
                                                    })
                                                })))
                                                .child(ui::dim(ui::caption(text(move || {
                                                    row.with(|r| s.fmt("Differs in: {}", &[&change_text(s, &r.3)]))
                                                }))))
                                                .child(when(
                                                    move || choice() == 0,
                                                    move || {
                                                        ui::choice(
                                                            vec![
                                                                (
                                                                    false,
                                                                    Box::new(move || s.label("This device"))
                                                                        as Box<dyn Fn() -> SharedString>,
                                                                ),
                                                                (true, Box::new(move || s.label("Backup"))),
                                                            ],
                                                            remote,
                                                            move |v| {
                                                                s.prefer_remote.update(|p| {
                                                                    if v {
                                                                        p.insert(id());
                                                                    } else {
                                                                        p.remove(&id());
                                                                    }
                                                                })
                                                            },
                                                        )
                                                    },
                                                ))
                                        },
                                    )))
                            },
                        ))
                        .child(list(
                            "Categories ({})",
                            Box::new(move || {
                                let r = review().unwrap_or_default();
                                let mut lines: Vec<(String, String)> = r
                                    .categories_only_remote
                                    .iter()
                                    .map(|c| (s.fmt("Category only in the backup: {}", &[c]).to_string(), String::new()))
                                    .collect();
                                lines.extend(
                                    r.categories_only_local
                                        .iter()
                                        .map(|c| (s.fmt("Category only on this device: {}", &[c]).to_string(), String::new())),
                                );
                                lines.extend(r.categories_renamed.iter().map(|(a, b)| (format!("{a} → {b}"), String::new())));
                                lines
                            }),
                        ))
                        .child(ui::section_title(move || s.label("Resolve Conflict")))
                        .child(ui::group(vec![
                            choice_row(
                                s,
                                0,
                                "Merge Both",
                                "Keep accounts from both sides. For accounts changed on both, use the version you picked above.",
                            )
                            .into_element(),
                            choice_row(s, 1, "Keep This Device’s Copy", "Overwrite the backup with this copy.").into_element(),
                            choice_row(s, 2, "Use the Backup’s Copy", "Replace this copy with the backup.").into_element(),
                        ]))
                        .child(
                            view().pt(px(16.0)).child(
                                ui::primary_button(move || s.label("Apply"), move || review().is_some(), move || s.syncing.get())
                                    .test_id("apply")
                                    .on_press(move || s.resolve_conflict(choice())),
                            ),
                        ),
                )
            }),
        )
}
fn choice_row(s: Store, value: u8, title: &'static str, detail: &'static str) -> impl IntoElement {
    let on = move || s.form.with(|f| f.algorithm as u8) == value;
    button()
        .test_id(title)
        .flex_row()
        .items_center()
        .gap(px(16.0))
        .px(px(16.0))
        .py(px(12.0))
        .bg(move || theme().card)
        .active(|st| st.bg(move || tok("surface-high")))
        .on_press(move || s.form.update(|f| f.algorithm = u32::from(value)))
        .child(
            view()
                .size(px(20.0))
                .rounded_full()
                .items_center()
                .justify_center()
                .border(px(2.0))
                .border_color(move || if on() { theme().primary } else { theme().muted_foreground })
                .child(when(on, || view().size(px(10.0)).rounded_full().bg(move || theme().primary))),
        )
        .child(
            view().flex_1().child(ui::body_large(text(move || s.label(title)))).child(ui::dim(ui::caption(text(move || s.label(detail))))),
        )
}

// ---- First run: restoring a backup -------------------------------------------------------------------------------------

/// "Restore a backup": where it is (a cloud service or an exported file), then its password.
pub fn restore_page(s: Store, scaffold: impl FnOnce(AnyElement, AnyElement) -> AnyElement) -> AnyElement {
    let use_as_master = s.use_export_password;
    use_as_master.set(true);
    let location = move || s.restore_location.with(|f| f.is_some());
    let export = move || !location() && s.has_file();
    let found = move || location() || export();
    let ready = move || {
        if !found() || s.form.with(|f| f.password.trim().is_empty()) {
            return false;
        }
        !export() || use_as_master.get() || crate::pages::unlock::new_password_ok(s)
    };
    let submit = move || {
        if ready() && !s.busy.get() {
            dismiss_keyboard();
            if location() { s.restore_from_location() } else { s.restore_export(false) }
        }
    };
    let change = move || {
        s.form.set(crate::store::Form::fresh());
        use_as_master.set(true);
        s.error.set(None);
        s.status.set(None);
        s.file.replace(None);
        s.file_name.set(String::new());
        s.discard_restore_location();
    };
    let back = move || {
        if s.busy.get() {
            return;
        }
        if found() { change() } else { super::onboarding::leave(s) }
    };
    let problem = move || {
        s.error
            .get()
            .map(|e| s.label(crate::i18n::error_key(e)))
            .or_else(|| s.status.get().filter(|st| st.tone == crate::store::Tone::Error).map(|st| st.text))
    };
    scaffold(
        ui::top_bar(s, None, back).into_element(),
        view()
            .w_full()
            .gap(px(16.0))
            .child(
                view()
                    .items_center()
                    .gap(px(12.0))
                    .child(ui::icon_tile(Icon::CloudDown, 88.0, 30.0, "secondary", "foreground"))
                    .child(ui::headline(text(move || s.label("Restore a backup"))).text_center())
                    .child(
                        ui::dim(ui::body_large(text(move || {
                            s.label("Choose where your backup is, then unlock it with your master password.")
                        })))
                        .text_center(),
                    ),
            )
            .child(when(
                move || problem().is_some(),
                move || {
                    view()
                        .flex_row()
                        .gap(px(12.0))
                        .p(px(16.0))
                        .rounded(px(24.0))
                        .bg(move || tok("destructive-container"))
                        .text_color(move || tok("destructive-container-foreground"))
                        .child(icon(Icon::Warning).size(px(22.0)))
                        .child(ui::body_large(text(move || problem().unwrap_or_default())).flex_1())
                },
            ))
            .child(
                when(found, move || {
                    view()
                        .gap(px(12.0))
                        .child(
                            ui::form_card()
                                .flex_row()
                                .items_center()
                                .gap(px(16.0))
                                .child(view().text_color(move || tok("success")).child(icon(Icon::SealFill).size(px(28.0))))
                                .child(
                                    view()
                                        .flex_1()
                                        .min_w(px(0.0))
                                        .child(ui::body_large(text(move || {
                                            s.label(if export() { "From an exported file" } else { "Backup found" })
                                        })))
                                        .child(
                                            ui::dim(ui::body(text(move || {
                                                if export() {
                                                    return SharedString::from(s.file_name.get());
                                                }
                                                match s.restore_location.get() {
                                                    Some(other) => s.fmt("From {}", &[&other.title()]),
                                                    None => SharedString::default(),
                                                }
                                            })))
                                            .number_of_lines(2),
                                        ),
                                ),
                        )
                        .child(ui::form_card().child(ui::field(
                            s,
                            "password",
                            Entry::Password,
                            move || s.label(if export() { "Export Password" } else { "Existing Master Password" }),
                            move || s.form.with(|f| f.password.to_string()),
                            move |v| {
                                s.error.set(None);
                                s.status.set(None);
                                s.form.update(|f| f.password = Zeroizing::new(v.to_owned()))
                            },
                            submit,
                            |_| {},
                            move || s.error.get().is_some(),
                            ReturnKeyType::Done,
                            None,
                        )))
                        .child(when(export, move || {
                            view()
                                .gap(px(12.0))
                                .child(
                                    ui::form_card().child(
                                        button()
                                            .flex_row()
                                            .items_center()
                                            .gap(px(16.0))
                                            .px(px(4.0))
                                            .on_press(move || use_as_master.update(|v| *v = !*v))
                                            .child(
                                                view().text_color(move || theme().muted_foreground).child(icon(Icon::Key).size(px(28.0))),
                                            )
                                            .child(ui::body_large(text(move || s.label("Use as Master Password"))).flex_1())
                                            .child(ui::switch(move || use_as_master.get())),
                                    ),
                                )
                                .child(when(
                                    move || !use_as_master.get(),
                                    move || {
                                        ui::form_card().child(super::onboarding::new_password_fields(
                                            s,
                                            "Master Password",
                                            "Enter again",
                                            submit,
                                        ))
                                    },
                                ))
                        }))
                        .child(
                            ui::primary_button(move || s.label("Unlock & Continue"), ready, move || s.busy.get())
                                .test_id("restore-go")
                                .on_press(submit),
                        )
                })
                .otherwise(move || {
                    when(
                        move || s.busy.get(),
                        move || {
                            view()
                                .items_center()
                                .gap(px(12.0))
                                .py(px(24.0))
                                .child(crate::components::spinner(28.0))
                                .child(ui::dim(ui::body(text(move || s.label("Looking for a Safay backup")))))
                        },
                    )
                    .otherwise(move || sources(s))
                }),
            )
            .into_element(),
    )
}

/// Where a backup can be: each cloud service of this build, and an exported file.
fn sources(s: Store) -> impl IntoElement {
    let mut rows: Vec<AnyElement> = providers()
        .into_iter()
        .map(|provider| {
            source_row(LocationIcon::Cloud(provider), provider.name().into())
                .test_id(provider.name())
                .on_press(move || {
                    s.error.set(None);
                    s.status.set(None);
                    open_setup(s, provider, true)
                })
                .into_element()
        })
        .collect();
    rows.push(
        source_row(LocationIcon::ExportFile, s.label("Safay Exported File"))
            .test_id("export-file")
            .on_press(move || {
                s.error.set(None);
                s.status.set(None);
                s.choose_backup()
            })
            .into_element(),
    );
    ui::group(rows)
}
fn source_row(kind: LocationIcon, title: SharedString) -> nsh_base::ButtonElement {
    button()
        .min_h(px(72.0))
        .px(px(16.0))
        .flex_row()
        .items_center()
        .gap(px(16.0))
        .bg(move || theme().card)
        .active(|st| st.bg(move || tok("surface-high")))
        .child(plate(kind))
        .child(ui::body_large(text(title)).flex_1())
        .child(view().text_color(move || theme().muted_foreground).child(icon(Icon::ChevronRight).size(px(24.0))))
}

/// Whether first run is on its restore step.
pub fn restoring(s: Store) -> bool {
    s.stage.get() == Stage::Restore
}
