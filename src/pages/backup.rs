//! Backup & Sync (640): the status card, history, location choice, conflict review and unreadable-backup restore.
use super::unlock::field_label;
use crate::{
    chrome::{Look, look},
    components::*,
    icons::{Icon, icon},
    store::{BackupPage, Store},
};
use nsh_base::{button, theme};
use nsh_core::prelude::*;
use safay_app::{
    Zeroizing,
    cloud::Provider,
    core::merge::AccountChange,
    sync::{Direction, HistoryEntry, Source, Status},
};

fn status(s: Store) -> Status {
    s.projection.with(|p| p.sync.status)
}
fn configured(s: Store) -> bool {
    s.projection.with(|p| p.sync.location.is_some())
}
/// The sidebar's 7pt status dot color: none when off, orange for a conflict, red for failures, green otherwise.
pub fn dot_color(s: Store) -> Option<AdaptiveColor> {
    match status(s) {
        Status::Off => None,
        Status::Conflict => Some(tok("warning")),
        Status::Unreadable | Status::Failed(_) => Some(theme().destructive),
        _ => Some(tok("success")),
    }
}
fn this_device(s: Store) -> SharedString {
    s.label(if look() == Look::Mac { "This Mac" } else { "This device" })
}
/// Local wall time for history rows, without a time-zone database: "Nov 5 10:42" style via UTC offset-free fields.
fn stamp(s: Store, ms: i64) -> String {
    let t = std::time::UNIX_EPOCH + std::time::Duration::from_millis(ms.max(0) as u64);
    let name = crate::platform::files::export_file_name(t);
    let d = &name["Safay-export-".len()..name.len() - ".json".len()];
    let _ = s;
    format!("{}-{}-{} {}:{}", &d[0..4], &d[4..6], &d[6..8], &d[9..11], &d[11..13])
}
fn size(bytes: Option<u64>) -> String {
    match bytes {
        Some(b) if b >= 1024 => format!("{:.1} KB", b as f64 / 1024.0),
        Some(b) => format!("{b} B"),
        None => String::new(),
    }
}
fn source_key(source: Source) -> &'static str {
    match source {
        Source::Auto => "Auto Sync",
        Source::Manual => "Manual Backup",
        Source::Merge => "Merge",
        Source::Restore => "Restore from Backup",
    }
}

/// The content's minimum height.
const MIN_BODY: f32 = 300.0;
/// A history row's height; the list shows three and scrolls the rest.
const HISTORY_ROW: f32 = 44.0;
const HISTORY_SHOWN: usize = 3;

pub fn backup_sheet(s: Store) -> impl IntoElement {
    let page = move || s.backup_page.get();
    sheet_frame(
        s,
        640.0,
        move || {
            s.label(match page() {
                BackupPage::Conflict => "Resolve Conflict",
                BackupPage::Restore => "Restore from Backup",
                BackupPage::Setup(_) => {
                    return s.fmt(
                        "Connect to {}",
                        &[match page() {
                            BackupPage::Setup(p) => p.name(),
                            _ => "",
                        }],
                    );
                }
                _ => "Backup & Sync",
            })
        },
        view(),
        // The overview stays still: only its history scrolls. The other pages scroll as a whole
        // when the window is too short for them.
        when(move || page() == BackupPage::Overview, move || sheet_body(MIN_BODY, overview(s)).flex_shrink(1.0)).otherwise(move || {
            overflow_scroll().child(sheet_body(
                MIN_BODY,
                view()
                    .child(when(move || page() == BackupPage::Choose, move || choose(s)))
                    .child(when(move || page() == BackupPage::Conflict, move || conflict(s)))
                    .child(when(move || page() == BackupPage::Restore, move || restore(s)))
                    .child(when(
                        move || matches!(page(), BackupPage::Setup(_)),
                        move || match page() {
                            BackupPage::Setup(provider) => setup(s, provider).into_element(),
                            _ => view().into_element(),
                        },
                    )),
            ))
        }),
        footer(s),
        move || s.close_sheet(),
    )
}

fn footer(s: Store) -> impl IntoElement {
    let page = move || s.backup_page.get();
    view()
        .flex_1()
        .flex_row()
        .items_center()
        .gap(px(10.0))
        .child(when(
            move || page() == BackupPage::Overview && configured(s),
            move || {
                view()
                    .flex_row()
                    .gap(px(10.0))
                    .child(
                        push_button(move || s.label("Change"), Kind::Standard, false)
                            .disabled(move || s.syncing.get())
                            .on_press(move || s.backup_page.set(BackupPage::Choose)),
                    )
                    .child(
                        push_button(move || s.label("Turn Off Backup"), Kind::Standard, false)
                            .disabled(move || s.syncing.get())
                            .on_press(move || s.turn_off_sync()),
                    )
            },
        ))
        .child(when(
            move || page() != BackupPage::Overview,
            move || {
                push_button(move || s.label(if page() == BackupPage::Conflict { "Decide Later" } else { "Back" }), Kind::Standard, false)
                    // A server connection can be left until the backup check (what it gets back is dropped); an OAuth
                    // sign-in cannot after the browser returns (token exchange, backup check).
                    .disabled(move || {
                        let server = matches!(page(), BackupPage::Setup(Provider::Webdav | Provider::Nextcloud));
                        s.cloud_form.with(|f| if server { f.checking } else { f.working && !f.waiting })
                    })
                    .on_press(move || {
                        s.status.set(None);
                        if s.cloud_restore.get() {
                            // Opened from the first-run restore page: back closes the sheet.
                            s.close_sheet();
                        } else if matches!(page(), BackupPage::Setup(_)) {
                            s.leave_cloud_setup();
                        } else {
                            s.backup_page.set(BackupPage::Overview)
                        }
                    })
            },
        ))
        .child(view().flex_1().min_w(px(0.0)).child(status_line(move || s.status.get())))
        .child(when(
            move || page() == BackupPage::Overview,
            move || push_button(move || s.label("Done"), Kind::Primary, false).on_press(move || s.close_sheet()),
        ))
        .child(when(
            move || page() == BackupPage::Conflict,
            move || {
                push_button(move || s.label("Apply"), Kind::Primary, false)
                    .disabled(move || s.syncing.get() || s.review.with(|r| r.is_none()))
                    .on_press(move || s.resolve_conflict(s.form.with(|f| f.algorithm as u8)))
            },
        ))
        .child(when(
            move || matches!(page(), BackupPage::Setup(_)) && !s.cloud_form.with(|f| f.waiting),
            move || {
                let oauth = move || matches!(page(), BackupPage::Setup(Provider::Dropbox | Provider::Onedrive | Provider::GoogleDrive));
                push_button(move || s.label(if oauth() { "Sign In" } else { "Connect" }), Kind::Primary, false)
                    .disabled(move || {
                        let ready = oauth()
                            || s.cloud_form.with(|f| {
                                !f.address.trim().is_empty()
                                    && (page() != BackupPage::Setup(Provider::Webdav)
                                        || (!f.username.trim().is_empty() && !f.password.is_empty()))
                            });
                        !ready || s.cloud_form.with(|f| f.working) || s.syncing.get()
                    })
                    .on_press(move || {
                        if let BackupPage::Setup(provider) = page() {
                            if oauth() { s.connect_oauth(provider) } else { s.connect_cloud(provider) }
                        }
                    })
            },
        ))
        .child(when(
            move || page() == BackupPage::Restore,
            move || {
                push_button(move || s.label("Restore"), Kind::Primary, false)
                    .disabled(move || s.syncing.get() || s.form.with(|f| f.password.is_empty()))
                    .on_press(move || s.restore_unreadable())
            },
        ))
}

