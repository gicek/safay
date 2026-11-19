//! The menu bar panel (macOS; `desktop.rs` opens it under the tray icon): a store of its own over the same session.
use crate::{
    components::*,
    icons::{Icon, icon},
    store::Store,
};
use nsh_base::{button, input, theme};
use nsh_core::{element::desktop::MenuItem, prelude::*};

pub const ROW: f32 = 44.0;

pub fn page(s: Store) -> impl IntoElement {
    when(move || s.ready.get(), move || content(s)).otherwise(|| view().flex_1())
}
fn content(s: Store) -> impl IntoElement {
    // The screenshot button pressed while locked: the panel unlocks first, then the capture starts.
    let capture_after_unlock = signal(false);
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    effect(move || {
        if s.unlocked() && capture_after_unlock.get() {
            capture_after_unlock.set(false);
            s.status.set(None);
            start_capture();
        }
    });
    // Unlocked here, the search field takes the focus while NSH still holds the lock screen's password field, and
    // removing that field a frame later clears the search field's focus state: the caret stays but not its focus ring.
    // Once the password field is gone, the search field takes the focus again.
    let was_locked = std::cell::Cell::new(false);
    effect(move || {
        if !s.unlocked() {
            was_locked.set(true);
        } else if was_locked.replace(false) {
            let search = untracked(|| s.search.get());
            set_timeout(std::time::Duration::from_millis(100), move || {
                search.blur();
                search.focus();
            });
        }
    });
    view()
        .flex_1()
        .child(when(move || s.unlocked(), move || unlocked(s)).otherwise(move || {
            when(
                move || s.projection.with(|p| p.exists),
                move || view().flex_1().items_center().justify_center().p(px(20.0)).child(super::unlock::unlock_form(s, true)),
            )
            .otherwise(move || setup(s))
        }))
        .child(when(move || s.projection.with(|p| p.exists), move || footer(s, capture_after_unlock)))
}
/// The panel closes while the user selects; the main window runs the capture and reports the result on the desktop.
#[cfg(any(target_os = "macos", target_os = "linux"))]
fn start_capture() {
    crate::desktop::close_panel();
    crate::platform::request_capture();
}
fn setup(s: Store) -> impl IntoElement {
    view()
        .flex_1()
        .p(px(24.0))
        .items_center()
        .justify_center()
        .gap(px(10.0))
        .child(app_icon(58.0))
        .child(heading(move || s.label("Get started with Safay"), 13.5))
        .child(note(move || s.label("Create a vault or restore a backup in the main window.")).max_w(px(220.0)).text_center())
        .child(push_button(move || s.label("Open Main Window"), Kind::Standard, false).on_press(open_main))
}
fn open_main() {
    crate::platform::open_main();
}
fn unlocked(s: Store) -> impl IntoElement {
    // The scope is a preference shared with Settings; a deleted category reads as all.
    effect(move || {
        let want = s.preferences.with(|p| p.menu_bar_category);
        let valid = want.filter(|id| s.projection.with(|p| p.categories.iter().any(|c| c.id == *id)));
        if untracked(|| s.category.get()) != valid {
            s.category.set(valid);
            s.refresh();
        }
    });
    // Where the list is scrolled and how tall it shows, for keeping a keyboard-highlighted row in view.
    let list = element_ref();
    let offset = signal(0.0_f32);
    let viewport = signal(0.0_f32);
    // Only the arrow keys scroll, and only as far as the row needs: hovering highlights a row without moving the list
    // (scrolling on hover moved another row under the pointer, which scrolled again).
    let reveal = move |i: usize| {
        let (top, height, at) = (i as f32 * ROW, viewport.get(), offset.get());
        if height <= 0.0 {
            return;
        }
        if top < at {
            list.scroll_to_offset(px(top), false);
        } else if top + ROW > at + height {
            list.scroll_to_offset(px(top + ROW - height), false);
        }
    };
    // While typing in the (single-line) search field too: the arrows move the highlight.
    on_key_with("down", KeyOptions { when: nsh_core::keymap::When::Always, ..Default::default() }, move |_| {
        let n = s.projection.with(|p| p.rows.len());
        if n > 0 {
            let next = s.highlight.get().map(|i| (i + 1).min(n - 1)).unwrap_or(0);
            s.highlight.set(Some(next));
            reveal(next);
        }
    });
    // While typing in the (single-line) search field too: the arrows move the highlight.
    on_key_with("up", KeyOptions { when: nsh_core::keymap::When::Always, ..Default::default() }, move |_| {
        let n = s.projection.with(|p| p.rows.len());
        if n > 0 {
            let next = s.highlight.get().map(|i| i.saturating_sub(1)).unwrap_or(n - 1);
            s.highlight.set(Some(next));
            reveal(next);
        }
    });
    view()
        .flex_1()
        .child(
            view().px(px(10.0)).pt(px(10.0)).pb(px(8.0)).child(
                view()
                    .h(px(38.0))
                    .flex_row()
                    .items_center()
                    .gap(px(6.0))
                    .px(px(14.0))
                    .rounded_full()
                    .bg(move || tok("input-bg").opacity(0.7))
                    .border_1()
                    .border_color(move || tok("separator").opacity(0.6))
                    .focus_within(|st| st.border_color(move || tok("code-accent").opacity(0.7)).border_2())
                    .text_color(move || theme().muted_foreground)
                    .child(icon(Icon::Search).size(px(15.0)))
                    .child(
                        input()
                            .element_ref(&s.search.get())
                            .test_id("panel-search")
                            .auto_focus(true)
                            .label(move || s.label("Search accounts"))
                            .placeholder(move || s.label("Search accounts").to_string())
                            .placeholder_color(move || {
                                let c = theme().muted_foreground;
                                if dark() { c.dark } else { c.light }
                            })
                            .value(move || s.query.get())
                            .on_change(move |v: &str| {
                                s.controller.get().activity();
                                s.query.set(v.into());
                                s.highlight.set(if v.is_empty() { None } else { Some(0) });
                                // A new search starts at the top of its results.
                                list.scroll_to_offset(px(0.0), false);
                                s.refresh();
                            })
                            // Escape leaves the search field before the window sees it: the panel closes at once.
                            .on_key_press(|key: &str| {
                                #[cfg(any(target_os = "macos", target_os = "linux"))]
                                if key == "Escape" {
                                    crate::desktop::close_panel();
                                }
                                let _ = key;
                            })
                            .on_submit(move |_: &str| {
                                let i = s.highlight.get().unwrap_or(0);
                                if let Some(id) = s.projection.with(|p| p.rows.get(i).map(|r| r.id)) {
                                    s.copy(id);
                                }
                            })
                            .flex_1()
                            .min_w(px(0.0))
                            .text_size(px(13.0))
                            .text_color(move || theme().foreground),
                    ),
            ),
        )
        .child(divider())
        .child(when(move || s.projection.with(|p| p.rows.is_empty()), move || empty(s)).otherwise(move || {
            // A virtualized list (NSH's rule for collections that grow): unlocking a vault of many accounts builds only the
            // rows that show, so the panel switches from the lock screen at once.
            virtual_list(
                move || s.projection.with(|p| p.rows.len()),
                move |at| {
                    let row = computed(move || s.projection.with(|p| p.rows.get(at.index()).cloned().unwrap_or_default()));
                    let index = move || at.index();
                    let id = move || row.with(|r| r.id);
                    let lit = move || s.highlight.get() == Some(index());
                    let copied = move || s.copied.get() == Some(id());
                    let last = move || index() + 1 >= s.projection.with(|p| p.rows.len());
                    let lit_or_next = move || s.highlight.get().is_some_and(|h| h == index() || h == index() + 1);
                    view()
                        // Only a pointer that moves takes the highlight: rows the arrow keys scroll under a resting pointer
                        // must not take it back from the keyboard.
                        .on_mouse_move(move |_: &MouseEvent| {
                            if s.highlight.get() != Some(index()) {
                                s.highlight.set(Some(index()));
                            }
                        })
                        .child(
                            view()
                                .absolute()
                                .left(px(12.0))
                                .right(px(12.0))
                                .bottom(px(0.0))
                                .h(px(1.0))
                                .bg(move || tok("separator"))
                                .opacity(move || if last() || lit_or_next() { 0.0_f32 } else { 0.6 }),
                        )
                        .child(
                            button()
                                .accessibility_label(move || Some(s.fmt("Copy code for {}", &[&row.with(|r| r.issuer.clone())])))
                                .h(move || px(ROW) * font_scale())
                                .px(px(8.0))
                                .flex_row()
                                .items_center()
                                .gap(px(10.0))
                                .rounded(px(7.0))
                                .bg(move || if lit() { tok("code-accent").opacity(0.16) } else { theme().background.opacity(0.0) })
                                .focus(|st| st.bg(move || tok("code-accent").opacity(0.16)))
                                .active(|st| st.opacity(0.8))
                                .on_press(move || s.copy(id()))
                                .child(badge_with_icon(
                                    move || row.with(|r| r.issuer.clone()),
                                    move || row.with(|r| r.recent),
                                    move || row.with(|r| r.icon.clone()),
                                    26.0,
                                ))
                                .child(
                                    view()
                                        .flex_1()
                                        .min_w(px(0.0))
                                        .gap(px(1.0))
                                        .child(
                                            text(move || row.with(|r| r.issuer.clone()))
                                                .number_of_lines(1)
                                                .text_size(px(12.5))
                                                .font_weight(FontWeight::SEMIBOLD),
                                        )
                                        .child(
                                            text(move || row.with(|r| r.username.clone()))
                                                .number_of_lines(1)
                                                .text_size(px(10.5))
                                                .text_color(move || theme().muted_foreground),
                                        ),
                                )
                                .child(
                                    view().w(px(92.0)).items_end().child(
                                        when(copied, move || {
                                            view()
                                                .flex_row()
                                                .items_center()
                                                .gap(px(4.0))
                                                .text_color(move || tok("success"))
                                                .child(icon(Icon::Check).size(px(12.0)))
                                                .child(
                                                    text(move || s.label("Copied")).text_size(px(11.5)).font_weight(FontWeight::SEMIBOLD),
                                                )
                                        })
                                        .otherwise(move || {
                                            text(move || row.with(|r| r.code.clone()))
                                                .font_family(CODE_FONT.family)
                                                .number_of_lines(1)
                                                .text_size(move || px(if row.with(|r| r.digits) > 6 { 13.0 } else { 14.0 }))
                                                .font_weight(FontWeight::SEMIBOLD)
                                        }),
                                    ),
                                )
                                .child(code_ring(move || Some(row.with(|r| (r.remaining, r.period))), 20.0)),
                        )
                },
            )
            .keys(move || s.projection.with(|p| p.rows.iter().map(|r| r.id).collect::<Vec<_>>()))
            .estimated_item_size(move || px(ROW) * font_scale())
            .content_padding_x(px(4.0))
            .element_ref(&list)
            .on_scroll(move |e: &ScrollEvent| {
                offset.set(e.offset.y.0);
                viewport.set(e.size.height.0);
            })
            .on_layout(move |bounds: &Bounds<Points>| viewport.set(bounds.size.height.0))
            .flex_1()
        }))
}
fn empty(s: Store) -> impl IntoElement {
    let vault_empty = move || s.projection.with(|p| p.account_count == 0);
    view()
        .flex_1()
        .p(px(20.0))
        .items_center()
        .justify_center()
        .gap(px(8.0))
        .child(
            when(vault_empty, move || {
                view()
                    .size(px(58.0))
                    .rounded(px(14.0))
                    .items_center()
                    .justify_center()
                    .bg(move || tok("code-accent").opacity(0.1))
                    .text_color(move || tok("code-accent"))
                    .child(icon(Icon::LockShield).size(px(26.0)))
            })
            // An empty category or search: the empty vault's tile, smaller, as in the main window.
            .otherwise(move || {
                view()
                    .size(px(44.0))
                    .rounded(px(11.0))
                    .items_center()
                    .justify_center()
                    .bg(move || tok("code-accent").opacity(0.10))
                    .border_1()
                    .border_color(move || tok("code-accent").opacity(0.18))
                    .text_color(move || tok("code-accent"))
                    .child(
                        when(move || s.query.with(|q| q.is_empty()), || icon(Icon::Folder).size(px(20.0)))
                            .otherwise(|| icon(Icon::Search).size(px(20.0))),
                    )
            }),
        )
        .child(
            text(move || {
                if vault_empty() {
                    s.label("No 2FA accounts yet")
                } else if s.query.with(|q| q.is_empty()) {
                    let name = s
                        .category
                        .get()
                        .and_then(|id| s.projection.with(|p| p.categories.iter().find(|c| c.id == id).map(|c| c.name.clone())))
                        .unwrap_or_default();
                    s.fmt("No accounts in “{}” yet", &[&name])
                } else {
                    s.label("No matching accounts")
                }
            })
            .text_center()
            .text_size(px(13.0))
            .font_weight(FontWeight::SEMIBOLD),
        )
        .child(when(vault_empty, move || {
            note(move || s.label("After you add accounts, your codes will appear here securely.")).max_w(px(230.0)).text_center()
        }))
}
fn footer_button(label: impl Fn() -> SharedString + 'static) -> nsh_base::ButtonElement {
    button()
        .accessibility_label(move || Some(label()))
        .min_w(px(28.0))
        .h(px(28.0))
        .px(px(6.0))
        .flex_row()
        .items_center()
        .justify_center()
        .gap(px(4.0))
        .rounded(px(7.0))
        .text_color(move || theme().muted_foreground)
        .hover(|st| st.bg(move || tok("hover")))
        .focus(|st| st.bg(move || tok("hover")))
}
fn footer(s: Store, capture_after_unlock: Signal<bool>) -> impl IntoElement {
    view().child(divider()).child(
        view()
            .flex_row()
            .items_center()
            .px(px(8.0))
            .py(px(7.0))
            .child(if crate::platform::supports::SCREEN_CAPTURE {
                // "Add account via screenshot". While locked, the panel's own unlock comes first (Touch ID at once when
                // it is set up, else the password field), and the capture starts once it is unlocked.
                footer_button(move || s.label("Add account via screenshot"))
                    .on_press(move || {
                        #[cfg(any(target_os = "macos", target_os = "linux"))]
                        {
                            if s.unlocked() {
                                start_capture();
                            } else {
                                capture_after_unlock.set(true);
                                s.status.set(Some(crate::store::Status {
                                    tone: crate::store::Tone::Info,
                                    text: s.label("Unlock Safay first"),
                                }));
                                if s.touch_id_ready() && !s.busy.get() {
                                    s.unlock_touch_id();
                                }
                            }
                        }
                        let _ = capture_after_unlock;
                    })
                    .child(crate::icons::symbol(Icon::QrScan, 15.0, move || theme().foreground))
                    .into_element()
            } else {
                view().w(px(28.0)).into_element()
            })
            .child(spacer())
            .child(when(
                move || s.unlocked() && s.projection.with(|p| !p.categories.is_empty()),
                move || {
                    footer_button(move || {
                        s.label(if crate::chrome::look() == crate::chrome::Look::Mac { "Menu Bar Accounts" } else { "Tray Accounts" })
                    })
                    .on_press(move || {
                        let current = s.category.get();
                        let mut items = vec![
                            MenuItem::new(s.label("All").to_string())
                                .checked(current.is_none())
                                .on_select(move || s.update_preferences(|p| p.menu_bar_category = None)),
                            MenuItem::separator(),
                        ];
                        items.extend(s.projection.with(|p| {
                            p.categories
                                .iter()
                                .map(|c| {
                                    let id = c.id;
                                    MenuItem::new(c.name.clone())
                                        .checked(current == Some(id))
                                        .on_select(move || s.update_preferences(move |p| p.menu_bar_category = Some(id)))
                                })
                                .collect::<Vec<_>>()
                        }));
                        show_menu(s, items, None);
                    })
                    .child(
                        text(move || {
                            s.category
                                .get()
                                .and_then(|id| {
                                    s.projection
                                        .with(|p| p.categories.iter().find(|c| c.id == id).map(|c| SharedString::from(c.name.clone())))
                                })
                                .unwrap_or_else(|| s.label("All"))
                        })
                        .max_w(px(150.0))
                        .number_of_lines(1)
                        .text_size(px(12.0))
                        .font_weight(FontWeight::MEDIUM),
                    )
                    // The name, then an 8pt semibold chevron.up.chevron.down.
                    .child(crate::icons::symbol_weight(Icon::ChevronUpDown, 8.0, 0.3, move || theme().muted_foreground))
                },
            ))
            .child(spacer())
            .child(
                footer_button(move || s.label("Menu"))
                    .on_press(move || {
                        let mut items = Vec::new();
                        if s.unlocked() {
                            items.push(MenuItem::new(s.label("Lock").to_string()).on_select(move || s.lock()));
                        }
                        items.push(MenuItem::new(s.label("Main Window").to_string()).on_select(open_main));
                        items.push(MenuItem::new(s.label("Settings").to_string()).on_select(crate::platform::open_settings_window));
                        items.push(MenuItem::separator());
                        items.push(MenuItem::new(s.label("Quit").to_string()).on_select(crate::platform::quit));
                        show_menu(s, items, None);
                    })
                    .child(crate::icons::symbol(Icon::MoreCircle, 15.0, move || theme().foreground)),
            ),
    )
}
