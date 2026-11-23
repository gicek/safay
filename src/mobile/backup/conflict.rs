//! "Resolve Conflict": what differs between this device and the backup, then which copy to keep.
use super::*;

// ---- Resolving a conflict ----------------------------------------------------------------------------------------------

/// "Resolve Conflict": what differs on each side, then which copy to keep.
pub fn conflict(s: Store, nav: Nav) -> impl IntoElement {
    let review = move || s.review.get();
    let choice = move || s.resolution.get();
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
                move || {
                    // Comparing, or what went wrong with a way to try again.
                    let failed =
                        move || s.status.get().filter(|st| st.tone == crate::store::Tone::Error && !s.syncing.get()).map(|st| st.text);
                    view().flex_1().items_center().justify_center().px(px(32.0)).gap(px(16.0)).child(
                        when(move || failed().is_none(), || crate::components::spinner(28.0)).otherwise(move || {
                            view()
                                .items_center()
                                .gap(px(16.0))
                                .child(ui::dim(ui::body(text(move || failed().unwrap_or_default()))).text_center())
                                .child(ui::outlined_button(move || s.label("Try Again")).on_press(move || {
                                    s.status.set(None);
                                    s.load_review();
                                }))
                        }),
                    )
                },
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
    let on = move || s.resolution.get() == value;
    ui::toggle(AccessibilityRole::Radio, move || s.label(title), on)
        .test_id(title)
        .flex_row()
        .items_center()
        .gap(px(16.0))
        .px(px(16.0))
        .py(px(12.0))
        .bg(move || theme().card)
        .active(|st| st.bg(move || tok("surface-high")))
        .on_press(move |_: &()| s.resolution.set(value))
        .child(ui::radio_mark(on))
        .child(
            view().flex_1().child(ui::body_large(text(move || s.label(title)))).child(ui::dim(ui::caption(text(move || s.label(detail))))),
        )
}