/// The linked service's mark, when the location is iCloud or cloud storage (folders keep the accent folder tile).
fn service_mark(s: Store) -> Option<LocationIcon> {
    s.projection.with(|p| match &p.sync.location {
        Some(safay_app::sync::Location::ICloud { .. }) => Some(LocationIcon::Icloud),
        Some(safay_app::sync::Location::Cloud { provider, .. }) => Some(LocationIcon::Cloud(*provider)),
        _ => None,
    })
}
/// The status card's second line: "Private to your Apple Account" for iCloud, else the location detail.
fn location_detail(s: Store, location: &safay_app::sync::Location) -> SharedString {
    match location {
        safay_app::sync::Location::ICloud { .. } => s.label("Private to your Apple Account"),
        other => other.detail().into(),
    }
}
fn location_icon(s: Store, size: f32) -> impl IntoElement {
    // A mark is drawn for the service it was built for, and the overview stays mounted while the location changes
    // (linking another service comes back to it): each service has its own, shown while it is the linked one.
    let marks = [
        LocationIcon::Icloud,
        LocationIcon::Cloud(Provider::Dropbox),
        LocationIcon::Cloud(Provider::Onedrive),
        LocationIcon::Cloud(Provider::GoogleDrive),
        LocationIcon::Cloud(Provider::Nextcloud),
        LocationIcon::Cloud(Provider::Webdav),
    ];
    view()
        .size(px(size))
        .children(marks.map(|kind| when(move || service_mark(s) == Some(kind), move || location_mark(kind, size)).into_element()))
        .child(when(move || service_mark(s).is_none(), move || location_tile(s, size)))
        .child(when(
            move || dot_color(s).is_some(),
            move || {
                view()
                    .absolute()
                    .right(px(-3.0))
                    .bottom(px(-3.0))
                    .size(px(12.0))
                    .rounded_full()
                    .border(px(2.0))
                    .border_color(move || tok("sheet"))
                    .bg(move || dot_color(s).unwrap_or(theme().muted_foreground))
            },
        ))
}
fn location_tile(s: Store, size: f32) -> impl IntoElement {
    view()
        .size(px(size))
        .rounded(px(10.0))
        .items_center()
        .justify_center()
        // Unconfigured: a gray "iCloud off" tile.
        .bg(move || if configured(s) { tok("code-accent").opacity(0.12) } else { theme().muted_foreground.opacity(0.13) })
        .child(
            when(move || configured(s), move || crate::icons::symbol(Icon::Folder, size * 0.5, move || tok("code-accent")))
                .otherwise(move || crate::icons::symbol(Icon::CloudOff, size * 0.5, move || theme().muted_foreground)),
        )
}

