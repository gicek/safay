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
        if s.cloud_form.with(|f| f.working && !f.waiting && !connecting(s).is_some_and(Provider::has_server)) {
            return;
        }
        dismiss_keyboard();
        s.close_sheet();
    };
    view()
        .child(ui::bottom_sheet(
            move || connecting(s).is_some(),
            close,
            // iOS: a column the system's sheet fills, so the setup inside it can fill it in turn and scroll.
            move || {
                view()
                    .flex_grow(if cfg!(target_os = "ios") { 1.0 } else { 0.0 })
                    .flex_shrink(1.0)
                    .min_h(px(0.0))
                    .child(dynamic(move || connecting(s).map(|p| setup(s, p, close).into_element())))
                    .into_element()
            },
        ))
        .child(when(move || s.cloud_form.with(|f| f.needs_consent) && connecting(s).is_some(), move || insecure(s)))
        .child(when(move || s.cloud_form.with(|f| f.untrusted.is_some()) && connecting(s).is_some(), move || certificate(s)))
}

fn setup(s: Store, provider: Provider, close: impl Fn() + Copy + 'static) -> impl IntoElement {
    let restore = move || s.cloud_restore.get();
    let oauth = is_oauth(provider);
    let ready = move || oauth || s.cloud_form.with(|f| crate::pages::backup::form_ready(provider, f));
    let go = move || {
        if !ready() || s.cloud_form.with(|f| f.working) {
            return;
        }
        dismiss_keyboard();
        if oauth { s.connect_oauth(provider) } else { s.connect_cloud(provider) }
    };
    let waiting = move || s.cloud_form.with(|f| f.waiting);
    // The bar's leading button: while the browser sign-in waits, it stops that and the form stays; otherwise the sheet
    // goes.
    let leading = move || {
        if waiting() {
            crate::platform::cancel_web_sign_in();
            s.cloud_form.update(|f| {
                f.working = false;
                f.waiting = false;
            })
        } else {
            close()
        }
    };
    // The bar's action: "Choose" while a folder is picked, else "Connect" ("Sign In" through the browser); a spinner
    // while it works or the browser sign-in waits.
    let picking = move || s.folder_browser.with(|b| b.open);
    let label = move || {
        s.label(if picking() {
            "Choose"
        } else if oauth {
            "Sign In"
        } else {
            "Connect"
        })
    };
    let enabled = move || if picking() { crate::pages::backup::folder_usable(s) } else { ready() };
    let busy = move || if picking() { s.folder_browser.with(|b| b.working) } else { s.cloud_form.with(|f| f.working) };
    let act = move || {
        if picking() { s.use_folder() } else { go() }
    };
    // The bar (its buttons are 44 pt, 4 above and below): the action is there, none under the scroll view.
    const CHROME: f32 = 52.0;
    view()
        // iOS: the system's sheet sizes itself; this fills it, so the scroll view below takes what the title and the
        // action leave and scrolls (a long list of a NAS's folders). Sized by its content, it would run past the sheet.
        .flex_grow(if cfg!(target_os = "ios") { 1.0 } else { 0.0 })
        .flex_shrink(1.0)
        .min_h(px(0.0))
        .pb(move || ui::sheet_bottom(12.0))
        // The sheet's bar, as a form's: leaving at the start, the action at the end. iOS: "Cancel" and the action in
        // glass, the title in the middle. Android (a full-screen dialog's): the close button, the title, the action in
        // words.
        .child({
            let title_ = move || s.fmt(if restore() { "Restore from {}" } else { "Connect to {}" }, &[provider.name()]);
            if ui::IOS {
                view()
                    .h(px(ui::TOUCH + 8.0))
                    .px(px(16.0))
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .child(ui::title(text(title_)).absolute().left(px(104.0)).right(px(104.0)).text_center().number_of_lines(1))
                    .child(ui::glass_text_button(move || s.label("Cancel")).test_id("Close").on_press(leading))
                    .child(bar_action(s, label, enabled, busy, act))
                    .into_element()
            } else {
                view()
                    .h(px(ui::TOUCH + 8.0))
                    .pl(px(4.0))
                    .pr(px(12.0))
                    .flex_row()
                    .items_center()
                    .gap(px(4.0))
                    .child(ui::icon_button(move || s.label("Close"), || Icon::Close).on_press(leading))
                    .child(ui::title_large(text(title_)).flex_1().min_w(px(0.0)).number_of_lines(1))
                    .child(bar_action(s, label, enabled, busy, act))
                    .into_element()
            }
        })
        .child(
            scroll()
                .max_h(move || ui::sheet_room(CHROME))
                .flex_grow(if cfg!(target_os = "ios") { 1.0 } else { 0.0 })
                .flex_shrink(1.0)
                .min_h(px(0.0))
                .keyboard_dismiss_mode(KeyboardDismissMode::None)
                .keyboard_taps(KeyboardTaps::Handled)
                .child(
                    view()
                        .px(px(24.0))
                        .pt(px(8.0))
                        .gap(px(20.0))
                        .child(pairing(s, provider))
                        .child(when(move || s.folder_browser.with(|b| b.open), move || folder_browser(s)).otherwise(move || {
                            when(move || oauth, move || oauth_body(s, provider)).otherwise(move || server_body(s, provider, go))
                        }))
                        .child(when(
                            move || s.cloud_form.with(|f| f.error.is_some()),
                            move || {
                                ui::body(text(move || s.cloud_form.with(|f| f.error.clone().unwrap_or_default())))
                                    .text_color(move || theme().destructive)
                            },
                        )),
                ),
        )
}

