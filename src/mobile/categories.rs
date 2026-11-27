//! "Manage Categories": rename with a press, reorder by dragging after a long press (screen readers: the "Move Up" and
//! "Move Down" actions), delete, and make a new one.
use super::{Nav, ui};
use crate::{
    icons::{Icon, icon},
    reorder::{CATEGORY_GAP, Kind, Pickup, Reorder},
    store::{CategoryEdit, Store},
};
use nsh_base::theme;
use nsh_core::{AccessibilityActionDef, prelude::*};
use safay_app::session::CategoryRow;

pub fn page(s: Store, nav: Nav) -> impl IntoElement {
    let r = Reorder::new(s, Kind::Categories, |_| (if ui::IOS { 0.0 } else { CATEGORY_GAP }, 1), Pickup::LongPress);
    let count = move || r.count();
    // The edit mode is this page's: it ends as the page goes, or once nothing is left to delete.
    nav.editing.set(false);
    cleanup(move || nav.editing.set(false));
    effect(move || {
        if count() == 0 {
            untracked(|| nav.editing.set(false));
        }
    });
    view()
        .flex_1()
        .pt(ui::page_top)
        .child(ui::top_bar(s, Some("Manage Categories"), move || {
            nav.pop();
        }))
        // None yet: centered on the page, as the other empty states.
        .child(when(
            move || count() == 0,
            move || {
                view()
                    .flex_1()
                    .items_center()
                    .justify_center()
                    .px(px(32.0))
                    .pb_safe_area_plus(px(48.0))
                    .gap(px(8.0))
                    .child(view().text_color(move || theme().muted_foreground).child(icon(Icon::Folder).size(px(48.0))))
                    .child(ui::title(text(move || s.label("No categories yet"))).text_center().mt(px(8.0)))
                    .child(
                        ui::dim(ui::body(text(move || s.label("Create one, then long-press an account and choose Add to.")))).text_center(),
                    )
            },
        ))
        .child(when(move || count() > 0, move ||
            scroll().flex_1().scrollable(move || r.scrollable()).child(
                view()
                    .px(px(ui::MARGIN))
                    .pt(px(8.0))
                    .pb(px(ui::FAB_CLEARANCE))
                    .child(
                        view()
                            .gap(px(if ui::IOS { 0.0 } else { CATEGORY_GAP }))
                            .element_ref(&r.list)
                            .child(each(
                                move || r.categories(),
                                |c| c.id,
                                move |c, index| category_row(s, nav, r, c, index),
                            ))
                            .child(when(move || r.pointer.get().is_some() && r.dragged.get().is_some(), move || standin(s, r))),
                    )
                    .child(
                        ui::dim(ui::caption(text(move || {
                            s.label(if ui::IOS {
                                "Tap a category to rename it, or touch and hold it to drag it to another place. Tap Edit to delete one; its accounts stay."
                            } else {
                                "Tap a category to rename it, or touch and hold it to drag it to another place. Deleting a category doesn’t delete its accounts."
                            })
                        })))
                        .p(px(16.0)),
                    ),
            ),
        ))
        // iOS: "+" in the system's header instead (`mobile::native_stack`).
        .child(when(|| !ui::IOS, move ||
            view().absolute().right(px(ui::MARGIN)).bottom(px(0.0)).pb_safe_area_plus(px(ui::MARGIN)).child(
                ui::fab(move || s.label("New Category"))
                    .test_id("new-category")
                    .on_press(move || s.editing.set(Some(CategoryEdit::default())))
                    .child(icon(Icon::Plus).size(px(24.0))),
            ),
        ))
}

