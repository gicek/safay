//! The list: a top bar (the app's name, adding and the menu), the categories as chips, and the accounts as one group
//! of rows with a letter index beside them. A press on a row copies its code, a long press opens its menu and picks it
//! up to drag; the button at the bottom opens the search.
use super::{Nav, Page, ui};
use crate::{
    components::{CODE_FONT, badge_with_icon, tok},
    icons::{Icon, icon},
    initials,
    reorder::{Kind, Pickup, Reorder},
    store::Store,
};
use nsh_base::{button, input, theme};
use nsh_core::prelude::*;
use safay_app::session::Row;

/// A row: 14 pt above and below the two lines of its names.
const ROW: f32 = 72.0;
/// What the list leaves free at its end, so its last row scrolls clear of the search button.
const CLEARANCE: f32 = 120.0;
/// The top bar.
const HEADER: f32 = 56.0;
/// The search button.
const FAB: f32 = 56.0;
/// How many accounts the list holds before its letter index shows.
const INDEXED: usize = 8;

pub fn page(s: Store, nav: Nav) -> impl IntoElement {
    let empty_vault = move || s.projection.with(|p| p.account_count == 0);
    // A closed search leaves no text behind: the list shows every account again.
    effect(move || {
        if !nav.searching.get() && untracked(|| !s.query.with(String::is_empty)) {
            untracked(|| {
                s.query.set(String::new());
                s.refresh();
            });
        }
    });
    view()
        .flex_1()
        .pt_safe_area()
        .child(header(s, nav))
        .child(when(move || !empty_vault(), move || chips(s, nav)))
        .child(
            when(empty_vault, move || empty(s, nav)).otherwise(move || {
                when(move || s.projection.with(|p| p.rows.is_empty()), move || nothing(s)).otherwise(move || list(s, nav))
            }),
        )
        .child(when(move || !empty_vault(), move || search(s, nav)))
        .child(add_menu(s, nav))
        .child(overflow(s, nav))
        .child(super::menus::account(s, nav))
}

/// The bar at the top: the app's name, the add button and the menu.
fn header(s: Store, nav: Nav) -> impl IntoElement {
    view()
        .flex_row()
        .items_center()
        .h(px(HEADER))
        .pl(px(ui::MARGIN + 4.0))
        .pr(px(4.0))
        .child(ui::title_large(text("Safay")).flex_1())
        .child(ui::bar_button(move || s.label("Add Account"), || Icon::Plus).test_id("add").on_press(move || {
            dismiss_keyboard();
            nav.menu.set(false);
            nav.adding.update(|v| *v = !*v)
        }))
        // The glyph is the row of dots, upright here.
        .child(ui::bar_button(move || s.label("More"), || Icon::More).rotate(|| std::f32::consts::FRAC_PI_2).test_id("more").on_press(
            move || {
                dismiss_keyboard();
                nav.adding.set(false);
                nav.menu.set(true)
            },
        ))
}

