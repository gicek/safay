//! Where the backup is kept: choosing a location, and restoring from a backup this vault cannot open.
use super::*;

// ---- Choosing a location, restoring from an unreadable backup --------------------------------------------------------

/// The dialogs of the card: where to keep the backup, and the master password of a backup this vault cannot open.
pub fn dialogs(s: Store, nav: Nav) -> impl IntoElement {
    view().child(when(move || nav.restoring.get(), move || restore_unreadable(s, nav)))
}

/// "Choose Location": a page of the services, the one in use checked. A service's setup opens over it, so its back
/// comes back here; once a location is set up, the page goes too.
pub fn location(s: Store, nav: Nav) -> impl IntoElement {
    let current = move || match s.projection.with(|p| p.sync.location.clone()) {
        Some(Location::Cloud { provider, .. }) => Some(provider),
        _ => None,
    };
    let was = s.projection.with_untracked(|p| p.sync.location.clone());
    effect(move || {
        let now = s.projection.with(|p| p.sync.location.clone());
        if now.is_some() && now != was {
            untracked(|| nav.pop());
        }
    });
    let rows: Vec<AnyElement> = providers()
        .into_iter()
        .map(move |provider| {
            button()
                .test_id(provider.name())
                .accessibility_label(move || Some(SharedString::from(provider.name())))
                .min_h(move || px(ui::ROW) * font_scale())
                .px(px(16.0))
                .flex_row()
                .items_center()
                .gap(px(16.0))
                .bg(move || theme().card)
                .active(|st| st.bg(move || if ui::IOS { tok("pressed") } else { tok("surface-high") }))
                .on_press(move || open_setup(s, provider, false))
                .child(plate(LocationIcon::Cloud(provider)))
                .child(ui::body_large(text(provider.name())).flex_1())
                .child(
                    when(
                        move || current() == Some(provider),
                        || view().text_color(move || theme().primary).child(icon(Icon::Check).size(px(24.0))),
                    )
                    .otherwise(ui::chevron),
                )
                .into_element()
        })
        .collect();
    view().flex_1().pt(ui::page_top).child(ui::top_bar(s, Some("Choose Location"), move || nav.pop())).child(
        scroll().flex_1().child(
            view().items_center().pb_safe_area_plus(px(24.0)).child(
                view()
                    .w_full()
                    .max_w(px(480.0))
                    .px(px(ui::MARGIN))
                    .pt(px(8.0))
                    .child(
                        ui::dim(ui::body(text(move || {
                            s.label("Choose where to keep the encrypted backup. Only one location is used at a time.")
                        })))
                        .px(px(16.0))
                        .pb(px(12.0)),
                    )
                    .child(ui::group(rows)),
            ),
        ),
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
        move || {
            view()
                .gap(px(8.0))
                .child(ui::message(ui::body(text(move || {
                    s.label(
                        "To use that backup, restore from it and unlock with its master password. This replaces the vault on this device.",
                    )
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
                .child(ui::field_note(move || s.status.get().map(|st| st.text), || true))
        },
        ui::dialog_actions(false).child(ui::dialog_action(move || s.label("Cancel"), ui::Act::Cancel).on_press(close)).child(
            ui::dialog_action(move || s.label("Restore"), ui::Act::Default)
                .test_id("restore-go")
                .disabled(move || s.syncing.get())
                .on_press(submit),
        ),
        close,
    )
}
