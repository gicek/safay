//! The dialogs every page may raise: the master password asked again, an account's QR code, a category's name.
use super::ui::{self, Entry};
use crate::store::{Sheet, Store};
use nsh_core::prelude::*;
use safay_app::Zeroizing;

pub fn all(s: Store) -> impl IntoElement {
    view()
        .child(when(move || s.editing.with(Option::is_some), move || category_name(s)))
        .child(when(move || matches!(s.sheet.get(), Some(Sheet::Qr(_))) && !s.qr.with(|q| q.is_empty()), move || qr(s)))
        .child(when(move || s.auth.get().is_some() && !s.auth_prompting.get(), move || auth(s)))
}

/// The master password, before a secret shows or the vault is exported or erased.
fn auth(s: Store) -> impl IntoElement {
    let submit = move || {
        if !s.auth_password.with(|p| p.is_empty()) && !s.busy.get() {
            s.authorize()
        }
    };
    ui::dialog(
        move || s.label("Verify your identity"),
        move || {
            view()
                .gap(px(8.0))
                .child(ui::message(ui::body(text(move || s.label("Enter your master password to continue.")))))
                .child(ui::field(
                    s,
                    "auth-password",
                    Entry::Password,
                    move || s.label("Master Password"),
                    move || s.auth_password.with(|p| p.to_string()),
                    move |v| {
                        s.auth_error.set(None);
                        s.auth_password.set(Zeroizing::new(v.to_owned()))
                    },
                    submit,
                    |_| {},
                    move || s.auth_error.get().is_some(),
                    ReturnKeyType::Done,
                    None,
                ))
                .child(ui::field_note(move || s.auth_error.get().map(|e| s.label(crate::i18n::error_key(e))), || true))
        },
        ui::dialog_actions(false)
            .child(ui::dialog_action(move || s.label("Cancel"), ui::Act::Cancel).on_press(move || s.cancel_auth()))
            .child(ui::dialog_action(move || s.label("Continue"), ui::Act::Default).test_id("authorize").on_press(submit)),
        move || {
            if !s.busy.get() {
                s.cancel_auth()
            }
        },
    )
}

/// An account as a QR code, for another device to scan.
fn qr(s: Store) -> impl IntoElement {
    ui::dialog(
        move || s.label("Export QR Code"),
        move || {
            view()
                .items_center()
                .gap(px(12.0))
                .child(view().p(px(8.0)).rounded(px(12.0)).bg(white()).child(image(move || s.qr.with(|v| v.to_string())).size(px(240.0))))
                .child(ui::title(text(move || {
                    s.detail.with(|d| d.as_ref().map(|d| SharedString::from(d.issuer.clone())).unwrap_or_default())
                })))
                .child(
                    ui::dim(ui::caption(text(move || {
                        s.label("Anyone who scans this code can generate your codes. Scan it only on devices you trust.")
                    })))
                    .text_center(),
                )
        },
        ui::dialog_actions(false)
            .child(ui::dialog_action(move || s.label("Done"), ui::Act::Default).test_id("qr-done").on_press(move || s.close_sheet())),
        move || s.close_sheet(),
    )
}

