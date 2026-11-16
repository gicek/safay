//! Sidebar, card grid and list. Account actions live in the native context menu;
//! keyboard users reach the same menu with the context-menu key or Shift+F10 on a focused card.
use super::reorder::{self, Reorder};
use crate::{
    chrome::{self, Look, look},
    components::*,
    icons::{Icon, icon},
    store::{CategoryEdit, Sheet, Store},
};
use nsh_base::{button, input, theme};
use nsh_core::{element::desktop::MenuItem, prelude::*};
use safay_app::{Uuid, session::Row};

const CARD_MIN: f32 = 210.0;
pub(super) const GAP: f32 = 12.0;
const CARD_H: f32 = 94.0;
const LIST_H: f32 = 58.0;

fn content_width(s: Store) -> f32 {
    let side = if s.sidebar.get() && window().width.0 >= 640.0 { chrome::sidebar_width() + chrome::sidebar_inset() } else { 0.0 };
    window().width.0 - side - 32.0
}
pub(super) fn columns(s: Store) -> u32 {
    if s.cards.get() { (((content_width(s) + GAP) / (CARD_MIN * font_scale() + GAP)).floor() as u32).clamp(1, 6) } else { 1 }
}
fn row_height(s: Store) -> f32 {
    if s.cards.get() { CARD_H } else { LIST_H }
}

// ---------------------------------------------------------------- sidebar

fn sidebar_row(
    s: Store,
    glyph: Icon,
    label: impl Fn() -> SharedString + 'static,
    count: impl Fn() -> Option<usize> + 'static,
    active: impl Fn() -> bool + Copy + 'static,
) -> nsh_base::ButtonElement {
    let h = chrome::sidebar_row_height();
    let accent_fill = look() == Look::Mac;
    // macOS: the accent in an active window, the system's gray (and normal text) in an inactive one.
    let focused = chrome::window_active();
    let accent = move || active() && accent_fill && focused();
    let label = std::rc::Rc::new(label);
    let name = label.clone();
    button()
        .accessibility_label(move || Some(name()))
        .selected(active)
        .min_h(move || px(h) * font_scale())
        .px(px(10.0))
        .flex_row()
        .items_center()
        .gap(px(10.0))
        .rounded(px(if look() == Look::Mac { 6.0 } else { chrome::control_radius(false) }))
        .bg(move || match (active(), accent_fill) {
            (false, _) => theme().sidebar.opacity(0.0),
            (true, true) if focused() => tok("selection"),
            (true, true) => tok("selection-inactive"),
            (true, false) => tok("row-selected"),
        })
        .text_color(move || if accent() { tok("selection-foreground") } else { theme().sidebar_foreground })
        .hover(move |st| {
            if accent_fill {
                st.bg(move || match (active(), focused()) {
                    (true, true) => tok("selection"),
                    (true, false) => tok("selection-inactive"),
                    _ => tok("hover"),
                })
            } else {
                st.bg(move || tok("hover"))
            }
        })
        .focus(focus_ring)
        .active(|st| st.scale(0.98))
        .on_press(move || s.controller.get().activity())
        .child(when(
            move || look() == Look::Windows && active(),
            || view().absolute().left(px(0.0)).top(px(10.0)).bottom(px(10.0)).w(px(3.0)).rounded_full().bg(move || theme().primary),
        ))
        .child(view().w(px(24.0)).items_center().child(crate::icons::symbol(glyph, 17.0, move || {
            if accent() { tok("selection-foreground") } else { theme().sidebar_foreground }
        })))
        .child(text(move || label()).number_of_lines(1).flex_1().text_size(px(13.0)).font_weight(FontWeight::MEDIUM))
        .child(
            text(move || count().map(|n| n.to_string()).unwrap_or_default())
                .text_size(px(10.0))
                .text_color(move || if accent() { tok("selection-foreground").opacity(0.85) } else { theme().muted_foreground }),
        )
}
fn header(text_: impl Fn() -> SharedString + 'static) -> impl IntoElement + Styled {
    text(text_).text_size(px(11.0)).font_weight(FontWeight::SEMIBOLD).text_color(move || theme().muted_foreground)
}