/// A category's row: a press renames it, a long press picks it up to drag, its button deletes it.
fn category_row(s: Store, nav: Nav, r: Reorder, c: ReadSignal<CategoryRow>, index: ReadSignal<usize>) -> impl IntoElement {
    let id = c.with_untracked(|c| c.id);
    let me = element_ref();
    let el = ui::group_corners(
        view()
            .flex_row()
            .items_center()
            .overflow_hidden()
            .bg(move || theme().card)
            .min_h(move || px(ui::ROW) * font_scale())
            // Its place stays empty while it is dragged.
            .opacity(move || if r.is_dragging(Some(id)) { 0.0_f32 } else { 1.0 }),
        move || index.get(),
        move || r.count(),
    );
    r.item(el, me, move || Some(id))
        // iOS, in its edit mode: the system's red delete control at the row's start.
        .child(when(
            move || ui::IOS && nav.editing.get(),
            move || {
                nsh_base::button()
                    .test_id("delete-category")
                    .accessibility_label(move || Some(s.label("Delete Category")))
                    .size(px(ui::TOUCH))
                    .ml(px(4.0))
                    .mr(px(-12.0))
                    .items_center()
                    .justify_center()
                    .on_press(move || s.delete_category(id))
                    .child(
                        view()
                            .size(px(22.0))
                            .rounded_full()
                            .bg(move || theme().destructive)
                            .items_center()
                            .justify_center()
                            .child(view().w(px(11.0)).h(px(2.0)).rounded_full().bg(|| AdaptiveColor::rgb(0xffffff, 0xffffff))),
                    )
            },
        ))
        .child(
            // A view, not a button: it takes a press (rename) and a long press (pick up).
            view()
                .accessible(true)
                .role(AccessibilityRole::Button)
                .label(move || c.with(|c| SharedString::from(format!("{}, {}", c.name, s.count("accounts_n", c.count)))))
                .test_id("category")
                .flex_1()
                .min_w(px(0.0))
                .pl(px(20.0))
                .py(px(12.0))
                .active(|st| st.opacity(0.7))
                .on_press(move |_: &()| {
                    let name = c.with(|c| c.name.clone());
                    s.editing.set(Some(CategoryEdit { id: Some(id), name, ..Default::default() }))
                })
                .on_long_press(move |_: &PressEvent| {
                    haptic(Haptic::Impact(HapticImpact::Medium));
                    r.arm(id)
                })
                // Screen readers reorder from the row's actions instead of dragging.
                .accessibility_actions(vec![
                    AccessibilityActionDef { name: "move-up".into(), label: Some(s.label("Move Up").as_ref().into()) },
                    AccessibilityActionDef { name: "move-down".into(), label: Some(s.label("Move Down").as_ref().into()) },
                ])
                .on_accessibility_action(move |action: &str| match action {
                    "move-up" => s.move_category(id, -1),
                    "move-down" => s.move_category(id, 1),
                    _ => {}
                })
                .child(names(s, move || Some(c.get()))),
        )
        // Android: its delete button, always there. iOS: the row's chevron (a press renames it).
        .child(if ui::IOS {
            view().pr(px(16.0)).child(ui::chevron()).into_element()
        } else {
            ui::icon_button(move || s.label("Delete Category"), || Icon::Trash)
                .mr(px(4.0))
                .on_press(move || s.delete_category(id))
                .into_element()
        })
}

/// A category's name over how many accounts it has.
fn names(s: Store, c: impl Fn() -> Option<CategoryRow> + Copy + 'static) -> impl IntoElement {
    view()
        .child(ui::body_large(text(move || SharedString::from(c().map(|c| c.name).unwrap_or_default()))).number_of_lines(1))
        .child(ui::dim(ui::body(text(move || s.count("accounts_n", c().map_or(0, |c| c.count))))))
}

/// The dragged category following the finger: its row's look, lifted by a shadow.
fn standin(s: Store, r: Reorder) -> impl IntoElement {
    let frame = move || r.pointer.get().unwrap_or_default();
    let category = move || {
        let id = r.dragged.get();
        s.projection.with(|p| p.categories.iter().find(|c| Some(c.id) == id).cloned())
    };
    ui::lifted(view())
        .absolute()
        .left(move || px(frame().0))
        .top(move || px(frame().1))
        .w(move || px(frame().2))
        .h(move || px(frame().3))
        .justify_center()
        .pl(px(20.0))
        .pointer_events(nsh_core::PointerEvents::None)
        .child(names(s, category))
}