fn overview(s: Store) -> impl IntoElement {
    let st = move || status(s);
    view()
        .gap(px(16.0))
        .child(
            view()
                .flex_row()
                .items_center()
                .gap(px(14.0))
                .p(px(14.0))
                .rounded(px(10.0))
                // `.quaternary.opacity(0.35)`.
                .bg(move || theme().foreground.opacity(0.035))
                .child(location_icon(s, 40.0))
                .child(
                    view()
                        .flex_1()
                        .min_w(px(0.0))
                        .gap(px(3.0))
                        .child(
                            heading(
                                move || {
                                    s.projection
                                        .with(|p| p.sync.location.as_ref().map(|l| SharedString::from(l.title())))
                                        .unwrap_or_else(|| s.label("Backup is off"))
                                },
                                13.5,
                            )
                            .number_of_lines(1),
                        )
                        .child(
                            text(move || {
                                s.projection
                                    .with(|p| p.sync.location.as_ref().map(|l| location_detail(s, l)))
                                    .unwrap_or_else(|| s.label("Keep an end-to-end encrypted backup in a folder or a cloud service"))
                            })
                            .number_of_lines(2)
                            .text_size(px(12.0))
                            .text_color(move || theme().muted_foreground),
                        ),
                )
                .child(
                    when(
                        move || configured(s),
                        move || {
                            view()
                                .items_end()
                                .gap(px(6.0))
                                .child(when(
                                    move || !matches!(st(), Status::Conflict | Status::Unreadable | Status::Failed(safay_app::sync::SyncError::ManualWriteRequired)),
                                    move || {
                                        push_button(
                                            move || s.label(if s.syncing.get() { "Syncing" } else { "Back Up Now" }),
                                            Kind::Standard,
                                            false,
                                        )
                                        .disabled(move || s.syncing.get() || !s.unlocked())
                                        .on_press(move || s.sync_now(Source::Manual))
                                    },
                                ))
                                .child(
                                    text(move || match st() {
                                        Status::Conflict => s.label("Sync Conflict Detected"),
                                        Status::Unreadable => s.label("Backup Can’t Be Opened"),
                                        Status::Failed(e) => s.label(crate::store::sync_error_key(e)),
                                        _ => s
                                            .projection
                                            .with(|p| p.sync.last_synced_ms)
                                            .map(|ms| s.fmt("Last synced {}", &[&stamp(s, ms)]))
                                            .unwrap_or_default(),
                                    })
                                    .max_w(px(220.0))
                                    .number_of_lines(2)
                                    .text_right()
                                    .text_size(px(11.0))
                                    .text_color(move || match st() {
                                        Status::Conflict => tok("warning"),
                                        Status::Unreadable | Status::Failed(_) => theme().destructive,
                                        _ => tok("success"),
                                    }),
                                )
                        },
                    )
                    .otherwise(move || {
                        push_button(move || s.label("Set Up"), Kind::Primary, false).on_press(move || s.backup_page.set(BackupPage::Choose))
                    }),
                ),
        )
        // The note under the status card: a pinned certificate stays "not verified".
        .child(when(
            move || s.projection.with(|p| p.sync.location.as_ref().is_some_and(|l| l.unverified_certificate())),
            move || {
                view()
                    .flex_row()
                    .items_center()
                    .gap(px(5.0))
                    .pl(px(4.0))
                    .child(crate::icons::symbol(Icon::ShieldWarningFill, 11.5, move || tok("warning")))
                    .child(
                        text(move || s.label("Certificate not verified"))
                            .text_size(px(11.5))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(move || tok("warning")),
                    )
            },
        ))
        .child(
            view()
                .flex_row()
                .gap(px(6.0))
                .text_color(move || theme().muted_foreground)
                .items_center()
                .child(crate::icons::symbol_weight(Icon::Lock, 10.0, 0.0, move || theme().muted_foreground))
                .child(
                    note(move || s.label("Backups are end-to-end encrypted with your master password; only you can open them.")).flex_1(),
                ),
        )
        .child(when(
            move || st() == Status::Failed(safay_app::sync::SyncError::ManualWriteRequired),
            move || callout(
                s,
                move || tok("warning"),
                "Replace cloud backup?",
                "This provider cannot protect this write against changes from another device. Replacing the backup uploads this device’s vault.",
                "Replace Backup",
                move || s.resolve_conflict(1),
            ),
        ))
        .child(when(
            move || st() == Status::Conflict,
            move || {
                callout(
                    s,
                    move || tok("warning"),
                    "Sync Conflict Detected",
                    if look() == Look::Mac {
                        "This Mac and the backup both changed. Compare the accounts on each side, then keep one copy or merge both."
                    } else {
                        "This device and the backup both changed. Compare the accounts on each side, then keep one copy or merge both."
                    },
                    "Resolve Conflict",
                    move || {
                        s.status.set(None);
                        s.form.update(|f| f.algorithm = 0);
                        s.backup_page.set(BackupPage::Conflict);
                        s.load_review();
                    },
                )
            },
        ))
        .child(when(
            move || st() == Status::Unreadable,
            move || {
                callout(
                    s,
                    move || theme().destructive,
                    "Backup Can’t Be Opened",
                    if look() == Look::Mac {
                        "To use that backup, restore from it and unlock with its master password. This replaces the vault on this Mac."
                    } else {
                        "To use that backup, restore from it and unlock with its master password. This replaces the vault on this device."
                    },
                    "Restore from Backup",
                    move || {
                        s.status.set(None);
                        s.form.update(|f| f.password = Zeroizing::new(String::new()));
                        s.backup_page.set(BackupPage::Restore)
                    },
                )
            },
        ))
        // The label and its list are 8pt apart.
        .child(
            view().gap(px(8.0)).child(section_label(move || s.label("Backup History"))).child(
                when(
                    move || s.projection.with(|p| p.sync.history.is_empty()),
                    move || {
                        view()
                            .min_h(px(64.0))
                            .items_center()
                            .justify_center()
                            .rounded(px(9.0))
                            // `.quaternary.opacity(0.25)`.
                            .bg(move || theme().foreground.opacity(0.025))
                            .child(
                                text(move || s.label("No backup history yet"))
                                    .text_size(px(12.0))
                                    .text_color(move || theme().muted_foreground),
                            )
                    },
                )
                .otherwise(move || {
                    // Every record, three rows high (with the hairlines between them); the rest scroll.
                    let height = move || {
                        let n = s.projection.with(|p| p.sync.history.len()).min(HISTORY_SHOWN) as f32;
                        px(n * HISTORY_ROW + (n - 1.0).max(0.0))
                    };
                    scroll().h(height).rounded(px(10.0)).bg(move || tok("hover").opacity(0.4)).child(each(
                        move || s.projection.with(|p| p.sync.history.iter().cloned().enumerate().collect::<Vec<_>>()),
                        |(i, h)| format!("{i}:{}", h.at_ms),
                        move |item, _| history_row(s, item),
                    ))
                }),
            ),
        )
}
fn callout(
    s: Store,
    tint: impl Fn() -> AdaptiveColor + Copy + 'static,
    title: &'static str,
    detail: &'static str,
    action: &'static str,
    press: impl Fn() + 'static,
) -> impl IntoElement {
    view()
        .flex_row()
        .items_center()
        .gap(px(12.0))
        .p(px(14.0))
        .rounded(px(10.0))
        .bg(move || tint().opacity(0.08))
        .border_1()
        .border_color(move || tint().opacity(0.32))
        .child(view().text_color(tint).child(icon(Icon::Warning).size(px(18.0))))
        .child(
            view().flex_1().gap(px(3.0)).child(heading(move || s.label(title), 13.0)).child(
                text(move || s.label(detail)).text_size(px(12.0)).line_height(px(17.0)).text_color(move || theme().muted_foreground),
            ),
        )
        .child(push_button(move || s.label(action), Kind::Primary, false).disabled(move || s.syncing.get()).on_press(press))
}
fn history_row(s: Store, item: ReadSignal<(usize, HistoryEntry)>) -> impl IntoElement {
    let h = move || item.with(|x| x.1.clone());
    let title = move || {
        let e = h();
        if e.failed {
            return s.fmt("{} failed", &[&s.label(source_key(e.source))]);
        }
        s.label(match e.direction {
            Some(Direction::Upload) => "Uploaded",
            Some(Direction::Download) => "Downloaded",
            Some(Direction::Unchanged) => "No changes",
            None => source_key(e.source),
        })
    };
    let detail = move || {
        let e = h();
        let mut parts = vec![e.location.clone()];
        if !e.failed {
            parts.push(s.label(source_key(e.source)).to_string());
        }
        if e.direction != Some(Direction::Upload) && !e.failed && e.device != crate::platform::device_name() && e.device != e.location {
            parts.push(s.fmt("From {}", &[&e.device]).to_string());
        }
        parts.retain(|p| !p.is_empty());
        parts.join(" · ")
    };
    view().child(when(move || item.with(|x| x.0) > 0, || view().ml(px(46.0)).child(divider()))).child(
        view()
            .flex_row()
            .items_center()
            .gap(px(10.0))
            .h(px(HISTORY_ROW))
            .px(px(12.0))
            .child(
                view()
                    .size(px(24.0))
                    .rounded_full()
                    .items_center()
                    .justify_center()
                    .bg(move || if h().failed { theme().destructive.opacity(0.12) } else { tok("code-accent").opacity(0.12) })
                    .text_color(move || if h().failed { theme().destructive } else { tok("code-accent") })
                    .child(when(move || h().failed, || icon(Icon::Warning).size(px(13.0))).otherwise(move || {
                        when(move || h().direction == Some(Direction::Download), || icon(Icon::Download).size(px(13.0)))
                            .otherwise(|| icon(Icon::Cloud).size(px(13.0)))
                    })),
            )
            .child(
                view()
                    .flex_1()
                    .min_w(px(0.0))
                    .gap(px(1.0))
                    .child(text(title).number_of_lines(1).text_size(px(12.5)).font_weight(FontWeight::MEDIUM))
                    .child(text(detail).number_of_lines(1).text_size(px(11.0)).text_color(move || theme().muted_foreground)),
            )
            .child(
                view()
                    .items_end()
                    .gap(px(1.0))
                    .child(text(move || stamp(s, h().at_ms)).text_size(px(11.5)).text_color(move || theme().muted_foreground))
                    .child(text(move || size(h().bytes)).text_size(px(11.0)).text_color(move || tok("tertiary"))),
            ),
    )
}