pub fn sidebar(s: Store) -> impl IntoElement {
    let all_active = move || s.category.get().is_none();
    let r = Reorder::new(s, reorder::Kind::Categories);
    view()
        .child(escape_cancels(r))
        .flex_1()
        .px(px(8.0))
        .pb(px(10.0))
        .child(
            scroll()
                .flex_1()
                .on_context_menu(move |e: &ContextMenuEvent| {
                    show_menu(
                        s,
                        vec![MenuItem::new(s.label("New Category").to_string()).on_select(move || new_category(s, None))],
                        Some(e.page_position),
                    );
                })
                .child(
                    view()
                        .gap(px(2.0))
                        .child(header(move || s.label("Accounts")).px(px(10.0)).pt(px(22.0)))
                        .child(
                            sidebar_row(
                                s,
                                Icon::Accounts,
                                move || s.label("All"),
                                move || Some(s.projection.with(|p| p.account_count)),
                                all_active,
                            )
                            .on_press(move || {
                                s.category.set(None);
                                s.refresh();
                            }),
                        )
                        .child(
                            view()
                                .flex_row()
                                .items_center()
                                .pl(px(10.0))
                                .pr(px(4.0))
                                .pt(px(18.0))
                                .child(header(move || s.label("Categories")).flex_1())
                                .child(
                                    icon_button(move || s.label("New Category"), Icon::Plus)
                                        .size(px(18.0))
                                        .on_press(move || new_category(s, None)),
                                ),
                        )
                        .child(categories(s, r))
                        .child(when(move || s.editing.with(|e| e.as_ref().is_some_and(|e| e.id.is_none())), move || category_editor(s))),
                ),
        )
        .child(view().px(px(6.0)).pb(px(6.0)).child(divider()))
        .child(
            view()
                .gap(px(2.0))
                .child(
                    sidebar_row(s, Icon::ExportImport, move || s.label("Export / Import"), || None, || false)
                        .on_press(move || s.open_sheet(Sheet::ExportImport)),
                )
                .child(
                    view()
                        .child(
                            sidebar_row(s, Icon::Cloud, move || s.label("Backup & Sync"), || None, || false)
                                .on_press(move || s.open_sheet(Sheet::Backup)),
                        )
                        .child(when(
                            move || super::backup::dot_color(s).is_some(),
                            move || {
                                view()
                                    .absolute()
                                    .right(px(14.0))
                                    .top(px(0.0))
                                    .bottom(px(0.0))
                                    .justify_center()
                                    .pointer_events(nsh_core::PointerEvents::None)
                                    .child(
                                        view()
                                            .size(px(7.0))
                                            .rounded_full()
                                            .bg(move || super::backup::dot_color(s).unwrap_or(theme().muted_foreground)),
                                    )
                            },
                        )),
                )
                .child(
                    sidebar_row(s, Icon::Settings, move || s.label("Settings"), || None, || false)
                        .on_press(move || crate::shell::open_settings(s)),
                ),
        )
}
fn new_category(s: Store, account: Option<Uuid>) {
    s.editing.set(Some(CategoryEdit { account, ..Default::default() }));
}
/// The category rows, their own block as the drag container: dropping inside it saves the order; on the header, "All"
/// or outside the sidebar the order comes back.
fn categories(s: Store, r: Reorder) -> impl IntoElement {
    view()
        .gap(px(reorder::CATEGORY_GAP))
        .element_ref(&r.list)
        .child(each(move || r.categories(), |c| c.id, move |cat, _| category_row(s, r, cat)))
        .child(when(move || r.pointer.get().is_some() && r.dragged.get().is_some(), move || category_standin(s, r)))
}
/// The dragged category following the pointer: the row's look over an opaque background, lifted by a shadow.
fn category_standin(s: Store, r: Reorder) -> impl IntoElement {
    let frame = move || r.pointer.get().unwrap_or_default();
    let cat = move || {
        let id = r.dragged.get();
        s.projection.with(|p| p.categories.iter().find(|c| Some(c.id) == id).map(|c| (c.name.clone(), c.count)))
    };
    view()
        .absolute()
        .left(move || px(frame().0))
        .top(move || px(frame().1))
        .w(move || px(frame().2))
        .h(move || px(frame().3))
        .pointer_events(nsh_core::PointerEvents::None)
        .accessibility_elements_hidden(true)
        .rounded(px(if look() == Look::Mac { 6.0 } else { chrome::control_radius(false) }))
        .bg(move || theme().background)
        .shadow([soft_shadow(3.0, 16.0, 0.18)])
        .child(sidebar_row(
            s,
            Icon::Folder,
            move || cat().map(|c| c.0).unwrap_or_default().into(),
            move || cat().map(|c| c.1),
            move || r.dragged.get().is_some() && s.category.get() == r.dragged.get(),
        ))
}
/// While a drag runs, Escape puts the order back.
fn escape_cancels(r: Reorder) -> impl IntoElement {
    when(
        move || r.dragged.get().is_some(),
        move || {
            on_key_with("escape", KeyOptions { when: nsh_core::keymap::When::Always, ..Default::default() }, move |_| r.escape());
            view().size(px(0.0))
        },
    )
}
fn category_row(s: Store, r: Reorder, cat: ReadSignal<safay_app::session::CategoryRow>) -> impl IntoElement {
    let id = move || cat.with(|c| c.id);
    let active = move || s.category.get() == Some(id());
    when(move || s.editing.with(|e| e.as_ref().is_some_and(|e| e.id == Some(id()))), move || category_editor(s)).otherwise(move || {
        let me = element_ref();
        let el = view()
            .opacity(move || if r.is_dragging(Some(id())) { 0.4_f32 } else { 1.0 })
            .transition_opacity()
            .duration(reorder::CATEGORY_SWAP_MS)
            .child(
                sidebar_row(s, Icon::Folder, move || cat.with(|c| c.name.clone()).into(), move || Some(cat.with(|c| c.count)), active)
                    .on_press(move || {
                        s.category.set(Some(id()));
                        s.refresh();
                    }),
            )
            .on_context_menu(move |e: &ContextMenuEvent| {
                let target = id();
                let name = cat.with(|c| c.name.clone());
                show_menu(
                    s,
                    vec![
                        MenuItem::new(s.label("Rename").to_string()).on_select(move || {
                            s.editing.set(Some(CategoryEdit { id: Some(target), name: name.clone(), ..Default::default() }))
                        }),
                        MenuItem::new(s.label("Delete Category").to_string()).on_select(move || s.delete_category(target)),
                        MenuItem::separator(),
                        MenuItem::new(s.label("Move Up").to_string()).on_select(move || s.move_category(target, -1)),
                        MenuItem::new(s.label("Move Down").to_string()).on_select(move || s.move_category(target, 1)),
                        MenuItem::separator(),
                        MenuItem::new(s.label("New Category").to_string()).on_select(move || new_category(s, None)),
                    ],
                    Some(e.page_position),
                );
            });
        r.item(el, me, move || Some(id()))
    })
}
/// CategoryNameField: Return commits, Escape cancels, leaving with an empty name discards.
fn category_editor(s: Store) -> impl IntoElement {
    let error = move || s.editing.with(|e| e.as_ref().and_then(|e| e.error));
    on_key_with("escape", KeyOptions { when: nsh_core::keymap::When::Always, ..Default::default() }, move |_| s.editing.set(None));
    view()
        .gap(px(3.0))
        .child(
            view()
                .flex_row()
                .items_center()
                .gap(px(10.0))
                .min_h(move || px(chrome::sidebar_row_height()) * font_scale())
                .px(px(10.0))
                .rounded(px(6.0))
                .bg(move || tok("hover"))
                .border_1()
                .border_color(move || if error().is_some() { theme().destructive.opacity(0.7) } else { tok("code-accent").opacity(0.6) })
                .text_color(move || theme().muted_foreground)
                .child(view().w(px(24.0)).items_center().child(icon(Icon::Folder).size(px(17.0))))
                .child(
                    input()
                        .test_id("category-name")
                        .auto_focus(true)
                        .label(move || s.label("Category name"))
                        .placeholder(move || s.label("Category name").to_string())
                        .value(move || s.editing.with(|e| e.as_ref().map(|e| e.name.clone()).unwrap_or_default()))
                        .on_change(move |v: &str| {
                            s.editing.update(|e| {
                                if let Some(e) = e {
                                    e.name = v.into();
                                    e.error = None;
                                }
                            })
                        })
                        .on_submit(move |_: &str| s.commit_category())
                        .on_blur(move |_: &()| {
                            if s.editing.with(|e| e.as_ref().is_some_and(|e| e.error.is_none())) {
                                s.commit_category()
                            }
                        })
                        .flex_1()
                        .min_w(px(0.0))
                        .text_size(px(13.0))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(move || theme().foreground),
                ),
        )
        .child(when(
            move || error().is_some(),
            move || {
                text(move || s.label(crate::i18n::error_key(error().unwrap_or(safay_app::session::Error::Category))))
                    .px(px(10.0))
                    .text_size(px(11.0))
                    .text_color(move || theme().destructive)
            },
        ))
}

