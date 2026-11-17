//! Recently Deleted: the accounts the user deleted, most recent first, each with the days left before it goes for good
//! (`safay_core::trash`). No codes: a row recovers or deletes, from its menu or its "Recover" button; "Delete All" empties
//! the list. The sidebar shows it only while it holds something.
use crate::{components::*, store::Store};
use nsh_base::theme;
use nsh_core::{element::desktop::MenuItem, prelude::*};
use safay_app::session::DeletedRow;

const ROW_H: f32 = 58.0;

pub fn page(s: Store) -> impl IntoElement {
    let rows = move || s.projection.with(|p| p.deleted.clone());
    view()
        .flex_1()
        .child(
            view()
                .flex_row()
                .items_center()
                .gap(px(12.0))
                .px(px(24.0))
                .pt(px(8.0))
                .pb(px(10.0))
                .child(note(move || s.label("Accounts are deleted for good 30 days after you delete them.")).flex_1().min_w(px(0.0)))
                .child(push_button(move || s.label("Delete All"), Kind::Standard, false).on_press(move || s.remove_deleted(None))),
        )
        .child(view().px(px(16.0)).child(divider()))
        .child(
            when(
                move || s.projection.with(|p| p.deleted.is_empty()),
                move || view().flex_1().items_center().justify_center().pb(px(42.0)).child(note(move || s.label("No matching accounts"))),
            )
            .otherwise(move || {
                scroll().flex_1().child(view().px(px(8.0)).py(px(4.0)).child(each(rows, |r| r.id, move |row, _| deleted_row(s, row))))
            }),
        )
}

fn deleted_row(s: Store, row: ReadSignal<DeletedRow>) -> impl IntoElement {
    let hovered = signal(false);
    let id = move || row.with(|r| r.id);
    let menu = move |at| {
        show_menu(
            s,
            vec![
                MenuItem::new(s.label("Recover").to_string()).on_select(move || s.recover_account(id())),
                MenuItem::separator(),
                MenuItem::new(s.label("Delete Now").to_string()).destructive(true).on_select(move || s.remove_deleted(Some(id()))),
            ],
            at,
        )
    };
    view()
        .h(move || px(ROW_H) * font_scale())
        .px(px(16.0))
        .flex_row()
        .items_center()
        .gap(px(12.0))
        .rounded(px(10.0))
        .bg(move || if hovered.get() { tok("hover").opacity(0.7) } else { theme().background.opacity(0.0) })
        .on_mouse_enter(move |_: &MouseEvent| hovered.set(true))
        .on_mouse_leave(move |_: &MouseEvent| hovered.set(false))
        .on_context_menu(move |e: &ContextMenuEvent| menu(Some(e.page_position)))
        .child(badge_with_icon(move || row.with(|r| r.issuer.clone()), || false, move || row.with(|r| r.icon.clone()), 34.0))
        .child(
            view()
                .flex_1()
                .min_w(px(0.0))
                .gap(px(2.0))
                .child(text(move || row.with(|r| r.issuer.clone())).number_of_lines(1).text_size(px(13.0)).font_weight(FontWeight::MEDIUM))
                .child(
                    text(move || row.with(|r| r.username.clone()))
                        .number_of_lines(1)
                        .text_size(px(12.0))
                        .text_color(move || theme().muted_foreground),
                ),
        )
        .child(
            text(move || s.count("days-left", row.with(|r| r.days_left as usize)))
                .text_size(px(12.0))
                .text_color(move || theme().muted_foreground),
        )
        .child(
            view()
                .opacity(move || if hovered.get() { 1.0_f32 } else { 0.0 })
                .child(push_button(move || s.label("Recover"), Kind::Link, false).on_press(move || s.recover_account(id()))),
        )
}