/// What a backup location looks like: a brand mark or a symbol on a gray plate.
#[derive(Clone, Copy, PartialEq)]
pub enum LocationIcon {
    Icloud,
    Folder,
    Cloud(Provider),
    /// A Safay export file (first-run restore).
    ExportFile,
}
pub fn location_mark(kind: LocationIcon, size: f32) -> impl IntoElement + Styled {
    let brand = match kind {
        LocationIcon::Icloud => Some(include_str!("../../assets/brands/icloud.svg")),
        LocationIcon::Cloud(Provider::Dropbox) => Some(include_str!("../../assets/brands/dropbox.svg")),
        LocationIcon::Cloud(Provider::Onedrive) => Some(include_str!("../../assets/brands/onedrive.svg")),
        LocationIcon::Cloud(Provider::GoogleDrive) => Some(include_str!("../../assets/brands/googledrive.svg")),
        LocationIcon::Cloud(Provider::Nextcloud) => Some(include_str!("../../assets/brands/nextcloud.svg")),
        _ => None,
    };
    let plate = view()
        .size(px(size))
        .rounded(px(size * 0.26))
        .items_center()
        .justify_center()
        // `.quaternary.opacity(0.6)`.
        .bg(move || theme().foreground.opacity(0.06));
    match brand {
        Some(markup) => {
            plate.child(svg(nsh_core::element::ImageSrc::svg_markup(markup)).size(px(size * 0.62)).overflow_hidden().into_element())
        }
        None => plate.child(
            crate::icons::symbol(
                match kind {
                    LocationIcon::Folder => Icon::FolderFill,
                    LocationIcon::ExportFile => Icon::DocFill,
                    _ => Icon::ServerRack,
                },
                size * 0.5,
                move || tok("code-accent"),
            )
            .into_element(),
        ),
    }
}
/// A backup location row: 32pt mark, 13pt semibold title, optional 11.5pt subtitle, chevron; 12×10
/// padding on `.quaternary.opacity(0.35)`. Shared by the Backup & Sync sheet and the first-run restore page.
pub fn location_row(
    s: Store,
    kind: LocationIcon,
    title: &'static str,
    subtitle: Option<&'static str>,
    disabled: impl Fn() -> bool + 'static,
    press: impl Fn() + 'static,
) -> nsh_base::ButtonElement {
    button()
        .accessibility_label(move || Some(s.label(title)))
        .disabled(disabled)
        .flex_row()
        .items_center()
        .gap(px(12.0))
        .px(px(12.0))
        .py(px(10.0))
        .rounded(px(10.0))
        .bg(move || theme().foreground.opacity(0.035))
        .hover(|st| st.bg(move || theme().foreground.opacity(0.06)))
        .focus(focus_ring)
        .active(|st| st.scale(0.99))
        .disabled_style(|st| st.opacity(0.5))
        .on_press(press)
        .child(location_mark(kind, 32.0))
        .child(
            view().flex_1().gap(px(2.0)).child(text(move || s.label(title)).text_size(px(13.0)).font_weight(FontWeight::SEMIBOLD)).child(
                match subtitle {
                    Some(t) => text(move || s.label(t))
                        .number_of_lines(2)
                        .text_size(px(11.5))
                        .text_color(move || theme().muted_foreground)
                        .into_element(),
                    None => view().into_element(),
                },
            ),
        )
        .child(crate::icons::symbol_weight(Icon::ChevronRight, 11.0, 0.3, move || tok("tertiary")))
}
/// Which OAuth services this build has client IDs for (SECURITY.md, Network); the others are not listed.
/// Whether a location can be set up here: OAuth services need sign-in support on this platform and a client ID in this
/// build; WebDAV and Nextcloud always can.
pub fn oauth_configured(provider: Provider) -> bool {
    !matches!(provider, Provider::Dropbox | Provider::Onedrive | Provider::GoogleDrive)
        || (crate::platform::supports::OAUTH && oauth_client(provider).is_some())
}
/// The public client of each service and the callback registered for it, both from `cloud-keys.toml` (`build.rs`).
pub fn oauth_client(provider: Provider) -> Option<safay_app::cloudset::OAuthClient> {
    use safay_app::cloudset::OAuthClient;
    // Where the sign-in happens in the browser (Windows, Linux), Google expects its "Desktop app" client: a loopback
    // redirect, whose port is chosen when the sign-in starts (`Store::connect_oauth`), and a client secret.
    let desktop = (
        option_env!("SAFAY_GOOGLE_DESKTOP_CLIENT_ID"),
        option_env!("SAFAY_GOOGLE_DESKTOP_REDIRECT_URI"),
        option_env!("SAFAY_GOOGLE_DESKTOP_CLIENT_SECRET"),
    );
    let (id, redirect, secret) = match (provider, desktop) {
        (Provider::Dropbox, _) => (option_env!("SAFAY_DROPBOX_APP_KEY"), option_env!("SAFAY_DROPBOX_REDIRECT_URI"), None),
        (Provider::Onedrive, _) => (option_env!("SAFAY_MICROSOFT_CLIENT_ID"), option_env!("SAFAY_MICROSOFT_REDIRECT_URI"), None),
        (Provider::GoogleDrive, (Some(id), Some(redirect), Some(secret))) if crate::platform::supports::BROWSER_SIGN_IN => {
            (Some(id), Some(redirect), Some(secret))
        }
        (Provider::GoogleDrive, _) => (option_env!("SAFAY_GOOGLE_CLIENT_ID"), option_env!("SAFAY_GOOGLE_REDIRECT_URI"), None),
        _ => return None,
    };
    let client = OAuthClient::new(id?, redirect?);
    let client = match secret {
        Some(secret) => client.with_client_secret(secret),
        None => client,
    };
    // Google Drive has no app folder of its own: Safay's is a folder with the configured name in My Drive.
    Some(if provider == Provider::GoogleDrive { client.with_folder_name(option_env!("SAFAY_GOOGLE_FOLDER_NAME")?) } else { client })
}
fn choose(s: Store) -> impl IntoElement {
    let row = move |kind: LocationIcon, title: &'static str, subtitle: Option<&'static str>, enabled: bool, press: Box<dyn Fn()>| {
        let row = location_row(s, kind, title, subtitle, move || s.syncing.get() || !enabled, press);
        view().flex_1().min_w(px(0.0)).child(row.flex_1()).into_element()
    };
    // iCloud needs a build signed with the iCloud entitlement; otherwise the row stays disabled with the reason.
    let icloud = safay_app::icloud::available();
    let mut rows = Vec::new();
    if crate::platform::supports::ICLOUD {
        rows.push(row(
            LocationIcon::Icloud,
            "iCloud",
            (!icloud).then_some("Not available in this build of Safay"),
            icloud,
            Box::new(move || s.link_icloud()),
        ));
    }
    if crate::platform::supports::CLOUD {
        for provider in [Provider::Dropbox, Provider::Onedrive, Provider::GoogleDrive, Provider::Nextcloud, Provider::Webdav] {
            if oauth_configured(provider) {
                rows.push(row(LocationIcon::Cloud(provider), provider.name(), None, true, Box::new(move || s.open_cloud_setup(provider))));
            }
        }
    }
    rows.push(row(
        LocationIcon::Folder,
        "Other Folder",
        Some(if crate::platform::supports::ICLOUD {
            "iCloud Drive or a folder synced by another app"
        } else {
            "A folder synced by another app"
        }),
        true,
        Box::new(move || s.choose_folder()),
    ));
    view()
        .gap(px(14.0))
        .child(text(move || s.label("Choose where to keep the encrypted backup. Only one location is used at a time.")).text_size(px(12.0)))
        .child(location_grid(rows))
}
/// Location cells two per line, as the restore page lists them; a last cell on its own keeps the column width. A cell
/// is an unpadded flex item around its row, so a line with an empty slot keeps the same column widths (a padded item's
/// flex basis is at least its padding).
pub fn location_grid(cells: Vec<AnyElement>) -> impl IntoElement {
    let mut lines = Vec::new();
    let mut cells = cells.into_iter();
    while let Some(first) = cells.next() {
        let second = cells.next().unwrap_or_else(|| view().flex_1().into_element());
        lines.push(view().flex_row().gap(px(8.0)).child(first).child(second).into_element());
    }
    view().gap(px(8.0)).children(lines)
}