// ---------------------------------------------------------------- accounts

pub fn page(s: Store) -> impl IntoElement {
    let r = Reorder::new(s, reorder::Kind::Accounts);
    view().flex_1().child(escape_cancels(r)).child(
        when(move || s.projection.with(|p| p.rows.is_empty()), move || empty_state(s)).otherwise(move || {
            view()
                .flex_1()
                .child(
                    virtual_list(
                        move || r.count(),
                        move |at| {
                            let row = computed(move || r.row(at.index()));
                            when(move || s.cards.get(), move || card(s, r, row, false))
                                .otherwise(move || list_row(s, r, row, at.index(), false))
                        },
                    )
                    .keys(move || r.ids())
                    .columns(move || columns(s))
                    .estimated_item_size(move || px(row_height(s)) * font_scale())
                    .row_gap(move || px(if s.cards.get() { GAP } else { 0.0 }))
                    .column_gap(px(GAP))
                    .content_padding_x(move || px(if s.cards.get() { 16.0 } else { 8.0 }))
                    .content_padding_y(move || px(if s.cards.get() { 16.0 } else { 4.0 }))
                    .on_scroll(move |e: &ScrollEvent| {
                        r.scrolled(e.offset.y.0);
                        s.controller.get().activity()
                    })
                    .element_ref(&r.list)
                    .flex_1(),
                )
                .child(when(move || r.dragged.get().is_some(), edge_zones))
                .child(when(move || r.pointer.get().is_some() && r.standin.with(|s| s.is_some()), move || standin(s, r)))
        }),
    )
}

