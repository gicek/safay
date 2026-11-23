//! "Manage Categories": rename with a press, delete, move up or down, and make a new one.
use super::{Nav, ui};
use crate::{
    components::tok,
    icons::{Icon, icon},
    store::{CategoryEdit, Store},
};
use nsh_base::{button, theme};
use nsh_core::prelude::*;

pub fn page(s: Store, nav: Nav) -> impl IntoElement {
    let count = move || s.projection.with(|p| p.categories.len());
    view()
        .flex_1()
        .pt_safe_area()
        .child(ui::top_bar(s, Some("Manage Categories"), move || {
            nav.back();
        }))
        .child(
            scroll().flex_1().child(
                view()
                    .px(px(ui::MARGIN))
                    .pt(px(8.0))
                    .pb(px(120.0))
                    .child(view().gap(px(ui::GROUP_GAP)).child(each(
                        move || s.projection.with(|p| p.categories.clone()),
                        |c| c.id,
                        move |c, index| {
                            let id = c.with_untracked(|c| c.id);
                            ui::group_corners(
                                view().flex_row().items_center().overflow_hidden().bg(move || theme().card).min_h(px(72.0)),
                                move || index.get(),
                                count,
                            )
                            .child(
                                button()
                                    .test_id("category")
                                    .flex_1()
                                    .min_w(px(0.0))
                                    .pl(px(20.0))
                                    .py(px(12.0))
                                    .active(|st| st.opacity(0.7))
                                    .on_press(move || {
                                        let name = c.with(|c| c.name.clone());
                                        s.editing.set(Some(CategoryEdit { id: Some(id), name, ..Default::default() }))
                                    })
                                    .child(ui::body_large(text(move || c.with(|c| SharedString::from(c.name.clone())))).number_of_lines(1))
                                    .child(ui::dim(ui::body(text(move || s.count("accounts_n", c.with(|c| c.count)))))),
                            )
                            .child(when(
                                move || index.get() > 0,
                                move || {
                                    ui::icon_button(move || s.label("Move Up"), || Icon::ChevronDown)
                                        .rotate(|| std::f32::consts::PI)
                                        .on_press(move || s.move_category(id, -1))
                                },
                            ))
                            .child(when(
                                move || index.get() + 1 < count(),
                                move || {
                                    ui::icon_button(move || s.label("Move Down"), || Icon::ChevronDown)
                                        .on_press(move || s.move_category(id, 1))
                                },
                            ))
                            .child(
                                ui::icon_button(move || s.label("Delete Category"), || Icon::Trash)
                                    .mr(px(4.0))
                                    .on_press(move || s.delete_category(id)),
                            )
                        },
                    )))
                    .child(
                        ui::dim(ui::caption(text(move || {
                            s.label(if count() == 0 {
                                "No categories yet. Create one, then long-press an account and choose Add to."
                            } else {
                                "Tap a category to rename it. Deleting a category doesn’t delete its accounts."
                            })
                        })))
                        .p(px(16.0)),
                    ),
            ),
        )
        .child(
            view().absolute().right(px(ui::MARGIN)).bottom(px(0.0)).pb_safe_area_plus(px(ui::MARGIN)).child(
                button()
                    .test_id("new-category")
                    .h(px(56.0))
                    .pl(px(16.0))
                    .pr(px(20.0))
                    .flex_row()
                    .items_center()
                    .gap(px(12.0))
                    .rounded(px(16.0))
                    .bg(move || tok("primary-container"))
                    .text_color(move || tok("primary-container-foreground"))
                    .shadow_lg()
                    .active(|st| st.scale(0.97))
                    .on_press(move || s.editing.set(Some(CategoryEdit::default())))
                    .child(icon(Icon::FolderPlus).size(px(24.0)))
                    .child(ui::label(text(move || s.label("New Category")))),
            ),
        )
}