/// The pairing header: Safay's icon, a locked link, the service's mark, then the headline and a note.
fn pairing_header(s: Store, provider: Provider) -> impl IntoElement {
    let link = move || {
        view()
            .flex_row()
            .items_center()
            .gap(px(4.0))
            .child(view().w(px(12.0)).h(px(2.0)).rounded_full().bg(move || theme().foreground.opacity(0.1)))
            .child(
                view()
                    .size(px(19.0))
                    .rounded_full()
                    .items_center()
                    .justify_center()
                    .bg(move || theme().foreground.opacity(0.07))
                    .child(crate::icons::symbol_weight(Icon::LockFill, 8.5, 0.3, move || theme().muted_foreground)),
            )
            .child(view().w(px(12.0)).h(px(2.0)).rounded_full().bg(move || theme().foreground.opacity(0.1)))
    };
    view()
        .flex_row()
        .items_center()
        .gap(px(16.0))
        .child(
            view()
                .flex_row()
                .items_center()
                .gap(px(6.0))
                .child(app_icon(50.0))
                .child(link())
                .child(location_mark(LocationIcon::Cloud(provider), 42.0)),
        )
        .child(
            view()
                .flex_1()
                .gap(px(3.0))
                .child(
                    text(move || {
                        s.label(match provider {
                            Provider::Webdav => "Connect to your own server or NAS",
                            Provider::Nextcloud => "Sign in with your Nextcloud account",
                            Provider::Dropbox => "Sign in with your Dropbox account",
                            Provider::Onedrive => "Sign in with your Microsoft account",
                            Provider::GoogleDrive => "Sign in with your Google account",
                        })
                    })
                    .text_size(px(13.5))
                    .font_weight(FontWeight::SEMIBOLD),
                )
                .child(
                    text(move || {
                        // Restoring only reads; otherwise only the encrypted vault is stored.
                        match (s.cloud_restore.get(), provider == Provider::Webdav) {
                            (false, true) => s.label("Only the encrypted vault is stored on your server."),
                            (false, false) => s.fmt("Only the encrypted vault is stored in {}.", &[provider.name()]),
                            (true, true) => s.label("Safay only reads your encrypted backup from your server."),
                            (true, false) => s.fmt("Safay only reads your encrypted backup from {}.", &[provider.name()]),
                        }
                    })
                    .text_size(px(12.0))
                    .text_color(move || theme().muted_foreground),
                ),
        )
}
/// The OAuth form: pairing header, where the backup is kept, and the three steps (they become progress).
fn oauth_setup(s: Store, provider: Provider) -> impl IntoElement {
    // The app folder as the service's own app shows it: Google Drive's is the folder named in `cloud-keys.toml`.
    let folder = move || match provider {
        Provider::GoogleDrive => s.fmt("My Drive › {}", &[option_env!("SAFAY_GOOGLE_FOLDER_NAME").unwrap_or_default()]),
        _ => s.label("Apps › Safay"),
    };
    let steps: Vec<Box<dyn Fn() -> SharedString>> = vec![
        Box::new(move || s.fmt("Sign in to {} and allow Safay", &[provider.name()])),
        Box::new(move || s.fmt("Connect to your {} account", &[provider.name()])),
        Box::new(move || s.label(if s.cloud_restore.get() { "Find your backup" } else { "Look for an existing backup, then back up" })),
    ];
    // 0: not started; 1: signing in; 2: connecting (after the browser returns); 3: looking for the backup.
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
        .gap(px(18.0))
        .child(pairing_header(s, provider))
        .child(
            view()
                .flex_row()
                .gap(px(11.0))
                .p(px(12.0))
                .rounded(px(10.0))
                .bg(move || tok("code-accent").opacity(0.07))
                .border_1()
                .border_color(move || tok("code-accent").opacity(0.18))
                .child(view().w(px(22.0)).items_center().child(crate::icons::symbol(Icon::FolderFill, 17.0, move || tok("code-accent"))))
                .child(
                    view().flex_1().gap(px(3.0)).child(text(folder).text_size(px(13.0)).font_weight(FontWeight::SEMIBOLD)).child(
                        text(move || {
                            let key = if s.cloud_restore.get() {
                                "Safay looks for your backup in this folder in your {}."
                            } else {
                                "Your backup is kept in this folder in your {}."
                            };
                            s.fmt(key, &[provider.name()])
                        })
                        .text_size(px(12.0))
                        .text_color(move || theme().muted_foreground),
                    ),
                ),
        )
        .child(progress_steps(steps, phase))
        // macOS signs in in a window of its own; elsewhere the page is in the browser.
        .child(when(move || crate::platform::supports::BROWSER_SIGN_IN, move || browser_note(s, provider)))
        .child(when(
            move || s.cloud_form.with(|f| f.error.is_some()),
            move || {
                inline_notice(
                    Icon::Warning,
                    move || theme().destructive,
                    move || s.cloud_form.with(|f| f.error.clone().unwrap_or_default()),
                )
            },
        ))
}
/// While a sign-in waits in the browser: where to go, and that Safay goes on by itself.
fn browser_note(s: Store, provider: Provider) -> impl IntoElement {
    when(
        move || s.cloud_form.with(|f| f.waiting),
        move || {
            text(move || {
                s.fmt("Sign in to {} in the browser window that opened and allow Safay. Safay continues automatically.", &[provider.name()])
            })
            .text_size(px(12.0))
            .text_color(move || theme().muted_foreground)
        },
    )
}
/// Steps that become progress: `phase()` is the current step, from 1 (0: not started); earlier steps are checked, the
/// current one spins.
fn progress_steps(steps: Vec<Box<dyn Fn() -> SharedString>>, phase: impl Fn() -> usize + Copy + 'static) -> impl IntoElement {
    view().gap(px(10.0)).children(steps.into_iter().enumerate().map(move |(i, label)| {
        let state = move || {
            let p = phase();
            if p == 0 {
                0
            } else if i + 1 < p {
                3
            } else if i + 1 == p {
                2
            } else {
                1
            }
        };
        view()
            .flex_row()
            .items_center()
            .gap(px(10.0))
            .child(
                view().size(px(20.0)).items_center().justify_center().child(
                    when(move || state() == 3, move || crate::icons::symbol(Icon::CheckCircleFill, 16.0, move || tok("code-accent")))
                        .otherwise(move || {
                            // The current step spins.
                            when(move || state() == 2, || spinner(14.0)).otherwise(move || {
                                view()
                                    .size(px(18.0))
                                    .rounded_full()
                                    .border_1()
                                    .border_color(move || tok("tertiary"))
                                    .items_center()
                                    .justify_center()
                                    .child(
                                        text((i + 1).to_string())
                                            .text_size(px(10.5))
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .text_color(move || theme().muted_foreground),
                                    )
                            })
                        }),
                ),
            )
            .child({
                // The current step is semibold; the weight is fixed per text, so the two weights swap.
                let label = std::rc::Rc::new(label);
                let styled = move |weight: FontWeight| {
                    let label = label.clone();
                    move || {
                        let label = label.clone();
                        text(move || label()).text_size(px(12.5)).font_weight(weight).text_color(move || match state() {
                            1 => tok("tertiary"),
                            0 => theme().muted_foreground,
                            _ => theme().foreground,
                        })
                    }
                };
                when(move || state() == 2, styled(FontWeight::SEMIBOLD)).otherwise(styled(FontWeight::NORMAL))
            })
            .into_element()
    }))
}
fn setup(s: Store, provider: Provider) -> AnyElement {
    if matches!(provider, Provider::Dropbox | Provider::Onedrive | Provider::GoogleDrive) {
        return oauth_setup(s, provider).into_element();
    }
    server_setup(s, provider).into_element()
}
fn server_setup(s: Store, provider: Provider) -> impl IntoElement {
    let form = move || s.cloud_form.get();
    let field =
        move |id: &'static str, label: &'static str, placeholder: &'static str, get: Box<dyn Fn() -> String>, set: Box<dyn Fn(&str)>| {
            labeled(move || s.label(label), text_field(s, id, move || s.label(label), move || placeholder.into(), get, set, false, || {}))
        };
    view()
        .gap(px(18.0))
        .child(pairing_header(s, provider))
        // Once connecting, the form gives way to the steps (they become progress); Nextcloud signs in in the browser.
        .child(
            when(
                move || s.cloud_form.with(|f| f.working),
                move || {
                    let mut steps: Vec<Box<dyn Fn() -> SharedString>> = Vec::new();
                    if provider == Provider::Nextcloud {
                        steps.push(Box::new(move || s.label("Get the sign-in page from your server")));
                        steps.push(Box::new(move || s.fmt("Sign in to {} and allow Safay", &[provider.name()])));
                        steps.push(Box::new(move || s.fmt("Connect to your {} account", &[provider.name()])));
                    } else {
                        steps.push(Box::new(move || s.label("Sign in to your server")));
                    }
                    let last = steps.len() + 1;
                    steps.push(Box::new(move || {
                        s.label(if s.cloud_restore.get() { "Find your backup" } else { "Look for an existing backup, then back up" })
                    }));
                    let phase = move || {
                        s.cloud_form.with(|f| {
                            if f.checking {
                                last
                            } else if f.waiting {
                                2
                            } else if f.allowed {
                                3
                            } else {
                                1
                            }
                        })
                    };
                    view().gap(px(14.0)).child(progress_steps(steps, phase)).child(browser_note(s, provider))
                },
            )
            .otherwise(move || {
                // A confirmation is about the address already entered: it takes the fields' place, so it stays in view.
                when(
                    move || s.cloud_form.with(|f| f.needs_consent || f.untrusted.is_some()),
                    move || {
                        view()
                            .child(when(move || s.cloud_form.with(|f| f.needs_consent), move || insecure_confirmation(s, provider)))
                            .child(when(
                                move || s.cloud_form.with(|f| f.untrusted.is_some()),
                                move || certificate_confirmation(s, provider),
                            ))
                    },
                )
                .otherwise(move || {
                    view()
                        .gap(px(18.0))
                        .child(
                            view()
                                .gap(px(6.0))
                                .child(field(
                                    "cloud-address",
                                    "Server Address",
                                    "https://",
                                    Box::new(move || form().address),
                                    Box::new(move |v: &str| {
                                        s.cloud_form.update(|f| {
                                            f.address = v.into();
                                            f.needs_consent = false;
                                            f.insecure = false;
                                            // A confirmed certificate belongs to the address it was read from.
                                            f.untrusted = None;
                                            f.trusted = None;
                                            f.error = None;
                                        })
                                    }),
                                ))
                                .child(
                                    text(move || {
                                        s.label(if provider == Provider::Webdav {
                                            "The WebDAV address of the folder, for example https://example.com/remote.php/webdav/"
                                        } else {
                                            "For example https://cloud.example.com. You’ll sign in and allow Safay in your browser."
                                        })
                                    })
                                    .text_size(px(11.5))
                                    .text_color(move || theme().muted_foreground),
                                ),
                        )
                        .child(when(
                            move || provider == Provider::Webdav,
                            move || {
                                view()
                                    .flex_row()
                                    .gap(px(12.0))
                                    .child(view().flex_1().child(field(
                                        "cloud-username",
                                        "Username",
                                        "",
                                        Box::new(move || form().username),
                                        Box::new(move |v: &str| s.cloud_form.update(|f| f.username = v.into())),
                                    )))
                                    .child(view().flex_1().child(labeled(
                                        move || s.label("Password"),
                                        secure_field_with(
                                            s,
                                            "cloud-password",
                                            move || s.label("Password"),
                                            move || form().password.to_string(),
                                            move |v| s.cloud_form.update(|f| f.password = Zeroizing::new(v.into())),
                                            move || s.connect_cloud(provider),
                                            |_| {},
                                            false,
                                            false,
                                        ),
                                    )))
                            },
                        ))
                })
            }),
        )
        .child(when(
            move || s.cloud_form.with(|f| f.error.is_some()),
            move || {
                inline_notice(
                    Icon::Warning,
                    move || theme().destructive,
                    move || s.cloud_form.with(|f| f.error.clone().unwrap_or_default()),
                )
            },
        ))
}
/// The plain-HTTP confirmation: orange box, explanation, Cancel / Use Unencrypted Connection.
fn insecure_confirmation(s: Store, provider: Provider) -> impl IntoElement {
    view()
        .gap(px(10.0))
        .p(px(13.0))
        .rounded(px(10.0))
        .bg(move || tok("warning").opacity(0.08))
        .border_1()
        .border_color(move || tok("warning").opacity(0.32))
        .child(
            view()
                .flex_row()
                .items_center()
                .gap(px(6.0))
                .child(crate::icons::symbol(Icon::LockOpenFill, 12.5, move || tok("warning")))
                .child(
                    text(move || s.label("Unencrypted connection"))
                        .text_size(px(12.5))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(move || tok("warning")),
                ),
        )
        .child(
            text(move || s.label("Your password is sent over HTTP in plain text. Only continue on a network you trust."))
                .text_size(px(11.5))
                .text_color(move || theme().muted_foreground),
        )
        .child(
            view()
                .flex_row()
                .gap(px(10.0))
                .child(spacer())
                .child(
                    push_button(move || s.label("Cancel"), Kind::Standard, false)
                        .on_press(move || s.cloud_form.update(|f| f.needs_consent = false)),
                )
                .child(push_button(move || s.label("Use Unencrypted Connection"), Kind::Standard, false).on_press(move || {
                    s.cloud_form.update(|f| {
                        f.insecure = true;
                        f.needs_consent = false;
                    });
                    s.connect_cloud(provider);
                })),
        )
}