/// The auto-scroll zones at the top and bottom of the list while dragging: a faint accent wash fading inwards.
fn edge_zones() -> impl IntoElement {
    let band = |top: bool| {
        let wash = |alpha: f32| view().flex_1().bg(move || tok("code-accent").opacity(alpha));
        let washes = if top { [0.10, 0.05, 0.02] } else { [0.02, 0.05, 0.10] };
        let el = view().absolute().left(px(0.0)).right(px(0.0)).h(px(reorder::EDGE_ZONE));
        let el = if top { el.top(px(0.0)) } else { el.bottom(px(0.0)) };
        el.child(wash(washes[0])).child(wash(washes[1])).child(wash(washes[2]))
    };
    view().absolute().inset_0().pointer_events(nsh_core::PointerEvents::None).child(band(true)).child(band(false))
}

/// The stand-in following the pointer: the dragged card or row as it was when the drag began, lifted by a shadow.
fn standin(s: Store, r: Reorder) -> impl IntoElement {
    let frame = move || r.pointer.get().unwrap_or_default();
    let row = computed(move || r.standin.get());
    view()
        .absolute()
        .left(move || px(frame().0))
        .top(move || px(frame().1))
        .w(move || px(frame().2))
        .h(move || px(frame().3))
        .pointer_events(nsh_core::PointerEvents::None)
        .accessibility_elements_hidden(true)
        .child(when(move || s.cards.get(), move || card(s, r, row, true)).otherwise(move || list_row(s, r, row, 0, true)))
}