/// The search button at the bottom: a press opens the field to its left, the keyboard up, and the button closes it
/// again, text and keyboard with it. Above the keyboard while it is up.
fn search(s: Store, nav: Nav) -> impl IntoElement {
    let open = move || nav.searching.get();
    let field = element_ref();
    // The field's width when open: the row less its margins, the button and the gap between them.
    let wide = move || window().width - px(2.0 * ui::MARGIN + FAB + 8.0);
    view()
        .absolute()
        .left(px(0.0))
        .right(px(0.0))
        .bottom(px(0.0))
        .pb(move || keyboard_height().max(safe_area().bottom) + px(ui::MARGIN))
        .px(px(ui::MARGIN))
        .flex_row()
        .justify_end()
        .items_center()
        .gap(px(8.0))
        .pointer_events(nsh_core::PointerEvents::BoxNone)
        .child(
            view()
                .w(move || if open() { wide() } else { px(0.0) })
                .h(px(FAB))
                .opacity(move || if open() { 1.0_f32 } else { 0.0 })
                .overflow_hidden()
                .flex_row()
                .items_center()
                .rounded_full()
                // White as the rows it floats over, set off from them by its outline and a deep shadow.
                .bg(move || theme().card)
                .border_1()
                .border_color(move || theme().border)
                .shadow_xl()
                .pointer_events(move || if open() { nsh_core::PointerEvents::Auto } else { nsh_core::PointerEvents::None })
                .transition()
                .duration(ui::SPATIAL_MS)
                .ease_out()
                .on_press(move |_: &()| field.focus())
                .child(view().pl(px(16.0)).text_color(move || theme().muted_foreground).child(icon(Icon::Search).size(px(22.0))))
                .child(
                    ui::themed_input(input())
                        .element_ref(&field)
                        .test_id("search")
                        .label(move || s.label("Search accounts or sites"))
                        .placeholder(move || s.label("Search accounts or sites").to_string())
                        .value(move || s.query.get())
                        .on_change(move |v: &str| {
                            s.controller.get().activity();
                            s.query.set(v.to_owned());
                            s.refresh();
                        })
                        .return_key_type(ReturnKeyType::Search)
                        .on_submit(move |_: &str| dismiss_keyboard())
                        .auto_correct(false)
                        .auto_capitalize(nsh_core::AutoCapitalize::None)
                        .auto_fill(nsh_core::AutoFill::Off)
                        .flex_1()
                        .min_w(px(0.0))
                        .mx(px(12.0))
                        .text_size(px(16.0))
                        .text_color(move || theme().foreground),
                ),
        )
        .child(
            button()
                .test_id("search-button")
                .accessibility_label(move || Some(s.label(if open() { "Close" } else { "Search" })))
                .size(px(FAB))
                .items_center()
                .justify_center()
                .rounded_full()
                .bg(move || theme().primary)
                .text_color(move || theme().primary_foreground)
                .shadow_lg()
                .transition()
                .duration(ui::SPATIAL_MS)
                .ease_out()
                .active(|st| st.scale(0.96))
                .on_press(move || {
                    if open() {
                        nav.close_search();
                    } else {
                        nav.adding.set(false);
                        nav.menu.set(false);
                        nav.letter.set(None);
                        nav.searching.set(true);
                        // Once the field is laid out: the keyboard comes up with the field opening.
                        set_timeout(std::time::Duration::from_millis(60), move || field.focus());
                    }
                })
                .child(
                    view()
                        .size(px(24.0))
                        .child(dynamic(move || icon(if open() { Icon::Close } else { Icon::Search }).size(px(24.0)).into_element())),
                ),
        )
}

fn chips(s: Store, nav: Nav) -> impl IntoElement {
    let select = move |id| {
        nav.letter.set(None);
        s.category.set(id);
        s.refresh();
    };
    // Chips scroll sideways with the finger; no indicator under them.
    scroll().horizontal().scrollbar(false).flex_shrink(0.0).child(
        view()
            .flex_row()
            .gap(px(8.0))
            .px(px(ui::MARGIN))
            .pb(px(4.0))
            .child(ui::chip(move || s.label("All"), move || s.category.get().is_none(), None).on_press(move || select(None)))
            .child(each(
                move || s.projection.with(|p| p.categories.clone()),
                |c| c.id,
                move |c, _| {
                    let id = c.with_untracked(|c| c.id);
                    ui::chip(move || c.with(|c| SharedString::from(c.name.clone())), move || s.category.get() == Some(id), None)
                        .on_press(move || select(Some(id)))
                },
            ))
            .child(
                ui::chip(move || s.label("Manage"), || false, Some(Icon::Sliders))
                    .test_id("manage")
                    .on_press(move || nav.push(Page::Categories)),
            ),
    )
}