/// Colon-separated, upper-case hex: `AB:CD:…`.
fn formatted_fingerprint(fingerprint: &[u8; 32]) -> String {
    fingerprint.iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(":")
}
/// The certificate confirmation (SECURITY.md): the system does not trust the server's certificate, so its
/// SHA-256 fingerprint is shown for the user to compare with what the server shows; once confirmed, only this certificate
/// is trusted. A pinned certificate that changed is never offered here.
fn certificate_confirmation(s: Store, provider: Provider) -> impl IntoElement {
    let fingerprint = move || s.cloud_form.with(|f| f.untrusted);
    view()
        .gap(px(10.0))
        .p(px(14.0))
        .rounded(px(10.0))
        .bg(move || tok("warning").opacity(0.08))
        .border_1()
        .border_color(move || tok("warning").opacity(0.32))
        .child(
            view()
                .flex_row()
                .items_center()
                .gap(px(6.0))
                .child(crate::icons::symbol(Icon::ShieldWarningFill, 12.5, move || tok("warning")))
                .child(
                    text(move || s.label("The server’s certificate can’t be verified"))
                        .text_size(px(12.5))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(move || tok("warning")),
                ),
        )
        .child(
            text(move || {
                s.label("Continue only if this fingerprint matches the one your server shows. Safay will trust only this certificate.")
            })
            .text_size(px(11.5))
            .text_color(move || theme().muted_foreground),
        )
        .child(
            view()
                .gap(px(3.0))
                .child(
                    text(move || s.label("SHA-256 fingerprint"))
                        .text_size(px(11.0))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(move || theme().muted_foreground),
                )
                .child(
                    view().selectable(true).child(
                        text(move || fingerprint().map(|f| formatted_fingerprint(&f)).unwrap_or_default())
                            .test_id("certificate-fingerprint")
                            .font_family("ui-monospace")
                            .text_size(px(11.5)),
                    ),
                ),
        )
        .child(
            view()
                .flex_row()
                .gap(px(10.0))
                .child(spacer())
                .child(push_button(move || s.label("Cancel"), Kind::Standard, false).on_press(move || s.decline_certificate()))
                .child(push_button(move || s.label("Trust This Certificate"), Kind::Standard, false).on_press(move || {
                    if let Some(f) = fingerprint() {
                        s.trust_certificate(provider, f);
                    }
                })),
        )
}

