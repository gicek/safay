//! First run: restoring a backup from a cloud service, a folder or an exported file.
use super::*;

// ---- First run: restoring a backup -------------------------------------------------------------------------------------

/// One step back on the first run's restore page: from a backup found to where it is, else to the welcome page.
pub fn restore_back(s: Store) {
    if unlock::restore_found(s) { unlock::choose_restore_again(s) } else { onboarding::leave(s) }
}

/// "Restore a backup": where it is (a cloud service or an exported file), then its password.
pub fn restore_page(s: Store, scaffold: impl FnOnce(AnyElement, AnyElement) -> AnyElement) -> AnyElement {
    let use_as_master = s.use_export_password;
    use_as_master.set(true);
    let export = move || unlock::restore_from_export(s);
    let found = move || unlock::restore_found(s);
    let ready = move || unlock::restore_ready(s);
    let submit = move || {
        if ready() && !s.busy.get() {
            dismiss_keyboard();
            unlock::submit_restore(s)
        }
    };
    let back = move || {
        onboarding::back(s);
    };
    let problem = move || {
        s.error
            .get()
            .map(|e| s.label(crate::i18n::error_key(e)))
            .or_else(|| s.status.get().filter(|st| st.tone == crate::store::Tone::Error).map(|st| st.text))
    };
    scaffold(
        ui::top_bar(s, None, back),
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
                        .child(ui::form(move || {
                            ui::field(
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
                            )
                        }))
                        .child(when(export, move || {
                            view()
                                .gap(px(12.0))
                                .child(
                                    ui::form_card().child(
                                        ui::toggle(
                                            AccessibilityRole::Switch,
                                            move || s.label("Use as Master Password"),
                                            move || use_as_master.get(),
                                        )
                                        .flex_row()
                                        .items_center()
                                        .gap(px(16.0))
                                        .px(px(4.0))
                                        .on_press(move |_: &()| use_as_master.update(|v| *v = !*v))
                                        .child(view().text_color(move || theme().muted_foreground).child(icon(Icon::Key).size(px(28.0))))
                                        .child(ui::body_large(text(move || s.label("Use as Master Password"))).flex_1())
                                        .child(ui::switch(move || use_as_master.get())),
                                    ),
                                )
                                .child(when(
                                    move || !use_as_master.get(),
                                    move || onboarding::new_password_form(s, "Master Password", "Enter again", submit, element_ref()),
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
    // iOS: iCloud first, in a build signed with the iCloud entitlement.
    let icloud = (crate::platform::supports::ICLOUD && safay_app::icloud::available()).then(|| {
        source_row(LocationIcon::Icloud, "iCloud".into())
            .test_id("iCloud")
            .on_press(move || {
                s.error.set(None);
                s.status.set(None);
                s.choose_restore_icloud()
            })
            .into_element()
    });
    let mut rows: Vec<AnyElement> = icloud.into_iter().collect();
    rows.extend(providers().into_iter().map(|provider| {
        source_row(LocationIcon::Cloud(provider), provider.name().into())
            .test_id(provider.name())
            .on_press(move || {
                s.error.set(None);
                s.status.set(None);
                open_setup(s, provider, true)
            })
            .into_element()
    }));
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
        .min_h(move || px(ui::ROW) * font_scale())
        .px(px(16.0))
        .flex_row()
        .items_center()
        .gap(px(16.0))
        .bg(move || theme().card)
        .active(|st| st.bg(move || tok("surface-high")))
        .child(plate(kind))
        .child(ui::body_large(text(title)).flex_1())
        .child(ui::chevron())
}

/// Whether first run is on its restore step.
pub fn restoring(s: Store) -> bool {
    s.stage.get() == Stage::Restore
}