/// The rows, as one group. A long press on a row opens its menu and picks it up: moving the finger then drags it to
/// another place (the menu closes), and the order is saved on release. A dragged row's place stays empty while a
/// lifted copy of it follows the finger.
fn list(s: Store, nav: Nav) -> impl IntoElement {
    let r = Reorder::new(s, Kind::Accounts, |_| (ui::GROUP_GAP, 1), Pickup::LongPress);
    // The index's letters, in its order: those the shown accounts file under.
    let letters = computed(move || {
        let mut letters: Vec<char> = s.projection.with(|p| p.rows.iter().map(|row| initials::initial(&row.issuer)).collect());
        letters.sort_by_key(|l| initials::order(*l));
        letters.dedup();
        letters
    });
    // Narrowed to a letter: the rows filed under it, in the list's order.
    let narrowed = computed(move || {
        let letter = nav.letter.get()?;
        Some(s.projection.with(|p| {
            p.rows.iter().enumerate().filter(|(_, row)| initials::initial(&row.issuer) == letter).map(|(i, _)| i).collect::<Vec<_>>()
        }))
    });
    // A letter with nothing left under it (its last account deleted or renamed) lets the list go.
    effect(move || {
        if narrowed.with(|n| n.as_ref().is_some_and(Vec::is_empty)) {
            untracked(|| nav.letter.set(None));
        }
    });
    let count = move || narrowed.with(|n| n.as_ref().map(Vec::len)).unwrap_or_else(|| r.count());
    let row_at = move |at: usize| match narrowed.with(|n| n.as_ref().map(|n| n.get(at).copied())) {
        Some(index) => index.and_then(|i| s.projection.with(|p| p.rows.get(i).cloned())),
        None => r.row(at),
    };
    effect(move || {
        if r.dragged.get().is_some() {
            untracked(|| nav.context.set(None));
        }
    });
    view()
        .flex_1()
        .child(
            virtual_list(count, move |at| {
                // The position is read in the bindings, not once here: a cell keeps its content as rows come and
                // go or the list scrolls, and shows whichever row is now at its place (its data, its corners).
                let row = computed(move || row_at(at.index()));
                account_row(s, nav, r, row, move || at.index(), count)
            })
            .keys(move || match narrowed.get() {
                Some(n) => s.projection.with(|p| n.iter().filter_map(|i| p.rows.get(*i)).map(|r| r.id.to_string()).collect::<Vec<_>>()),
                None => r.ids().iter().map(|id| id.to_string()).collect::<Vec<_>>(),
            })
            .estimated_item_size(move || px(ROW) * font_scale())
            .row_gap(px(ui::GROUP_GAP))
            .content_padding_x(px(ui::MARGIN))
            .content_padding_y(px(8.0))
            .footer(view().h(px(CLEARANCE)))
            .on_scroll(move |e: &ScrollEvent| {
                r.scrolled(e.offset.y.0);
                s.controller.get().activity()
            })
            .keyboard_dismiss_mode(KeyboardDismissMode::OnDrag)
            .element_ref(&r.list)
            .flex_1(),
        )
        .child(when(move || r.pointer.get().is_some() && r.standin.with(|row| row.is_some()), move || standin(r)))
        .child(when(
            move || !nav.searching.get() && r.dragged.get().is_none() && r.count() >= INDEXED && letters.with(|l| l.len() > 1),
            move || index(nav, r, letters),
        ))
}

fn account_row(
    s: Store,
    nav: Nav,
    r: Reorder,
    row: Memo<Option<Row>>,
    index: impl Fn() -> usize + Copy + 'static,
    count: impl Fn() -> usize + Copy + 'static,
) -> impl IntoElement {
    let id = move || row.with(|r| r.as_ref().map(|r| r.id));
    let issuer = move || row.with(|r| r.as_ref().map(|r| r.issuer.clone()).unwrap_or_default());
    let copied = move || s.copied.get().is_some() && s.copied.get() == id();
    let me = element_ref();
    let el = ui::group_corners(
        // A view, not a button: a row takes a press and a long press.
        view()
            .accessible(true)
            .role(nsh_core::AccessibilityRole::Button)
            .label(move || s.fmt("Copy code for {}", &[&issuer()]))
            .min_h(move || px(ROW) * font_scale())
            .px(px(16.0))
            .py(px(14.0))
            .flex_row()
            .items_center()
            .gap(px(14.0))
            // The flash of a copy: the container color at once, back to the row's own as it fades.
            .bg(move || if copied() { tok("primary-container") } else { theme().card })
            .transition()
            .duration(move || if copied() { 0.0 } else { ui::EFFECTS_MS })
            .ease_out()
            .active(|st| st.bg(move || tok("surface-high")))
            // Its place stays empty while it is dragged.
            .opacity(move || if r.is_dragging(id()) { 0.0_f32 } else { 1.0 })
            .on_press(move |_: &()| {
                if let Some(id) = id() {
                    haptic(Haptic::Notification(HapticNotification::Success));
                    s.copy(id)
                }
            })
            .on_long_press(move |e: &PressEvent| {
                if let Some(id) = id() {
                    haptic(Haptic::Impact(HapticImpact::Medium));
                    dismiss_keyboard();
                    // Narrowed to a letter, the list shows only some accounts: its order is changed in the whole list.
                    if nav.letter.get_untracked().is_none() {
                        r.arm(id);
                    }
                    nav.context.set(Some((id, e.page_position)))
                }
            }),
        index,
        count,
    );
    face(r.item(el, me, id), row)
}