/// The setup bar's action: "Connect" (or "Choose", "Sign In") in glass on iOS, in words in the accent color on Android;
/// dimmed until it can be pressed, a spinner while it works (screen readers hear "Connecting").
fn bar_action(
    s: Store,
    label: impl Fn() -> SharedString + Copy + 'static,
    enabled: impl Fn() -> bool + Copy + 'static,
    busy: impl Fn() -> bool + Copy + 'static,
    act: impl Fn() + 'static,
) -> impl IntoElement {
    let ink = move || if enabled() { if ui::IOS { theme().foreground } else { theme().primary } } else { theme().muted_foreground };
    let b = nsh_base::button()
        .test_id("connect")
        .accessibility_label(move || Some(if busy() { s.label("Connecting") } else { label() }))
        .disabled(move || !enabled() || busy())
        .h(px(if ui::IOS { ui::TOUCH } else { 40.0 }))
        .min_w(px(if ui::IOS { 72.0 } else { 64.0 }))
        .px(px(if ui::IOS { 20.0 } else { 12.0 }))
        .items_center()
        .justify_center()
        .on_press(act)
        .child(when(busy, move || crate::components::spinner_in(20.0, ink)).otherwise(move || {
            let t = text(label).number_of_lines(1).text_color(ink);
            if ui::IOS {
                ui::body_large(t).font_weight(FontWeight::SEMIBOLD)
            } else {
                t.text_size(px(14.0)).font_weight(FontWeight::MEDIUM)
            }
        }));
    if ui::IOS {
        ui::glass(b.active(|st| st.scale(0.94))).into_element()
    } else {
        b.rounded_full().active(|st| st.bg(move || tok("pressed"))).into_element()
    }
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
                if let Some(title) = crate::pages::backup::folder_step_title(s) {
                    return title;
                }
                s.label(match provider {
                    Provider::Webdav => "Connect to your own server or NAS",
                    Provider::Synology => "Sign in to your Synology NAS",
                    Provider::Nextcloud => "Sign in with your Nextcloud account",
                    Provider::Dropbox => "Sign in with your Dropbox account",
                    Provider::Onedrive => "Sign in with your Microsoft account",
                    _ => "Sign in with your Google account",
                })
            }))
            .text_center(),
        )
        .child(
            ui::dim(ui::body(text(move || match (s.cloud_restore.get(), matches!(provider, Provider::Webdav | Provider::Synology)) {
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
    let otp = element_ref();
    let synology = provider == Provider::Synology;
    let webdav = provider == Provider::Webdav || synology;
    // After the password, Synology's optional code; WebDAV connects.
    let after_password = move || if synology { otp.focus() } else { go() };
    view()
        .gap(px(12.0))
        .child(ui::field(
            s,
            "server-address",
            Entry::Text,
            move || s.label(if synology { "QuickConnect ID or Address" } else { "Server Address" }),
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
                s.label(match provider {
                    Provider::Webdav => "The WebDAV address of the folder, for example https://example.com/remote.php/webdav/",
                    Provider::Synology => "For example my-nas, or https://192.168.1.10:5001 on your network",
                    _ => "For example https://cloud.example.com. You’ll sign in and allow Safay in your browser.",
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
                        after_password,
                        |_| {},
                        || false,
                        if synology { ReturnKeyType::Next } else { ReturnKeyType::Go },
                        Some(password),
                    ))
            },
        ))
        .child(when(
            move || synology,
            move || {
                view()
                    .gap(px(12.0))
                    .child(ui::field(
                        s,
                        "server-otp",
                        Entry::Text,
                        move || s.label("Verification Code (Optional)"),
                        move || s.cloud_form.with(|f| f.otp.to_string()),
                        move |v| s.cloud_form.update(|f| f.otp = Zeroizing::new(v.to_owned())),
                        go,
                        |_| {},
                        || false,
                        ReturnKeyType::Go,
                        Some(otp),
                    ))
                    .child(
                        ui::dim(ui::caption(text(move || {
                            s.label("Only if this account uses two-step verification: the code from your authenticator app.")
                        })))
                        .px(px(16.0)),
                    )
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

/// Picking the folder for the backup (WebDAV, Nextcloud, Synology): the folder's path with up and New Folder, its subfolders, and what it
/// means for the backup.
fn folder_browser(s: Store) -> impl IntoElement {
    use crate::pages::backup::{backup_saved, folder_hint, folder_title};
    let busy = move || s.folder_browser.with(|b| b.working);
    let rows = move || {
        let folders = s.folder_browser.with(|b| b.listing.as_ref().map(|l| l.folders.clone()));
        let backup = s.folder_browser.with(|b| b.listing.as_ref().filter(|l| l.has_backup).map(|l| l.backup_saved_ms));
        let found = move || {
            view()
                .flex_row()
                .items_center()
                .gap(px(16.0))
                .px(px(16.0))
                .h(px(64.0))
                .child(view().text_color(move || theme().primary).child(icon(Icon::LockShieldFill).size(px(22.0))))
                .child(
                    view()
                        .flex_1()
                        .gap(px(2.0))
                        .child(ui::label(text(move || s.label("Safay Backup Found"))))
                        .child(ui::dim(ui::caption(text(move || backup_saved(s, backup.flatten()))))),
                )
        };
        match folders {
            None => view().h(px(96.0)).items_center().justify_center().child(crate::components::spinner(20.0)).into_element(),
            Some(folders) if folders.is_empty() && backup.is_some() => found().into_element(),
            Some(folders) if folders.is_empty() => view()
                .h(px(64.0))
                .items_center()
                .justify_center()
                .child(ui::dim(ui::body(text(move || s.label("No folders")))))
                .into_element(),
            Some(folders) => view()
                .child(when(move || backup.is_some(), found))
                .children(folders.into_iter().map(|name| {
                    let label = name.clone();
                    nsh_base::button()
                        .accessibility_label({
                            let name = name.clone();
                            move || Some(name.clone().into())
                        })
                        .disabled(busy)
                        .flex_row()
                        .items_center()
                        .gap(px(16.0))
                        .px(px(16.0))
                        .h(px(56.0))
                        .on_press(move || s.open_folder(name.clone()))
                        .child(view().text_color(move || theme().primary).child(icon(Icon::FolderFill).size(px(22.0))))
                        .child(ui::body_large(text(label)).flex_1().number_of_lines(1))
                        .child(view().text_color(move || theme().muted_foreground).child(icon(Icon::ChevronRight).size(px(16.0))))
                        .into_element()
                }))
                .into_element(),
        }
    };
    view()
        .gap(px(12.0))
        .child(
            view()
                .flex_row()
                .items_center()
                .gap(px(4.0))
                .child(ui::icon_button(move || s.label("Back"), || Icon::ChevronLeft).disabled(busy).on_press(move || s.folder_up()))
                .child(ui::title(text(move || folder_title(s))).flex_1().number_of_lines(1))
                .child(when(
                    move || crate::pages::backup::can_create_folder(s),
                    move || {
                        ui::icon_button(move || s.label("New Folder"), || Icon::FolderPlus)
                            .disabled(move || busy() || s.folder_browser.with(|b| b.in_shares() || b.new_folder.is_some()))
                            .on_press(move || s.start_folder())
                    },
                )),
        )
        .child(view().rounded(px(24.0)).bg(move || theme().secondary).overflow_hidden().child(dynamic(rows)))
        .child(when(
            move || folder_hint(s).is_some(),
            move || ui::dim(ui::caption(text(move || folder_hint(s).map(|(_, t)| t).unwrap_or_default()))).px(px(16.0)),
        ))
        // While a new folder is named, its dialog says what went wrong.
        .child(when(
            move || s.folder_browser.with(|b| b.error.is_some() && b.new_folder.is_none()),
            move || {
                ui::body(text(move || s.folder_browser.with(|b| b.error.clone().unwrap_or_default())))
                    .text_color(move || theme().destructive)
            },
        ))
}

/// A new folder's name, in the folder shown (`folder_browser.new_folder` while it is asked for): iOS, the system's alert
/// with a text field; Android, a dialog. Over the whole interface, not inside the setup sheet, as the other dialogs are.
/// A name the server refuses asks again, saying why; once made, the new folder opens.
pub fn new_folder(s: Store) -> AnyElement {
    if ui::IOS {
        system_new_folder(s);
        return view().into_element();
    }
    when(move || s.folder_browser.with(|b| b.new_folder.is_some()), move || drawn_new_folder(s)).into_element()
}
fn system_new_folder(s: Store) {
    // An alert is up; a name is being made (no alert until it is refused, an error, or the folder opens).
    let asking = stored(false);
    let saving = stored(false);
    effect(move || {
        let (name, error) = s.folder_browser.with(|b| (b.new_folder.clone(), b.error.clone()));
        let Some(name) = name else {
            saving.replace(false);
            return;
        };
        if asking.get() {
            return;
        }
        if saving.get() {
            if error.is_none() {
                return;
            }
            saving.replace(false);
        }
        asking.replace(true);
        untracked(move || {
            let mut alert = nsh_base::alert_dialog()
                .title(s.label("New Folder"))
                .input(
                    nsh_base::AlertDialogInput::new()
                        .default_value(name)
                        .placeholder(s.label("Folder Name"))
                        .select_text_on_focus(true)
                        .required(true),
                )
                .cancel("cancel", s.label("Cancel"))
                .action("create", s.label("Create"));
            if let Some(error) = error {
                alert = alert.description(error);
            }
            let answer = alert.show();
            s.task(async move {
                let reply = answer.await.ok();
                asking.replace(false);
                match reply.and_then(|r| if r.action.as_deref() == Some("create") { r.value } else { None }) {
                    Some(name) => {
                        saving.replace(true);
                        s.folder_browser.update(|b| {
                            b.new_folder = Some(name.to_string());
                            b.error = None;
                        });
                        s.create_folder();
                    }
                    None => s.folder_browser.update(|b| {
                        b.new_folder = None;
                        b.error = None;
                    }),
                }
            });
        });
    });
}
fn drawn_new_folder(s: Store) -> impl IntoElement {
    let blank = move || s.folder_browser.with(|b| b.new_folder.as_deref().is_none_or(|n| n.trim().is_empty()));
    let busy = move || s.folder_browser.with(|b| b.working);
    let submit = move || {
        if !blank() && !busy() {
            s.create_folder()
        }
    };
    let cancel = move || {
        dismiss_keyboard();
        s.folder_browser.update(|b| {
            b.new_folder = None;
            b.error = None;
        })
    };
    // Its one field takes the keyboard at once, as the system's alert with a text field does.
    let field = element_ref();
    request_animation_frame(move |_| field.focus());
    ui::dialog(
        move || s.label("New Folder"),
        move || {
            view()
                .gap(px(8.0))
                .child(ui::field(
                    s,
                    "new-folder",
                    Entry::Text,
                    move || s.label("Folder Name"),
                    move || s.folder_browser.with(|b| b.new_folder.clone().unwrap_or_default()),
                    move |v| {
                        s.folder_browser.update(|b| {
                            b.new_folder = Some(v.to_owned());
                            b.error = None;
                        })
                    },
                    submit,
                    |_| {},
                    move || s.folder_browser.with(|b| b.error.is_some()),
                    ReturnKeyType::Done,
                    Some(field),
                ))
                .child(ui::field_note(move || s.folder_browser.with(|b| b.error.clone()), || true))
        },
        ui::dialog_actions(false).child(ui::dialog_action(move || s.label("Cancel"), ui::Act::Cancel).on_press(cancel)).child(
            ui::dialog_action(move || s.label("Create"), ui::Act::Default)
                .test_id("create-folder")
                .disabled(move || blank() || busy())
                .on_press(submit),
        ),
        cancel,
    )
}

fn insecure(s: Store) -> impl IntoElement {
    let cancel = move || s.cloud_form.update(|f| f.needs_consent = false);
    ui::dialog(
        move || s.label("Unencrypted connection"),
        move || {
            ui::dim(ui::body(text(move || s.label("Your password is sent over HTTP in plain text. Only continue on a network you trust."))))
        },
        ui::dialog_actions(true).child(ui::dialog_action(move || s.label("Cancel"), ui::Act::Cancel).on_press(cancel)).child(
            ui::dialog_action(move || s.label("Use Unencrypted Connection"), ui::Act::Destructive).on_press(move || {
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
        move || {
            view()
                .gap(px(8.0))
                .child(ui::dim(ui::body(text(move || {
                    s.label("Continue only if this fingerprint matches the one your server shows. Safay will trust only this certificate.")
                }))))
                .child(ui::label(text(move || s.label("SHA-256 fingerprint"))))
                .child(text(formatted).font_family(crate::components::CODE_FONT.family).text_size(px(12.0)).line_height(px(17.0)))
        },
        ui::dialog_actions(true)
            .child(ui::dialog_action(move || s.label("Cancel"), ui::Act::Cancel).on_press(move || s.decline_certificate()))
            .child(ui::dialog_action(move || s.label("Trust This Certificate"), ui::Act::Destructive).on_press(move || {
                if let (Some(provider), Some(f)) = (connecting(s), fingerprint()) {
                    s.trust_certificate(provider, f)
                }
            })),
        move || s.decline_certificate(),
    )
}