/// A new category's name, or another name for one there is. iOS: the system's alert with a text field; a name it
/// refuses (one taken) asks again, saying why.
fn category_name(s: Store) -> AnyElement {
    if ui::IOS {
        system_category_name(s);
        return view().into_element();
    }
    drawn_category_name(s).into_element()
}
fn system_category_name(s: Store) {
    let shown = stored(0_u64);
    // A name is being saved: no alert until it is refused (an error) or the edit ends.
    let saving = stored(false);
    // Each time the edit asks anew (opened, or refused with an error), one alert.
    effect(move || {
        let Some(edit) = s.editing.get() else { return };
        if saving.get() {
            if edit.error.is_none() {
                return;
            }
            saving.replace(false);
        }
        let ask = shown.get() + 1;
        shown.replace(ask);
        untracked(move || {
            let renaming = edit.id.is_some();
            let mut alert = nsh_base::alert_dialog()
                .title(s.label(if renaming { "Rename" } else { "New Category" }))
                .input(
                    nsh_base::AlertDialogInput::new()
                        .default_value(edit.name.clone())
                        .placeholder(s.label("Category name"))
                        .select_text_on_focus(renaming)
                        .required(true),
                )
                .cancel("cancel", s.label("Cancel"))
                .action("commit", s.label(if renaming { "Done" } else { "Create" }));
            if let Some(error) = edit.error {
                alert = alert.description(s.label(crate::i18n::error_key(error)));
            }
            let answer = alert.show();
            s.task(async move {
                let reply = answer.await.ok();
                if shown.get() != ask {
                    return;
                }
                match reply.and_then(|r| if r.action.as_deref() == Some("commit") { r.value } else { None }) {
                    Some(name) => {
                        saving.replace(true);
                        s.editing.update(|e| {
                            if let Some(e) = e {
                                e.name = name.to_string();
                                e.error = None;
                            }
                        });
                        s.commit_category();
                    }
                    None => s.editing.set(None),
                }
            });
        });
    });
}
fn drawn_category_name(s: Store) -> impl IntoElement {
    let renaming = move || s.editing.with(|e| e.as_ref().is_some_and(|e| e.id.is_some()));
    let blank = move || s.editing.with(|e| e.as_ref().is_none_or(|e| e.name.trim().is_empty()));
    let submit = move || {
        if !blank() {
            s.commit_category()
        }
    };
    let cancel = move || {
        dismiss_keyboard();
        s.editing.set(None)
    };
    // Its one field takes the keyboard at once, as the system's alert with a text field does.
    let field = element_ref();
    request_animation_frame(move |_| field.focus());
    ui::dialog(
        move || s.label(if renaming() { "Rename" } else { "New Category" }),
        move || {
            view()
                .gap(px(8.0))
                .child(ui::field(
                    s,
                    "category-name",
                    Entry::Text,
                    move || s.label("Category name"),
                    move || s.editing.with(|e| e.as_ref().map(|e| e.name.clone()).unwrap_or_default()),
                    move |v| {
                        s.editing.update(|e| {
                            if let Some(e) = e {
                                e.name = v.to_owned();
                                e.error = None;
                            }
                        })
                    },
                    submit,
                    |_| {},
                    move || s.editing.with(|e| e.as_ref().is_some_and(|e| e.error.is_some())),
                    ReturnKeyType::Done,
                    Some(field),
                ))
                .child(ui::field_note(
                    move || s.editing.with(|e| e.as_ref().and_then(|e| e.error)).map(|e| s.label(crate::i18n::error_key(e))),
                    || true,
                ))
        },
        ui::dialog_actions(false).child(ui::dialog_action(move || s.label("Cancel"), ui::Act::Cancel).on_press(cancel)).child(
            ui::dialog_action(move || s.label(if renaming() { "Done" } else { "Create" }), ui::Act::Default)
                .test_id("commit")
                .on_press(submit),
        ),
        cancel,
    )
}

// ---- Settings ------------------------------------------------------------------------------------------------------

/// The dialogs settings and the add menu raise.
pub fn of(s: Store, nav: super::Nav) -> impl IntoElement {
    use super::Dialog;
    let is = move |d: Dialog| nav.dialog.get() == Some(d);
    view()
        .child(when(move || is(Dialog::AutoLock), move || auto_lock(s, nav)))
        .child(when(move || is(Dialog::Language), move || language(s, nav)))
        .child(when(move || is(Dialog::BackupPassword), move || backup_password(s, nav)))
}

