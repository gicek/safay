//! What a long press on an account opens: its menu at the finger (its QR code, its details, its categories, deleting
//! it), and from there the sheet that puts it in categories.
use super::{Nav, ui};
use crate::{
    components::{badge_with_icon, tok},
    icons::{Icon, icon},
    store::{Auth, CategoryEdit, Sheet, Store},
};
use nsh_base::theme;
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

/// "Add to": the categories, the account's ticked. Nothing changes until "Save".
pub fn add_to(s: Store, nav: Nav) -> impl IntoElement {
    let staged = signal(BTreeSet::<Uuid>::new());
    let before = signal(BTreeSet::<Uuid>::new());
    // Opening stages what the account is in now.
    effect(move || {
        if let Some(id) = nav.add_to.get() {
            let now: BTreeSet<Uuid> = untracked(|| {
                s.projection
                    .with(|p| p.rows.iter().find(|r| r.id == id).map(|r| r.categories.iter().copied().collect()).unwrap_or_default())
            });
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
    let save = move || {
        if let Some(id) = nav.add_to.get() {
            let (was, now) = (before.get(), staged.get());
            let added = now.difference(&was).map(|c| (*c, true));
            let removed = was.difference(&now).map(|c| (*c, false));
            s.set_memberships(id, added.chain(removed).collect());
        }
        nav.add_to.set(None)
    };
    ui::bottom_sheet(
        move || nav.add_to.get().is_some(),
        move || nav.add_to.set(None),
        view()
            .pb_safe_area_plus(px(8.0))
            .child(
                view()
                    .flex_row()
                    .items_center()
                    .px(px(12.0))
                    .child(
                        ui::text_button(move || s.label("New Category"), false)
                            .on_press(move || s.editing.set(Some(CategoryEdit { account: nav.add_to.get(), ..Default::default() }))),
                    )
                    .child(view().flex_1())
                    .child(ui::text_button(move || s.label("Save"), false).test_id("save").on_press(save)),
            )
            .child(scroll().max_h(px(360.0)).child(view().py(px(8.0)).child(each(
                move || s.projection.with(|p| p.categories.clone()),
                |c| c.id,
                move |c, _| {
                    let id = c.with_untracked(|c| c.id);
                    let on = move || staged.with(|st| st.contains(&id));
                    ui::toggle(AccessibilityRole::Checkbox, move || c.with(|c| SharedString::from(c.name.clone())), on)
                        .min_h(move || px(ui::TOUCH) * font_scale())
                        .px(px(24.0))
                        .flex_row()
                        .items_center()
                        .gap(px(16.0))
                        .active(|st| st.bg(move || tok("pressed")))
                        .on_press(move |_: &()| {
                            staged.update(|st| {
                                if !st.remove(&id) {
                                    st.insert(id);
                                }
                            })
                        })
                        .child(view().text_color(move || theme().foreground).child(icon(Icon::Folder).size(px(24.0))))
                        .child(
                            ui::body_large(text(move || c.with(|c| SharedString::from(c.name.clone()))))
                                .number_of_lines(1)
                                .flex_1()
                                .text_color(move || theme().foreground),
                        )
                        .child(when(on, || view().text_color(move || theme().primary).child(icon(Icon::Check).size(px(24.0)))))
                },
            )))),
    )
}