/// What a row shows: the account's mark, its names, its code and the countdown.
fn face(el: View, row: Memo<Option<Row>>) -> View {
    let issuer = move || row.with(|r| r.as_ref().map(|r| r.issuer.clone()).unwrap_or_default());
    let urgent = move || row.with(|r| r.as_ref().is_some_and(|r| r.remaining <= 5));
    el.child(badge_with_icon(issuer, || false, move || row.with(|r| r.as_ref().and_then(|r| r.icon.clone())), 42.0))
        .child(view().flex_1().min_w(px(0.0)).child(ui::title(text(issuer)).number_of_lines(1)).child(
            ui::dim(ui::body(text(move || row.with(|r| r.as_ref().map(|r| r.username.clone()).unwrap_or_default())))).number_of_lines(1),
        ))
        .child(
            text(move || row.with(|r| r.as_ref().map(|r| r.code.clone()).unwrap_or_default()))
                .font_family(CODE_FONT.family)
                .font_weight(FontWeight::SEMIBOLD)
                .text_size(px(24.0))
                .letter_spacing(px(0.5))
                .number_of_lines(1)
                .flex_shrink(0.0)
                .text_color(move || if urgent() { tok("urgent") } else { theme().foreground }),
        )
        .child(ring(move || row.with(|r| r.as_ref().map(|r| (r.remaining, r.period))), 26.0))
}

/// The dragged row following the finger: the row's look, lifted by a shadow, its corners all the inner ones'.
fn standin(r: Reorder) -> impl IntoElement {
    let frame = move || r.pointer.get().unwrap_or_default();
    let row = computed(move || r.standin.get());
    face(
        view()
            .absolute()
            .left(move || px(frame().0))
            .top(move || px(frame().1))
            .w(move || px(frame().2))
            .h(move || px(frame().3))
            .px(px(16.0))
            .py(px(14.0))
            .flex_row()
            .items_center()
            .gap(px(14.0))
            .rounded(px(ui::GROUP_INNER * 2.0))
            .bg(move || theme().card)
            .shadow_lg()
            .pointer_events(nsh_core::PointerEvents::None),
        row,
    )
}

