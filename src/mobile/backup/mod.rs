//! Backup and sync on a phone: the card in settings, the history, choosing a location, connecting a cloud service (a
//! sheet from the bottom), resolving a conflict and, on first run, restoring a backup. The store does the work; these
//! are its pages.
use super::{
    Nav, Page, onboarding,
    ui::{self, Entry},
};
use crate::{
    components::tok,
    icons::{Icon, icon},
    pages::backup::{
        LocationIcon, app_folder, change_text, formatted_fingerprint, location_mark, oauth_configured, size, source_key, stamp,
    },
    pages::unlock,
    store::{BackupPage, Sheet, Stage, Store},
};
use nsh_base::{button, theme};
use nsh_core::prelude::*;
use safay_app::{
    Zeroizing,
    cloud::Provider,
    sync::{Direction, HistoryEntry, Location, Source, Status, SyncError},
};

/// The cloud services a phone offers, in order: those this build has a client for, then the servers.
fn providers() -> Vec<Provider> {
    Provider::LISTED.into_iter().filter(|p| oauth_configured(*p)).collect()
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

mod cloud;
mod conflict;
mod location;
mod restore;

pub use cloud::setup_sheet;
pub use conflict::conflict;
pub use location::dialogs;
pub use restore::{restore_back, restore_page, restoring};

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
            let folder = app_folder(s, provider);
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
                    .child(ui::caption(text(move || SharedString::from(stamp(e().at_ms)))))
                    .child(ui::dim(ui::caption(text(move || SharedString::from(size(e().bytes)))))),
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