fn conflict(s: Store) -> impl IntoElement {
    let review = move || s.review.get();
    let side = move |title: Box<dyn Fn() -> SharedString>, glyph: Icon, count: Box<dyn Fn() -> usize>, detail: Box<dyn Fn() -> String>| {
        view()
            .flex_1()
            .flex_row()
            .items_center()
            .gap(px(10.0))
            .p(px(12.0))
            .rounded(px(10.0))
            .bg(move || tok("hover").opacity(0.6))
            .child(view().text_color(move || tok("code-accent")).child(icon(glyph).size(px(20.0))))
            .child(
                view()
                    .gap(px(2.0))
                    .child(heading(title, 13.0))
                    .child(
                        text(move || s.fmt("Accounts: {}", &[&count().to_string()]))
                            .text_size(px(12.0))
                            .text_color(move || theme().muted_foreground),
                    )
                    .child(text(detail).number_of_lines(1).text_size(px(11.0)).text_color(move || theme().muted_foreground)),
            )
    };
    when(
        move || review().is_none(),
        move || view().min_h(px(260.0)).items_center().justify_center().child(note(move || s.label("Comparing…"))),
    )
    .otherwise(move || {
        view()
            .gap(px(14.0))
            .child(
                view()
                    .flex_row()
                    .gap(px(12.0))
                    .child(side(
                        Box::new(move || this_device(s)),
                        Icon::Window,
                        Box::new(move || review().map(|r| r.local_count).unwrap_or(0)),
                        Box::new(crate::platform::device_name),
                    ))
                    .child(side(
                        Box::new(move || s.label("Backup")),
                        Icon::Cloud,
                        Box::new(move || review().map(|r| r.remote_count).unwrap_or(0)),
                        Box::new(move || review().and_then(|r| r.remote_device).unwrap_or_default()),
                    )),
            )
            .child(
                scroll().h(px(220.0)).rounded(px(10.0)).border_1().border_color(move || tok("separator").opacity(0.6)).child(
                    view()
                        .p(px(12.0))
                        .gap(px(12.0))
                        .child(group(
                            s,
                            move || review().map(|r| r.only_local.len()).unwrap_or(0),
                            if look() == Look::Mac { "Only on this Mac ({})" } else { "Only on this device ({})" },
                            move || review().map(|r| r.only_local).unwrap_or_default(),
                        ))
                        .child(group(
                            s,
                            move || review().map(|r| r.only_remote.len()).unwrap_or(0),
                            "Only in the backup ({})",
                            move || review().map(|r| r.only_remote).unwrap_or_default(),
                        ))
                        .child(changed_group(s))
                        .child(when(
                            move || {
                                review().is_some_and(|r| {
                                    !r.categories_only_local.is_empty()
                                        || !r.categories_only_remote.is_empty()
                                        || !r.categories_renamed.is_empty()
                                })
                            },
                            move || {
                                view().gap(px(4.0)).child(section_label(move || s.label("Categories"))).child(
                                    text(move || {
                                        let r = review().unwrap_or_default();
                                        let mut lines: Vec<String> = r
                                            .categories_only_remote
                                            .iter()
                                            .map(|c| s.fmt("Category only in the backup: {}", &[c]).to_string())
                                            .collect();
                                        lines.extend(
                                            r.categories_only_local
                                                .iter()
                                                .map(|c| s.fmt("Category only on this device: {}", &[c]).to_string()),
                                        );
                                        lines.extend(r.categories_renamed.iter().map(|(a, b)| format!("{a} → {b}")));
                                        lines.join("\n")
                                    })
                                    .text_size(px(12.0))
                                    .line_height(px(18.0)),
                                )
                            },
                        )),
                ),
            )
            .child(choice_row(
                s,
                0,
                "Merge Both",
                "Keep accounts from both sides. For accounts changed on both, use the version you picked above.",
            ))
            .child(choice_row(
                s,
                1,
                if look() == Look::Mac { "Keep This Mac’s Copy" } else { "Keep This Device’s Copy" },
                "Overwrite the backup with this copy.",
            ))
            .child(choice_row(s, 2, "Use the Backup’s Copy", "Replace this copy with the backup."))
    })
}
fn group(
    s: Store,
    count: impl Fn() -> usize + Copy + 'static,
    title: &'static str,
    rows: impl Fn() -> Vec<(String, String)> + Copy + 'static,
) -> impl IntoElement {
    when(
        move || count() > 0,
        move || {
            view().gap(px(4.0)).child(section_label(move || s.fmt(title, &[&count().to_string()]))).child(
                text(move || {
                    rows().into_iter().map(|(i, u)| if u.is_empty() { i } else { format!("{i} — {u}") }).collect::<Vec<_>>().join("\n")
                })
                .text_size(px(12.5))
                .line_height(px(19.0)),
            )
        },
    )
}
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
fn changed_group(s: Store) -> impl IntoElement {
    let rows = move || s.review.with(|r| r.as_ref().map(|r| r.changed.clone()).unwrap_or_default());
    when(
        move || !rows().is_empty(),
        move || {
            view()
                .gap(px(6.0))
                .child(section_label(move || s.fmt("Different on both sides ({})", &[&rows().len().to_string()])))
                .child(note(move || s.label("Pick which version Merge Both keeps for each account.")))
                .child(each(
                    rows,
                    |r| r.0,
                    move |row, _| {
                        let id = move || row.with(|r| r.0);
                        let remote = move || s.prefer_remote.with(|p| p.contains(&id()));
                        view()
                            .flex_row()
                            .items_center()
                            .gap(px(10.0))
                            .child(
                                view()
                                    .flex_1()
                                    .min_w(px(0.0))
                                    .child(text(move || row.with(|r| format!("{} — {}", r.1, r.2))).number_of_lines(1).text_size(px(12.5)))
                                    .child(
                                        text(move || row.with(|r| change_text(s, &r.3)))
                                            .number_of_lines(1)
                                            .text_size(px(11.0))
                                            .text_color(move || theme().muted_foreground),
                                    ),
                            )
                            .child(
                                segmented(
                                    vec![
                                        (false, Box::new(move || this_device(s)) as Box<dyn Fn() -> SharedString>),
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
                                .w(px(170.0)),
                            )
                    },
                ))
        },
    )
}
fn choice_row(s: Store, value: u8, title: &'static str, detail: &'static str) -> impl IntoElement {
    let on = move || s.form.with(|f| f.algorithm as u8 == value);
    button()
        .selected(on)
        .accessibility_label(move || Some(s.label(title)))
        .flex_row()
        .items_center()
        .gap(px(10.0))
        .p(px(10.0))
        .rounded(px(9.0))
        .bg(move || if on() { tok("code-accent").opacity(0.08) } else { theme().background.opacity(0.0) })
        .border_1()
        .border_color(move || if on() { tok("code-accent").opacity(0.5) } else { tok("separator").opacity(0.6) })
        .focus(|st| st.border_color(move || theme().ring))
        .on_press(move || s.form.update(|f| f.algorithm = u32::from(value)))
        .child(
            view()
                .size(px(16.0))
                .rounded_full()
                .border(move || px(if on() { 5.0 } else { 1.0 }))
                .border_color(move || if on() { theme().primary } else { tok("input") }),
        )
        .child(
            view()
                .flex_1()
                .gap(px(2.0))
                .child(text(move || s.label(title)).text_size(px(13.0)).font_weight(FontWeight::SEMIBOLD))
                .child(text(move || s.label(detail)).text_size(px(11.5)).text_color(move || theme().muted_foreground)),
        )
}

fn restore(s: Store) -> impl IntoElement {
    view()
        .gap(px(14.0))
        .child(
            text(move || {
                s.label(if look() == Look::Mac {
                    "To use that backup, restore from it and unlock with its master password. This replaces the vault on this Mac."
                } else {
                    "To use that backup, restore from it and unlock with its master password. This replaces the vault on this device."
                })
            })
            .text_size(px(13.0))
            .line_height(px(19.0)),
        )
        .child(view().gap(px(7.0)).child(field_label(s, "Existing Master Password")).child(secure_field(
            s,
            "backup-password",
            move || s.label("Existing Master Password"),
            move || s.form.with(|f| f.password.to_string()),
            move |v| s.form.update(|f| f.password = Zeroizing::new(v.into())),
            move || s.restore_unreadable(),
            || {},
            true,
        )))
}
