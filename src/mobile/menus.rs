//! What a long press on an account opens: its menu at the finger (its QR code, its details, its categories, deleting
//! it), and from there the sheet that puts it in categories.
use super::{Nav, ui};
use crate::{
    components::{badge_with_icon, tok},
    icons::{Icon, icon},
    store::{Auth, CategoryEdit, Sheet, Store},
};
use nsh_base::{button, theme};
use nsh_core::prelude::*;
use safay_app::Uuid;
use std::collections::BTreeSet;

/// The menu's width, and the room it keeps from the screen's edges.
const WIDTH: f32 = 212.0;
const EDGE: f32 = 8.0;

/// The account's menu, a corner at the place of the press.
pub fn account(s: Store, nav: Nav) -> impl IntoElement {
    let close = move || nav.context.set(None);
    when(
        move || nav.context.get().is_some(),
        move || {
            let Some((id, at)) = nav.context.get_untracked() else { return view().into_element() };
            let Some(row) = s.projection.with_untracked(|p| p.rows.iter().find(|r| r.id == id).cloned()) else {
                return view().into_element();
            };
            let category = s.category.get_untracked();
            let category_name =
                category.and_then(|c| s.projection.with_untracked(|p| p.categories.iter().find(|x| x.id == c).map(|x| x.name.clone())));
            let size = window();
            let (inset, scale) = (safe_area(), font_scale());
            // Below and right of the finger; left of it or above it where that does not fit, clear of the system's bars.
            // Its height: the account's names, a row for each action, the line before "Delete" and the padding.
            let rows = if category.is_some() { 5.0 } else { 4.0 };
            let height = (76.0 + ui::TOUCH * rows) * scale + 17.0;
            let (lowest, highest) = (size.height.0 - inset.bottom.0 - EDGE, inset.top.0 + EDGE);
            let left = if at.x.0 + EDGE + WIDTH + EDGE <= size.width.0 { at.x.0 + EDGE } else { (at.x.0 - EDGE - WIDTH).max(EDGE) };
            let top = if at.y.0 + EDGE + height <= lowest { at.y.0 + EDGE } else { (at.y.0 - EDGE - height).max(highest) };
            let shown = stored(row.clone());
            view()
                .absolute()
                .inset(px(0.0))
                .on_press(move |_: &()| close())
                .child(
                    ui::popover(view().absolute().left(px(left)).top(px(top)).w(px(WIDTH)))
                        .child(
                            view()
                                .flex_row()
                                .items_center()
                                .gap(px(12.0))
                                .px(px(16.0))
                                .pt(px(4.0))
                                .pb(px(8.0))
                                .child(badge_with_icon(
                                    move || shown.with(|r| r.issuer.clone()),
                                    || false,
                                    move || shown.with(|r| r.icon.clone()),
                                    36.0,
                                ))
                                .child(
                                    view()
                                        .flex_1()
                                        .min_w(px(0.0))
                                        .child(ui::label(text(row.issuer.clone())).number_of_lines(1))
                                        .child(ui::dim(ui::caption(text(row.username.clone()))).number_of_lines(1)),
                                ),
                        )
                        // A press on the row copies the code: the menu shows the QR code instead.
                        .child(item(s, Icon::Qr, "Show QR Code", false, move || {
                            close();
                            s.reveal(Auth::Qr(id))
                        }))
                        .child(item(s, Icon::Info, "Details", false, move || {
                            close();
                            s.open_sheet(Sheet::Detail(id))
                        }))
                        .child(item(s, Icon::Folder, "Add to", false, move || {
                            close();
                            nav.add_to.set(Some(id))
                        }))
                        .child(when(
                            move || category.is_some(),
                            move || {
                                let name = category_name.clone().unwrap_or_default();
                                ui::menu_item(Icon::FolderMinus, move || s.fmt("Remove from “{}”", &[&name]), false).on_press(move || {
                                    close();
                                    if let Some(category) = category {
                                        s.membership(category, id, false)
                                    }
                                })
                            },
                        ))
                        .child(view().h(px(1.0)).mx(px(16.0)).my(px(4.0)).bg(move || theme().border))
                        .child(item(s, Icon::Trash, "Delete", true, move || {
                            close();
                            s.delete_account(id)
                        })),
                )
                .into_element()
        },
    )
}
fn item(s: Store, glyph: Icon, name: &'static str, destructive: bool, press: impl Fn() + 'static) -> impl IntoElement {
    ui::menu_item(glyph, move || s.label(name), destructive).test_id(name).on_press(press)
}

