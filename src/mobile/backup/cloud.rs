//! Connecting a cloud service, in a sheet from the bottom: signing in through the browser, or a server's address and
//! account; an unencrypted connection and an unverified certificate are confirmed first.
use super::*;

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
    let folder = move || app_folder(s, provider);
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
    let formatted = move || fingerprint().map(|f| formatted_fingerprint(&f, 8)).unwrap_or_default();
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