/// The letters beside the list: a press on one narrows the list to the accounts filed under it (`initials`), in the
/// list's order; sliding along the letters narrows it to each in turn, the letter showing large beside the finger. The
/// chosen letter is filled. A press on it again lets the list go, as do a category, the search and the back key.
fn index(nav: Nav, r: Reorder, letters: Memo<Vec<char>>) -> impl IntoElement {
    let bar = element_ref();
    let area = element_ref();
    let owner = Owner::current();
    // The letters' column and the list's area, in the window, measured as a touch begins.
    let frames = stored(None::<((f32, f32), f32)>);
    // The letter chosen before this touch, and whether the touch chose another.
    let before = stored(None::<char>);
    let changed = stored(false);
    let shown = signal(None::<(char, f32)>);
    let pick = move |y: f32| {
        let Some(((top, height), area_top)) = frames.get() else { return };
        let count = letters.with(Vec::len);
        if count == 0 || height <= 0.0 {
            return;
        }
        let at = (((y - top) / height * count as f32).floor().max(0.0) as usize).min(count - 1);
        let Some(letter) = letters.with(|l| l.get(at).copied()) else { return };
        if nav.letter.get_untracked() != Some(letter) {
            haptic(Haptic::Selection);
            nav.letter.set(Some(letter));
            r.list.scroll_to_offset(px(0.0), false);
        }
        if before.get() != Some(letter) {
            changed.replace(true);
        }
        shown.set(Some((letter, y - area_top)));
    };
    let lift = move || {
        // A press on the chosen letter, without choosing another meanwhile, lets the list go.
        if !changed.get() && before.get().is_some() && shown.get_untracked().map(|(l, _)| l) == before.get() {
            nav.letter.set(None);
        }
        shown.set(None);
    };
    view()
        .element_ref(&area)
        .absolute()
        .inset(px(0.0))
        .pointer_events(nsh_core::PointerEvents::BoxNone)
        .child(
            view()
                .absolute()
                .right(px(0.0))
                .top(px(0.0))
                .bottom(px(CLEARANCE / 2.0))
                .w(px(ui::MARGIN))
                .justify_center()
                .pointer_events(nsh_core::PointerEvents::BoxNone)
                .child(
                    view()
                        .element_ref(&bar)
                        .items_center()
                        .py(px(6.0))
                        .accessibility_elements_hidden(true)
                        .claim_touches(TouchClaim::on_start())
                        .keep_touches(true)
                        .on_responder_grant(move |e: &ResponderEvent| {
                            let y = e.page.y.0;
                            before.replace(nav.letter.get_untracked());
                            changed.replace(false);
                            owner.run(|| {
                                spawn(async move {
                                    if let (Some(b), Some(a)) = (bar.measure().await, area.measure().await) {
                                        frames.replace(Some(((b.page.y.0, b.frame.size.height.0), a.page.y.0)));
                                        pick(y);
                                    }
                                });
                            });
                        })
                        .on_responder_move(move |e: &ResponderEvent| pick(e.page.y.0))
                        .on_responder_release(move |_: &ResponderEvent| lift())
                        .on_responder_terminate(move |_: &ResponderEvent| shown.set(None))
                        .child(each(
                            move || letters.get(),
                            |l| *l,
                            move |l, _| {
                                let on = move || nav.letter.get() == Some(l.get());
                                view()
                                    .size(px(15.0))
                                    .items_center()
                                    .justify_center()
                                    .rounded_full()
                                    .bg(move || if on() { theme().primary } else { theme().primary.opacity(0.0) })
                                    .child(
                                        text(move || SharedString::from(l.get().to_string()))
                                            .text_size(px(10.0))
                                            .line_height(px(15.0))
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .text_color(move || if on() { theme().primary_foreground } else { theme().primary }),
                                    )
                            },
                        )),
                ),
        )
        .child(when(
            move || shown.get().is_some(),
            move || {
                view()
                    .absolute()
                    .right(px(ui::MARGIN + 16.0))
                    .top(move || px(shown.get().map_or(0.0, |(_, y)| y) - 28.0))
                    .size(px(56.0))
                    .items_center()
                    .justify_center()
                    .rounded_full()
                    .bg(move || theme().primary)
                    .shadow_md()
                    .child(
                        text(move || SharedString::from(shown.get().map(|(l, _)| l.to_string()).unwrap_or_default()))
                            .text_size(px(26.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(move || theme().primary_foreground),
                    )
            },
        ))
}

/// The countdown: a track, an arc that shortens clockwise from the top, and the seconds left inside. Orange in the last
/// five seconds.
fn ring(row: impl Fn() -> Option<(u32, u32)> + Copy + 'static, size: f32) -> impl IntoElement {
    let full = size + 2.5;
    let r = size / 2.0;
    let circle = move |extra: String| {
        nsh_core::element::ImageSrc::svg_markup(&format!(
            "<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 {full} {full}' fill='none' stroke-width='2.5'><circle cx='{c}' cy='{c}' r='{r}' stroke='currentColor' {extra}/></svg>",
            c = full / 2.0
        ))
    };
    let track = circle(String::new());
    let urgent = move || row().is_some_and(|(left, _)| left <= 5);
    let ink = move || if urgent() { tok("urgent") } else { theme().primary };
    view()
        .size(px(size))
        .flex_shrink(0.0)
        .items_center()
        .justify_center()
        .child(svg(move || track.clone()).absolute().left(px(-1.25)).top(px(-1.25)).size(px(full)).text_color(move || theme().border))
        .child(
            svg(move || {
                let (remaining, period) = row().unwrap_or((0, 30));
                let circumference = 2.0 * std::f32::consts::PI * r;
                let visible = circumference * (remaining as f32 / period.max(1) as f32).clamp(0.02, 1.0);
                let c = full / 2.0;
                circle(format!("stroke-linecap='round' stroke-dasharray='{visible:.3} {circumference:.3}' transform='rotate(-90 {c} {c})'"))
            })
            .absolute()
            .left(px(-1.25))
            .top(px(-1.25))
            .size(px(full))
            .text_color(ink),
        )
        .child(
            text(move || SharedString::from(row().map(|(left, _)| left.to_string()).unwrap_or_default()))
                .text_size(px(size * 0.4))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(move || if urgent() { tok("urgent") } else { theme().muted_foreground }),
        )
}

/// No account yet: what to do first.
fn empty(s: Store, nav: Nav) -> impl IntoElement {
    view()
        .flex_1()
        .items_center()
        .justify_center()
        .p(px(32.0))
        .pb_safe_area_plus(px(32.0))
        .child(view().text_color(move || theme().primary).child(icon(Icon::QrScan).size(px(56.0))))
        .child(ui::title_large(text(move || s.label("No 2FA accounts yet"))).text_center().mt(px(16.0)))
        .child(
            ui::dim(ui::body(text(move || s.label("Enter the setup key from a service. Codes are encrypted on this device."))))
                .text_center()
                .mt(px(8.0)),
        )
        .child(
            view().mt(px(24.0)).child(
                ui::filled_button(Some(Icon::Keyboard), move || s.label("Enter Setup Key"))
                    .test_id("enter-key")
                    .on_press(move || nav.push(Page::Manual)),
            ),
        )
}

/// Accounts there are, none to show: nothing matches, or the category is empty.
fn nothing(s: Store) -> impl IntoElement {
    let searching = move || !s.query.get().trim().is_empty();
    view()
        .flex_1()
        .items_center()
        .justify_center()
        .p(px(32.0))
        // Centered in what the keyboard leaves: a search with no match is typed with the keyboard up.
        .pb_keyboard_plus(px(32.0))
        .child(
            view()
                .size(px(48.0))
                .text_color(move || theme().muted_foreground)
                .child(dynamic(move || icon(if searching() { Icon::Search } else { Icon::Folder }).size(px(48.0)).into_element())),
        )
        .child(
            ui::title(text(move || {
                if searching() {
                    return s.label("No matching accounts");
                }
                let name = s.projection.with(|p| p.categories.iter().find(|c| Some(c.id) == s.category.get()).map(|c| c.name.clone()));
                s.fmt("No accounts in “{}” yet", &[&name.unwrap_or_default()])
            }))
            .text_center()
            .mt(px(12.0)),
        )
}

/// The add button's menu, under it.
fn add_menu(s: Store, nav: Nav) -> impl IntoElement {
    when(
        move || nav.adding.get(),
        move || {
            view().absolute().inset(px(0.0)).on_press(move |_: &()| nav.adding.set(false)).child(
                dropdown()
                    .child(menu_item(s, Icon::Photo, "Choose from Photos", move || {
                        nav.adding.set(false);
                        s.scanned.set(Default::default());
                        s.choose_images()
                    }))
                    .child(menu_item(s, Icon::Keyboard, "Enter Setup Key", move || nav.push(Page::Manual)))
                    .child(menu_item(s, Icon::Doc, "Import Backup File", move || {
                        nav.adding.set(false);
                        s.choose_backup()
                    })),
            )
        },
    )
}
/// A menu under the top bar's buttons.
fn dropdown() -> View {
    ui::popover(
        view()
            .absolute()
            .right(px(ui::MARGIN))
            .top(px(0.0))
            .mt_safe_area_plus(px(HEADER - 4.0))
            .min_w(px(220.0))
            .py(px(8.0))
            .rounded(px(16.0)),
    )
}

/// The menu of the top bar's dots.
fn overflow(s: Store, nav: Nav) -> impl IntoElement {
    when(
        move || nav.menu.get(),
        move || {
            view().absolute().inset(px(0.0)).on_press(move |_: &()| nav.menu.set(false)).child(
                dropdown()
                    .child(menu_item(s, Icon::Settings, "Settings", move || nav.push(Page::Settings)))
                    .child(menu_item(s, Icon::Sliders, "Manage Categories", move || nav.push(Page::Categories)))
                    .child(menu_item(s, Icon::Lock, "Lock Now", move || {
                        nav.menu.set(false);
                        s.lock()
                    })),
            )
        },
    )
}
fn menu_item(s: Store, glyph: Icon, label_: &'static str, press: impl Fn() + 'static) -> impl IntoElement {
    button()
        .test_id(label_)
        .h(px(48.0))
        .px(px(12.0))
        .flex_row()
        .items_center()
        .gap(px(12.0))
        .active(|st| st.bg(move || tok("pressed")))
        .on_press(press)
        .child(view().text_color(move || theme().muted_foreground).child(icon(glyph).size(px(24.0))))
        .child(ui::label(text(move || s.label(label_))))
}