/// What the "Add to" sheet shows around its list on Android, at the default text size: its bar (the title over the
/// account) and the room under the list.
const ADD_TO_CHROME: f32 = 28.0 + 16.0 + 8.0 + 8.0 + 4.0;
/// "Add to": the categories, the account's ticked. A tick or an untick takes effect at once; the sheet only closes.
pub fn add_to(s: Store, nav: Nav) -> impl IntoElement {
    // What shows ticked, and what was last sent to the store: a tick shows at once, and goes to the store as soon as it
    // is free (a change it is still saving would drop another sent meanwhile).
    let staged = signal(BTreeSet::<Uuid>::new());
    let before = signal(BTreeSet::<Uuid>::new());
    let account_id = stored(None::<Uuid>);
    let pending = stored(false);
    // Opening stages what the account is in now.
    effect(move || {
        if let Some(id) = nav.add_to.get() {
            let now: BTreeSet<Uuid> = untracked(|| {
                s.projection
                    .with(|p| p.rows.iter().find(|r| r.id == id).map(|r| r.categories.iter().copied().collect()).unwrap_or_default())
            });
            account_id.replace(Some(id));
            staged.set(now.clone());
            before.set(now);
        }
    });
    // A category made from here holds the account already: it shows ticked.
    effect(move || {
        let Some(id) = nav.add_to.get() else { return };
        let now: BTreeSet<Uuid> =
            s.projection.with(|p| p.rows.iter().find(|r| r.id == id).map(|r| r.categories.iter().copied().collect()).unwrap_or_default());
        untracked(|| {
            let new: Vec<Uuid> = now.difference(&before.get()).copied().collect();
            if !new.is_empty() {
                staged.update(|st| st.extend(new.iter().copied()));
                before.update(|b| b.extend(new));
            }
        });
    });
    let flush = move || {
        let Some(id) = account_id.get() else { return };
        if s.busy.get_untracked() {
            pending.replace(true);
            return;
        }
        pending.replace(false);
        let (was, now) = (before.get_untracked(), staged.get_untracked());
        let added = now.difference(&was).map(|c| (*c, true));
        let removed = was.difference(&now).map(|c| (*c, false));
        let changes: Vec<_> = added.chain(removed).collect();
        if !changes.is_empty() {
            before.set(now);
            s.set_memberships(id, changes);
        }
    };
    // A change made while the store was busy goes once it is free.
    effect(move || {
        if !s.busy.get() && pending.get() {
            untracked(flush);
        }
    });
    let close = move || nav.add_to.set(None);
    let new_category = move || s.editing.set(Some(CategoryEdit { account: nav.add_to.get(), ..Default::default() }));
    // The account, as its row says it: the service's name and the account's.
    let account = move || {
        let id = nav.add_to.get();
        s.projection
            .with(|p| {
                p.rows
                    .iter()
                    .find(|r| Some(r.id) == id)
                    .map(|r| if r.username.is_empty() { r.issuer.clone() } else { format!("{} · {}", r.issuer, r.username) })
            })
            .map(SharedString::from)
            .unwrap_or_default()
    };
    let none = move || s.projection.with(|p| p.categories.is_empty());
    ui::bottom_sheet(
        move || nav.add_to.get().is_some(),
        close,
        move || {
            view()
                // iOS: the system's sheet sizes itself, at half the screen or all of it; the list fills what its bar
                // leaves, and scrolls.
                .flex_grow(if ui::IOS { 1.0 } else { 0.0 })
                .min_h(px(0.0))
                .pb_safe_area_plus(px(8.0))
                .child(if ui::IOS {
                    // iOS: the title in the middle of the sheet, "Done" in glass at the end, as the system's sheets.
                    view()
                        .h(px(ui::TOUCH + 8.0))
                        .px(px(16.0))
                        .mb(px(8.0))
                        .flex_row()
                        .items_center()
                        .justify_end()
                        .child(
                            ui::title(text(move || s.label("Add to")))
                                .absolute()
                                .left(px(96.0))
                                .right(px(96.0))
                                .text_center()
                                .number_of_lines(1),
                        )
                        .child(ui::glass_text_button(move || s.label("Done")).test_id("done").on_press(close))
                        .into_element()
                } else {
                    // Android: the sheet's close button at the start, as the other sheets' (a full-screen dialog's),
                    // then the title over the account in small text.
                    view()
                        .flex_row()
                        .items_center()
                        .gap(px(4.0))
                        .pl(px(4.0))
                        .pr(px(24.0))
                        .pb(px(8.0))
                        .child(ui::icon_button(move || s.label("Close"), || Icon::Close).test_id("close").on_press(close))
                        .child(
                            view()
                                .flex_1()
                                .min_w(px(0.0))
                                .child(ui::title_large(text(move || s.label("Add to"))).number_of_lines(1))
                                .child(ui::dim(ui::caption(text(account))).number_of_lines(1)),
                        )
                        .into_element()
                })
                // iOS: which account, under the bar, as a group's header.
                .child(when(|| ui::IOS, move || ui::footer(account).px(px(ui::MARGIN + 16.0)).pb(px(7.0))))
                // iOS: the categories as one group of rows, as the system's checklists; the last row makes a new one.
                // The list scrolls once the categories are more than the sheet holds, "New Category" at its end. Android:
                // as tall as the room the sheet has (`sheet_room`), less its bar.
                .child(
                    scroll()
                        .max_h(move || ui::sheet_room(ADD_TO_CHROME))
                        .flex_grow(if ui::IOS { 1.0 } else { 0.0 })
                        .flex_shrink(1.0)
                        .min_h(px(0.0))
                        .child(
                            view()
                                .py(px(if ui::IOS { 0.0 } else { 4.0 }))
                                .mx(px(if ui::IOS { ui::MARGIN } else { 0.0 }))
                                .mb(px(if ui::IOS { 16.0 } else { 0.0 }))
                                .rounded(px(if ui::IOS { ui::GROUP_RADIUS } else { 0.0 }))
                                .overflow_hidden()
                                .bg(move || if ui::IOS { tok("sheet-card") } else { theme().card.opacity(0.0) })
                                .child(each(
                                    move || s.projection.with(|p| p.categories.clone()),
                                    |c| c.id,
                                    move |c, index| {
                                        let id = c.with_untracked(|c| c.id);
                                        let on = move || staged.with(|st| st.contains(&id));
                                        ui::toggle(AccessibilityRole::Checkbox, move || c.with(|c| SharedString::from(c.name.clone())), on)
                                            .min_h(move || px(if ui::IOS { ui::TOUCH } else { 56.0 }) * font_scale())
                                            .px(px(if ui::IOS { 16.0 } else { 24.0 }))
                                            .flex_row()
                                            .items_center()
                                            .gap(px(16.0))
                                            .active(|st| st.bg(move || tok("pressed")))
                                            .on_press(move |_: &()| {
                                                haptic(Haptic::Selection);
                                                staged.update(|st| {
                                                    if !st.remove(&id) {
                                                        st.insert(id);
                                                    }
                                                });
                                                flush()
                                            })
                                            // Android: Material's checkbox at the start; iOS: a folder, the checkmark at the end.
                                            .child(if ui::IOS {
                                                view()
                                                    .text_color(move || theme().foreground)
                                                    .child(icon(Icon::Folder).size(px(24.0)))
                                                    .into_element()
                                            } else {
                                                checkbox_mark(on).into_element()
                                            })
                                            .child(
                                                ui::body_large(text(move || c.with(|c| SharedString::from(c.name.clone()))))
                                                    .number_of_lines(1)
                                                    .flex_1()
                                                    .text_color(move || theme().foreground),
                                            )
                                            .child(when(
                                                move || ui::IOS && on(),
                                                || view().text_color(move || theme().primary).child(icon(Icon::Check).size(px(24.0))),
                                            ))
                                            .child(when(move || ui::IOS && index.get() > 0, || ui::separator(16.0 + 24.0 + 16.0)))
                                    },
                                ))
                                // "New Category", a row of the list, after the categories.
                                .child(
                                    button()
                                        .test_id("new-category")
                                        .accessibility_label(move || Some(s.label("New Category")))
                                        .min_h(move || px(if ui::IOS { ui::TOUCH } else { 56.0 }) * font_scale())
                                        .px(px(if ui::IOS { 16.0 } else { 24.0 }))
                                        .flex_row()
                                        .items_center()
                                        .gap(px(16.0))
                                        .active(|st| st.bg(move || tok("pressed")))
                                        .on_press(new_category)
                                        .child(view().text_color(move || theme().primary).child(icon(Icon::FolderPlus).size(px(24.0))))
                                        .child(
                                            ui::body_large(text(move || s.label("New Category")))
                                                .flex_1()
                                                .text_color(move || theme().primary),
                                        )
                                        .child(when(move || ui::IOS && !none(), || ui::separator(16.0 + 24.0 + 16.0))),
                                ),
                        ),
                )
                // No category yet: what one is for.
                .child(when(none, move || {
                    ui::footer(move || s.label("Categories group your accounts. Create one, and this account goes in it."))
                        .px(px(if ui::IOS { ui::MARGIN + 16.0 } else { 24.0 }))
                        .pb(px(8.0))
                }))
                .into_element()
        },
    )
}

/// Material's checkbox: an 18 pt box with 2 pt corners, its outline the secondary text color; checked, filled with the
/// primary color and a check mark, the change eased.
fn checkbox_mark(on: impl Fn() -> bool + Copy + 'static) -> impl IntoElement {
    view().size(px(24.0)).items_center().justify_center().child(
        view()
            .size(px(18.0))
            .rounded(px(3.0))
            .border(px(2.0))
            .border_color(move || if on() { theme().primary } else { theme().muted_foreground })
            .bg(move || if on() { theme().primary } else { theme().card.opacity(0.0) })
            .transition()
            .duration(ui::EFFECTS_MS)
            .ease_out()
            .items_center()
            .justify_center()
            .text_color(move || theme().primary_foreground)
            .child(
                view()
                    .opacity(move || if on() { 1.0_f32 } else { 0.0 })
                    .scale(move || if on() { 1.0_f32 } else { 0.6 })
                    .transition()
                    .duration(ui::EFFECTS_MS)
                    .ease_out()
                    .child(crate::icons::icon_bold(Icon::Check).size(px(16.0))),
            ),
    )
}