/// One of a few values, picked and applied at once.
fn options<T: Copy + PartialEq + 'static>(
    s: Store,
    nav: super::Nav,
    title: &'static str,
    message: Option<&'static str>,
    values: Vec<(T, SharedString)>,
    current: impl Fn() -> T + Copy + 'static,
    pick: impl Fn(T) + Copy + 'static,
) -> impl IntoElement {
    use crate::components::tok;
    let close = move || nav.dialog.set(None);
    ui::dialog(
        move || s.label(title),
        move || {
            view()
                .mx(px(-24.0))
                .child(when(
                    move || message.is_some(),
                    move || ui::message(ui::body(text(move || s.label(message.unwrap_or_default())))).px(px(24.0)).pb(px(8.0)),
                ))
                .children(values.into_iter().map(move |(value, name)| {
                    let on = move || current() == value;
                    let said = name.clone();
                    ui::toggle(AccessibilityRole::Radio, move || said.clone(), on)
                        .min_h(move || px(ui::TOUCH) * font_scale())
                        .px(px(24.0))
                        .flex_row()
                        .items_center()
                        .gap(px(24.0))
                        .active(|st| st.bg(move || tok("pressed")))
                        .on_press(move |_: &()| {
                            pick(value);
                            close()
                        })
                        .child(ui::radio_mark(on))
                        .child(ui::body_large(text(name)))
                }))
        },
        ui::dialog_actions(false).child(ui::dialog_action(move || s.label("Cancel"), ui::Act::Cancel).on_press(close)),
        close,
    )
}
fn auto_lock(s: Store, nav: super::Nav) -> impl IntoElement {
    options(
        s,
        nav,
        "Auto-Lock",
        Some("Locks after the selected time without activity or in the background, and immediately when the device locks"),
        super::settings::AUTO_LOCK_MINUTES.iter().map(|m| (*m, s.count("minutes", *m as usize))).collect(),
        move || s.preferences.get().auto_lock_minutes,
        move |m| s.update_preferences(move |p| p.auto_lock_minutes = m),
    )
}
fn language(s: Store, nav: super::Nav) -> impl IntoElement {
    use safay_app::preferences::Language;
    options(
        s,
        nav,
        "Language",
        None,
        [Language::System, Language::En, Language::Hans, Language::Hant]
            .into_iter()
            .map(|l| (l, super::settings::language_name(s, l)))
            .collect(),
        move || s.preferences.get().language,
        move |l| s.update_preferences(move |p| p.language = l),
    )
}

/// The password of a backup file chosen for import.
fn backup_password(s: Store, nav: super::Nav) -> impl IntoElement {
    let close = move || {
        s.remove_file();
        nav.dialog.set(None)
    };
    let submit = move || {
        if s.form.with(|f| !f.password.is_empty()) && !s.busy.get() {
            dismiss_keyboard();
            s.import_file()
        }
    };
    super::shows_errors(s);
    // Its one field takes the keyboard at once, from the first frame it is up.
    let field = element_ref();
    request_animation_frame(move |_| field.focus());
    ui::dialog(
        move || s.label("Backup Password"),
        move || {
            view()
                .gap(px(12.0))
                .child(ui::message(ui::body(text(move || s.label("Enter the password that was used to export this backup.")))))
                .child(ui::field(
                    s,
                    "file-password",
                    Entry::Password,
                    move || s.label("Enter password"),
                    move || s.form.with(|f| f.password.to_string()),
                    move |v| {
                        s.error.set(None);
                        s.form.update(|f| f.password = Zeroizing::new(v.to_owned()))
                    },
                    submit,
                    |_| {},
                    move || s.error.get().is_some(),
                    ReturnKeyType::Done,
                    Some(field),
                ))
                .child(ui::field_note(move || s.error.get().map(|e| s.label(crate::i18n::error_key(e))), || true))
        },
        ui::dialog_actions(false)
            .child(ui::dialog_action(move || s.label("Cancel"), ui::Act::Cancel).on_press(close))
            .child(ui::dialog_action(move || s.label("Import"), ui::Act::Default).test_id("import-go").on_press(submit)),
        close,
    )
}