/// The empty vault: the accent shield in its tile over two faint concentric round halos, the title and what happens to the
/// codes, then the two ways to begin (add one, or import a backup or export). It fades in and rises 8pt when it
/// appears (240 ms ease-out), still with Reduce Motion. A category or a search with nothing in it shows a quieter
/// glyph and line.
fn empty_state(s: Store) -> impl IntoElement {
    let vault_empty = move || s.projection.with(|p| p.account_count == 0);
    let in_category = move || s.category.get().is_some() && s.query.with(|q| q.is_empty());
    let arrived = signal(reduce_motion());
    if !arrived.get_untracked() {
        set_timeout(std::time::Duration::from_millis(0), move || arrived.set(true));
    }
    let halo = move |size: f32, alpha: f32| view().absolute().size(px(size)).rounded_full().bg(move || tok("code-accent").opacity(alpha));
    view()
        .flex_1()
        .items_center()
        .justify_center()
        .gap(px(10.0))
        .pb(px(42.0))
        .opacity(move || if arrived.get() { 1.0_f32 } else { 0.0 })
        .translate_y(move || px(if arrived.get() { 0.0 } else { 8.0 }))
        .transition()
        .duration(240.0)
        .ease_out()
        .child(
            when(vault_empty, move || {
                view()
                    .size(px(140.0))
                    .my(px(-20.0))
                    .items_center()
                    .justify_center()
                    .child(halo(140.0, 0.035))
                    .child(halo(108.0, 0.055))
                    .child(
                        view()
                            .size(px(80.0))
                            .rounded(px(20.0))
                            .items_center()
                            .justify_center()
                            .bg(move || tok("code-accent").opacity(0.12))
                            .border_1()
                            .border_color(move || tok("code-accent").opacity(0.22))
                            .shadow([soft_shadow(6.0, 16.0, 0.06)])
                            .text_color(move || tok("code-accent"))
                            .child(icon(Icon::LockShield).size(px(38.0))),
                    )
            })
            .otherwise(move || {
                view()
                    .text_color(move || theme().muted_foreground)
                    .child(when(in_category, || icon(Icon::Folder).size(px(32.0))).otherwise(|| icon(Icon::Search).size(px(32.0))))
            }),
        )
        .child(
            text(move || {
                if vault_empty() {
                    s.label("No 2FA accounts yet")
                } else if in_category() {
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
            .text_size(move || px(if vault_empty() { 20.0 } else { 13.0 }))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(move || if vault_empty() { theme().foreground } else { theme().muted_foreground }),
        )
        .child(
            text(move || {
                if vault_empty() {
                    s.label("After you add accounts, your codes are encrypted on this device and shown only after unlock.")
                } else if in_category() {
                    s.label("In the main window, right-click an account and choose Add to.")
                } else {
                    "".into()
                }
            })
            .max_w(px(360.0))
            .text_center()
            .text_size(px(13.0))
            .line_height(px(19.0))
            .text_color(move || theme().muted_foreground),
        )
        .child(when(vault_empty, move || {
            view()
                .pt(px(10.0))
                .flex_row()
                .gap(px(10.0))
                .child(
                    push_button_with_icon(move || s.label("Add Account"), Kind::Primary, true, Icon::Plus)
                        .on_press(move || s.open_sheet(Sheet::Add)),
                )
                // Nothing to export yet: straight to the import tab.
                .child(push_button_with_icon(move || s.label("Import Accounts"), Kind::Standard, true, Icon::Download).on_press(
                    move || {
                        s.open_sheet(Sheet::ExportImport);
                        s.import_tab.set(true);
                    },
                ))
        }))
}

/// The account context menu, identical for cards and rows.
pub fn account_menu(s: Store, row: &Row, at: Point<Points>) {
    let id = row.id;
    let cats = s.projection.with(|p| p.categories.clone());
    let mut add_to: Vec<MenuItem> = cats
        .iter()
        .map(|c| {
            let member = row.categories.contains(&c.id);
            let cid = c.id;
            MenuItem::new(c.name.clone()).checked(member).on_select(move || s.membership(cid, id, !member))
        })
        .collect();
    if !add_to.is_empty() {
        add_to.push(MenuItem::separator());
    }
    add_to.push(MenuItem::new(s.label("New Category").to_string()).on_select(move || new_category(s, Some(id))));
    let mut items = vec![
        MenuItem::new(s.label("Copy code").to_string()).on_select(move || s.copy(id)),
        MenuItem::submenu(s.label("Add to").to_string(), add_to),
    ];
    if let Some(current) = s.category.get() {
        let name = cats.iter().find(|c| c.id == current).map(|c| c.name.clone()).unwrap_or_default();
        items.push(MenuItem::new(s.fmt("Remove from “{}”", &[&name]).to_string()).on_select(move || s.membership(current, id, false)));
    }
    items.extend([
        MenuItem::separator(),
        MenuItem::new(s.label("Details").to_string()).shortcut("mod+i").on_select(move || s.open_sheet(Sheet::Detail(id))),
        MenuItem::new(s.label("Show QR Code").to_string()).on_select(move || s.reveal(crate::store::Auth::Qr(id))),
        MenuItem::separator(),
        MenuItem::new(s.label("Move Up").to_string()).on_select(move || s.move_row(id, -1)),
        MenuItem::new(s.label("Move Down").to_string()).on_select(move || s.move_row(id, 1)),
        MenuItem::separator(),
        MenuItem::new(s.label("Delete").to_string()).on_select(move || s.delete_account(id)),
    ]);
    show_menu(s, items, Some(at));
}

fn identity(
    s: Store,
    row: Memo<Option<Row>>,
    badge_size: f32,
    user_size: f32,
    hovered: Signal<bool>,
    details_inline: bool,
) -> impl IntoElement + Styled {
    let issuer = move || row.with(|r| r.as_ref().map(|r| r.issuer.clone()).unwrap_or_default());
    let id = move || row.with(|r| r.as_ref().map(|r| r.id));
    view()
        .flex_row()
        .items_center()
        .gap(px(if badge_size > 32.0 { 12.0 } else { 10.0 }))
        .min_w(px(0.0))
        .child(badge_with_icon(
            issuer,
            move || row.with(|r| r.as_ref().is_some_and(|r| r.recent)),
            move || row.with(|r| r.as_ref().and_then(|r| r.icon.clone())),
            badge_size,
        ))
        .child(
            view()
                .flex_1()
                .min_w(px(0.0))
                .gap(px(2.0))
                .child(
                    // The details button floats over the line's end as a hover overlay, so the
                    // issuer keeps the full width and the line its 16pt height.
                    view()
                        .child(
                            text(issuer)
                                .number_of_lines(1)
                                .pr(move || px(if details_inline && hovered.get() { 22.0 } else { 0.0 }))
                                .text_size(px(13.5))
                                .line_height(px(16.0))
                                .font_weight(FontWeight::SEMIBOLD),
                        )
                        .child(when(
                            move || details_inline,
                            move || {
                                view()
                                    .absolute()
                                    .right(px(-4.0))
                                    .top(px(-2.0))
                                    .opacity(move || if hovered.get() { 1.0_f32 } else { 0.0 })
                                    .child(details_button(s, id))
                            },
                        )),
                )
                .child(
                    text(move || row.with(|r| r.as_ref().map(|r| r.username.clone()).unwrap_or_default()))
                        .number_of_lines(1)
                        .text_size(px(user_size))
                        .line_height(px(14.0))
                        .text_color(move || theme().muted_foreground),
                ),
        )
}
fn details_button(s: Store, id: impl Fn() -> Option<Uuid> + Copy + 'static) -> impl IntoElement {
    icon_button(move || s.label("Details"), Icon::Info).size(px(20.0)).on_press(move || {
        if let Some(id) = id() {
            s.open_sheet(Sheet::Detail(id))
        }
    })
}
fn code_text(row: Memo<Option<Row>>, size: f32) -> impl IntoElement + Styled {
    text(move || row.with(|r| r.as_ref().map(|r| r.code.clone()).unwrap_or_default()))
        .font_family(CODE_FONT.family)
        .text_size(px(size))
        .font_weight(FontWeight::SEMIBOLD)
        .number_of_lines(1)
        .text_color(move || theme().foreground)
}
fn copied_label(s: Store, row: Memo<Option<Row>>) -> impl IntoElement {
    let id = move || row.with(|r| r.as_ref().map(|r| r.id));
    when(
        move || s.copied.get().is_some() && s.copied.get() == id(),
        move || {
            view()
                .flex_row()
                .items_center()
                .gap(px(4.0))
                .text_color(move || tok("success"))
                .child(icon(Icon::Check).size(px(13.0)))
                .child(text(move || s.label("Copied")).text_size(px(12.0)).font_weight(FontWeight::SEMIBOLD))
        },
    )
}
fn press_target(s: Store, row: Memo<Option<Row>>, hovered: Signal<bool>) -> nsh_base::ButtonElement {
    let _ = hovered;
    let id = move || row.with(|r| r.as_ref().map(|r| r.id));
    button()
        .accessibility_label(move || {
            Some(s.fmt("Copy code for {}", &[&row.with(|r| r.as_ref().map(|r| r.issuer.clone()).unwrap_or_default())]))
        })
        .on_press(move || {
            if let Some(id) = id() {
                s.copy(id)
            }
        })
}
/// Hover tracking and the native context menu sit on a plain view around the press target.
fn events(s: Store, row: Memo<Option<Row>>, hovered: Signal<bool>, content: impl IntoElement) -> impl Interactive + Styled {
    view()
        .on_mouse_enter(move |_: &MouseEvent| hovered.set(true))
        .on_mouse_leave(move |_: &MouseEvent| hovered.set(false))
        .on_context_menu(move |e: &ContextMenuEvent| {
            if let Some(r) = row.get() {
                account_menu(s, &r, e.page_position)
            }
        })
        .child(content)
}
/// A card. `ghost` is the stand-in that follows the pointer while dragging: no events, a lifted shadow.
fn card(s: Store, r: Reorder, row: Memo<Option<Row>>, ghost: bool) -> impl IntoElement {
    let hovered = signal(false);
    let id = move || row.with(|r| r.as_ref().map(|r| r.id));
    let target = move || !ghost && r.is_target(id());
    let me = element_ref();
    let el = events(
        s,
        row,
        hovered,
        press_target(s, row, hovered)
            .h(move || px(CARD_H) * font_scale())
            // 12pt from the edge: the padding plus the 1pt border.
            .p(px(11.0))
            .gap(px(12.0))
            .justify_between()
            .rounded(px(chrome::card_radius()))
            .border(move || px(if target() { 1.4 } else { 1.0 }))
            .border_color(move || if target() { tok("code-accent").opacity(0.58) } else { tok("card-border") })
            .bg(move || theme().card)
            .shadow(if ghost {
                vec![soft_shadow(4.0, 20.0, 0.18)]
            } else if look() == Look::Windows {
                vec![]
            } else {
                vec![soft_shadow(1.0, 2.0, 0.03)]
            })
            .hover(move |st| {
                st.border_color(move || if target() { tok("code-accent").opacity(0.58) } else { tok("card-hover-border") })
                    .bg(move || tok("card-hover"))
            })
            .focus(|st| st.border_color(move || theme().ring))
            .active(|st| st.scale(0.99))
            .child(identity(s, row, 30.0, 11.5, hovered, !ghost).h(px(30.0)))
            .child(
                view()
                    .flex_row()
                    .items_center()
                    .gap(px(12.0))
                    .child(code_text(row, 21.0))
                    .child(copied_label(s, row))
                    .child(spacer())
                    .child(code_ring(move || row.with(|r| r.as_ref().map(|r| (r.remaining, r.period))), 20.0)),
            ),
    );
    reorderable(r, el, me, id, ghost)
}
/// While dragging, the item's own slot stays empty (the stand-in shows it); the stand-in takes no events.
fn reorderable<E: Styled + Interactive>(
    r: Reorder,
    el: E,
    me: ElementRef,
    id: impl Fn() -> Option<Uuid> + Copy + 'static,
    ghost: bool,
) -> E {
    if ghost {
        return el;
    }
    let el = el.opacity(move || if r.is_dragging(id()) { 0.0_f32 } else { 1.0 }).transition_opacity().duration(reorder::SWAP_MS);
    r.item(el, me, id)
}
fn list_row(s: Store, r: Reorder, row: Memo<Option<Row>>, index: usize, ghost: bool) -> impl IntoElement {
    let hovered = signal(false);
    let id = move || row.with(|r| r.as_ref().map(|r| r.id));
    let target = move || !ghost && r.is_target(id());
    let me = element_ref();
    let el = view()
        .child(events(
            s,
            row,
            hovered,
            press_target(s, row, hovered)
                .h(move || px(LIST_H) * font_scale())
                .px(px(16.0))
                .flex_row()
                .items_center()
                .gap(px(12.0))
                .rounded(px(10.0))
                .border_1()
                .border_color(move || if target() { tok("code-accent").opacity(0.42) } else { theme().background.opacity(0.0) })
                .bg(move || {
                    if ghost {
                        theme().background
                    } else if hovered.get() && !target() {
                        tok("hover").opacity(0.7)
                    } else {
                        theme().background.opacity(0.0)
                    }
                })
                .shadow(if ghost { vec![soft_shadow(4.0, 20.0, 0.18)] } else { vec![] })
                .focus(focus_ring)
                .child(identity(s, row, 34.0, 12.0, hovered, false).flex_1())
                .child(copied_label(s, row))
                .child(view().w(px(118.0)).items_end().child(code_text(row, 19.0)))
                .child(code_ring(move || row.with(|r| r.as_ref().map(|r| (r.remaining, r.period))), 28.0))
                .child(
                    view().w(px(30.0)).opacity(move || if hovered.get() && !ghost { 1.0_f32 } else { 0.0 }).child(details_button(s, id)),
                ),
        ))
        .child(when(move || !ghost && index + 1 < r.count() && !r.is_dragging(id()), || view().ml(px(62.0)).child(divider())));
    reorderable(r, el, me, id, ghost)
}
