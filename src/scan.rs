//! "Select on Screen": Safay's own selection over every display, in place of the system's screenshot crosshair.
//!
//! Each display is pictured once before anything shows ([`screen_capture::snapshot`]); an overlay window then covers
//! it, dims the screen and finds the 2FA QR codes in the picture. A found code is outlined and named (issuer only, never
//! its secret), or marked when the vault already has it; a click on it or its capsule picks it, and a drag picks any
//! area. Esc cancels. The pick is cut from the same picture,
//! so what the user saw is what is read. The overlays are non-activating panels, so Safay's other windows stay where
//! they are and the app in front keeps the keyboard; Esc is a system-wide shortcut while they show.
use crate::{
    components::{app_icon, tok},
    i18n::{self, Language},
    icons::{Icon, symbol_weight},
    platform::screen_capture::{self, Found, Rect, Shot},
};
use nsh_base::theme;
use nsh_core::{
    PointerEvents,
    desktop::{WindowAttributes, WindowFrameStyle, WindowKind, WindowLevel, WindowPosition, Workspaces},
    element::desktop::{WindowRef, displays, open_window, register_global_shortcut, unregister_global_shortcut, window},
    prelude::*,
};
use safay_app::session::Error;
use std::{
    rc::Rc,
    sync::{Arc, mpsc},
    time::Duration,
};

/// Space around a found code's outline, and the least drag that counts as a selection (points).
const RING: f32 = 10.0;
const MIN_DRAG: f32 = 12.0;
/// A found code's outline: its stroke and corner radius.
const OUTLINE: f32 = 3.0;
const OUTLINE_RADIUS: f32 = 6.0;
/// How far the pointer moves before a press becomes a drag.
const SLOP: f32 = 4.0;

enum Outcome {
    Cancel,
    Pick(usize, Rect),
}

/// Which of a code's accounts the vault already has (`Session::known_accounts`), asked off the engine thread.
pub type Known = Arc<dyn Fn(&str) -> Vec<bool> + Send + Sync>;

/// Lets the user pick a QR code on screen: the picked area as a PNG for the QR reader, `None` when they canceled. Falls
/// back to the system's interactive screenshot when the displays cannot be pictured.
pub async fn select(language: Language, known: Known) -> Result<Option<Vec<u8>>, Error> {
    thread_local! {
        static SHOWING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    }
    // One selection at a time: the capture shortcut pressed again while it shows does nothing.
    if SHOWING.with(|s| s.replace(true)) {
        return Ok(None);
    }
    let result = run(language, known).await;
    SHOWING.with(|s| s.set(false));
    result
}

async fn run(language: Language, known: Known) -> Result<Option<Vec<u8>>, Error> {
    let screens = displays();
    let bounds: Vec<Rect> = screens
        .iter()
        .map(|d| Rect { x: d.bounds.origin.x.0, y: d.bounds.origin.y.0, w: d.bounds.size.width.0, h: d.bounds.size.height.0 })
        .collect();
    if bounds.is_empty() {
        return screen_capture::capture().await;
    }
    let pictured = bounds.clone();
    let shots = blocking(move || screen_capture::snapshot(&pictured)).await.map_err(|_| Error::Storage)??;
    if shots.iter().all(Option::is_none) {
        return screen_capture::capture().await;
    }
    let shots: Arc<Vec<Option<Shot>>> = Arc::new(shots);
    let under = pointer_display(&bounds);
    let (sender, outcome) = mpsc::channel::<Outcome>();
    let sender = Rc::new(sender);
    let codes: Vec<Signal<Option<Vec<Found>>>> = bounds.iter().map(|_| signal(None)).collect();
    // Where each display's visible area starts under its top (the menu bar): the instructions sit there.
    let tops: Vec<f32> = screens.iter().map(|d| (d.visible_bounds.origin.y - d.bounds.origin.y).0).collect();
    let mut windows: Vec<WindowRef> = Vec::new();
    crate::desktop::in_root(|| {
        for (i, b) in bounds.iter().enumerate() {
            let (send, closed) = (sender.clone(), sender.clone());
            let (codes, area, hud, top) = (codes[i], (b.w, b.h), i == under, tops[i]);
            let mut attributes = WindowAttributes::default();
            attributes.frame = Some(WindowFrameStyle::None);
            attributes.level = Some(WindowLevel::AboveDock);
            attributes.transparent = Some(true);
            attributes.shadow = Some(false);
            attributes.activate = Some(false);
            attributes.close_on_escape = Some(true);
            attributes.close_on_blur = Some(false);
            attributes.workspaces = Some(Workspaces::All);
            attributes.resizable = Some(false);
            attributes.movable = Some(false);
            let opened = open_window(
                window()
                    .id(format!("scan-{i}"))
                    .kind(WindowKind::Popup)
                    .default_size(size(px(b.w), px(b.h)))
                    .default_position(WindowPosition::At(point(px(b.x), px(b.y))))
                    .attributes(attributes)
                    .on_open_change(move |_| drop(closed.send(Outcome::Cancel)))
                    .content(move || overlay(i, area, top, codes, hud, language, send.clone())),
            );
            if let Some(opened) = opened {
                if hud {
                    opened.focus();
                }
                windows.push(opened);
            }
        }
    });
    if windows.is_empty() {
        return screen_capture::capture().await;
    }
    // A panel of an app in the background never gets keys: the app in front keeps the keyboard. So Esc is Safay's
    // system-wide while the overlays show.
    let cancel = sender.clone();
    drop(register_global_shortcut("escape", move || drop(cancel.send(Outcome::Cancel))));
    for (i, codes) in codes.iter().copied().enumerate() {
        let (shots, known) = (shots.clone(), known.clone());
        spawn(async move {
            let found = blocking(move || shots[i].as_ref().map(|shot| screen_capture::find(shot, &*known)).unwrap_or_default())
                .await
                .unwrap_or_default();
            codes.set(Some(found));
        });
    }
    let chosen = blocking(move || outcome.recv()).await;
    unregister_global_shortcut("escape");
    for w in &windows {
        w.close();
    }
    match chosen {
        Ok(Ok(Outcome::Pick(i, rect))) => blocking(move || shots[i].as_ref().map(|shot| screen_capture::crop(shot, rect)).transpose())
            .await
            .map_err(|_| Error::Storage)?,
        _ => Ok(None),
    }
}

/// The display under the pointer (the first when unknown): it shows the instructions and takes the keyboard.
fn pointer_display(bounds: &[Rect]) -> usize {
    let Some((x, y)) = crate::platform::macos::pointer() else { return 0 };
    // AppKit's screen coordinates start at the primary display's bottom left; NSH's at its top left.
    let primary = displays().into_iter().find(|d| d.primary).map(|d| d.bounds.size.height.0).unwrap_or(0.0);
    let (x, y) = (x as f32, primary - y as f32);
    bounds.iter().position(|b| b.contains(x, y)).unwrap_or(0)
}

/// The outline around a found code.
fn ring(found: &Found) -> Rect {
    found.rect.inset(-RING)
}
/// What is read for a found code: it with a quiet zone around it.
fn pick_area(found: &Found) -> Rect {
    found.rect.inset(-(found.rect.w.max(found.rect.h) * 0.2).max(12.0))
}
/// The drag's frame from `a` to `b`, on whole points like the dimming around it.
fn between(a: (f32, f32), b: (f32, f32)) -> Rect {
    whole(Rect { x: a.0.min(b.0), y: a.1.min(b.1), w: (a.0 - b.0).abs(), h: (a.1 - b.1).abs() })
}
/// `r` with its edges on whole points: on the pixel grid at any scale, so the dimming's pieces meet without a seam
/// (edges inside a pixel are antialiased, and two of them leave a light line across the screen).
fn whole(r: Rect) -> Rect {
    let (x, y) = (r.x.round(), r.y.round());
    Rect { x, y, w: (r.x + r.w).round() - x, h: (r.y + r.h).round() - y }
}

/// The parts of `area` outside every hole, as rectangles: bands between the holes' top and bottom edges, each cut
/// around the holes that cross it.
fn cover(area: Rect, holes: &[Rect]) -> Vec<Rect> {
    let holes: Vec<Rect> = holes.iter().copied().map(whole).collect();
    let holes = holes.as_slice();
    let bottom = area.y + area.h;
    let mut ys = vec![area.y, bottom];
    for h in holes {
        ys.push(h.y.clamp(area.y, bottom));
        ys.push((h.y + h.h).clamp(area.y, bottom));
    }
    ys.sort_by(f32::total_cmp);
    ys.dedup();
    let mut out = Vec::new();
    for band in ys.windows(2) {
        let (top, low) = (band[0], band[1]);
        if low <= top {
            continue;
        }
        let mut cuts: Vec<(f32, f32)> =
            holes.iter().filter(|h| h.y < low && h.y + h.h > top).map(|h| (h.x.max(area.x), (h.x + h.w).min(area.x + area.w))).collect();
        cuts.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut x = area.x;
        for (from, to) in cuts {
            if from > x {
                out.push(Rect { x, y: top, w: from - x, h: low - top });
            }
            x = x.max(to);
        }
        if x < area.x + area.w {
            out.push(Rect { x, y: top, w: area.x + area.w - x, h: low - top });
        }
    }
    out
}

fn black(alpha: f32) -> nsh_core::Rgba {
    nsh_core::Rgba { r: 0.0, g: 0.0, b: 0.0, a: alpha }
}
fn white(alpha: f32) -> nsh_core::Rgba {
    nsh_core::Rgba { r: 1.0, g: 1.0, b: 1.0, a: alpha }
}
/// A rectangle as a key: whole points.
type Key = (i32, i32, i32, i32);
/// Each found code's capsule as laid out, by the code's key.
type Capsules = Signal<Vec<(Key, Rect)>>;
fn key(r: &Rect) -> Key {
    (r.x as i32, r.y as i32, r.w as i32, r.h as i32)
}
fn at(el: View, r: Rect) -> View {
    el.absolute().left(px(r.x)).top(px(r.y)).w(px(r.w)).h(px(r.h))
}

/// One display's overlay: the dimmed screen, the found codes, the drag's frame, Safay's badge by the pointer and, on the
/// display under the pointer, the instructions.
#[allow(clippy::too_many_arguments)]
fn overlay(
    index: usize,
    area: (f32, f32),
    top: f32,
    codes: Signal<Option<Vec<Found>>>,
    hud: bool,
    language: Language,
    send: Rc<mpsc::Sender<Outcome>>,
) -> impl IntoElement {
    let hover = signal(None::<(f32, f32)>);
    let start = signal(None::<(f32, f32)>);
    let drag = signal(None::<Rect>);
    // Each found code's capsule as laid out, in the overlay's points: it picks the code too.
    let capsules: Capsules = signal(Vec::new());
    let shown = signal(reduce_motion());
    set_timeout(Duration::from_millis(16), move || shown.set(true));
    let found = move || codes.with(|c| c.clone().unwrap_or_default());
    let code_at = move |x: f32, y: f32| {
        found()
            .into_iter()
            .find(|f| ring(f).contains(x, y) || capsules.with(|c| c.iter().any(|(k, r)| *k == key(&f.rect) && r.contains(x, y))))
    };
    let hot = move || hover.get().and_then(|(x, y)| code_at(x, y)).map(|f| f.rect);
    let whole = Rect { x: 0.0, y: 0.0, w: area.0, h: area.1 };
    let dims = move || {
        // Inside the outline's stroke, so its rounded corners hide the hole's square ones.
        let mut holes: Vec<Rect> = found().iter().map(|f| ring(f).inset(OUTLINE / 2.0)).collect();
        holes.extend(drag.get());
        cover(whole, &holes)
    };
    // Where Cancel is laid out, in the overlay's points.
    let cancel_at = signal(None::<Rect>);
    // The overlay takes every press (a panel of an app in the background passes a click on to its buttons unreliably)
    // and, on release, picks the dragged area, Cancel or the code under the pointer.
    let release = {
        let send = send.clone();
        move |e: &ResponderEvent| {
            let picked = drag.get_untracked();
            start.set(None);
            drag.set(None);
            let (x, y) = (e.page.x.0, e.page.y.0);
            match picked {
                Some(r) if r.w >= MIN_DRAG && r.h >= MIN_DRAG => drop(send.send(Outcome::Pick(index, r))),
                _ if cancel_at.get_untracked().is_some_and(|r| r.contains(x, y)) => drop(send.send(Outcome::Cancel)),
                _ => {
                    if let Some(f) = code_at(x, y) {
                        drop(send.send(Outcome::Pick(index, pick_area(&f))));
                    }
                }
            }
        }
    };
    // The pointer over Cancel: it lights up (macOS keeps the cursor of an app in the background).
    let over_cancel = move || hover.get().zip(cancel_at.get()).is_some_and(|((x, y), r)| r.contains(x, y));
    // The buttons' own presses: assistive technologies press them.
    let pick: Rc<dyn Fn(&Found)> = {
        let send = send.clone();
        Rc::new(move |f: &Found| drop(send.send(Outcome::Pick(index, pick_area(f)))))
    };
    let cancel = {
        let send = send.clone();
        move || drop(send.send(Outcome::Cancel))
    };
    view()
        .flex_1()
        .cursor(Cursor::Crosshair)
        .opacity(move || if shown.get() { 1.0_f32 } else { 0.0 })
        .transition_opacity()
        .duration(160.0)
        .claim_touches(TouchClaim::on_start())
        .on_responder_grant(move |e: &ResponderEvent| {
            start.set(Some((e.page.x.0, e.page.y.0)));
            drag.set(None);
        })
        .on_responder_move(move |e: &ResponderEvent| {
            let p = (e.page.x.0, e.page.y.0);
            hover.set(Some(p));
            if let Some(s) = start.get_untracked()
                && (drag.get_untracked().is_some() || (p.0 - s.0).hypot(p.1 - s.1) >= SLOP)
            {
                drag.set(Some(between(s, p)));
            }
        })
        .on_responder_release(release)
        .on_responder_terminate(move |_: &ResponderEvent| {
            start.set(None);
            drag.set(None);
        })
        .on_mouse_move(move |e: &MouseEvent| hover.set(Some((e.position.x.0, e.position.y.0))))
        .on_mouse_leave(move |_: &MouseEvent| hover.set(None))
        .child(each(dims, key, |r, _| at(view(), r.get_untracked()).bg(black(0.45)).pointer_events(PointerEvents::None)))
        .child(each(found, |f| key(&f.rect), move |f, _| code_box(f.get_untracked(), hot, capsules, language, pick.clone())))
        .child(when(move || drag.get().is_some(), move || selection(move || drag.get().unwrap_or(whole))))
        .child(when(
            move || hover.get().is_some() && hot().is_none() && !over_cancel(),
            move || badge(move || hover.get().unwrap_or_default()),
        ))
        .child(when(move || hud, move || instructions(codes, top, language, cancel_at, over_cancel, cancel.clone())))
}

/// A found code: its outline (brighter under the pointer) and a capsule with what a click does. Both take the click
/// (the overlay's responder picks the code) and show the pointing hand.
fn code_box(
    f: Found,
    hot: impl Fn() -> Option<Rect> + Copy + 'static,
    capsules: Capsules,
    language: Language,
    pick: Rc<dyn Fn(&Found)>,
) -> impl IntoElement {
    let r = ring(&f);
    let id = key(&f.rect);
    let (on_box, on_capsule) = {
        let (a, b, fa, fb) = (pick.clone(), pick, f.clone(), f.clone());
        (move || a(&fa), move || b(&fb))
    };
    let lit = move || hot() == Some(f.rect);
    let existing = f.existing;
    let label: SharedString = if existing {
        i18n::tr(language, "Already in Safay")
    } else if f.accounts > 1 {
        i18n::tr(language, "add_n.other").replacen("{}", &f.accounts.to_string(), 1).into()
    } else if f.issuer.trim().is_empty() {
        i18n::tr(language, "Add Account")
    } else {
        i18n::tr(language, "Add {}").replacen("{}", f.issuer.trim(), 1).into()
    };
    // Where the capsule's row starts: centered under the outline, wide enough for a long name.
    let row = (r.x - 80.0, r.y + r.h + 12.0);
    view()
        .absolute()
        .left(px(r.x))
        .top(px(r.y))
        .w(px(r.w))
        .h(px(r.h))
        .pointer_events(PointerEvents::BoxNone)
        .child(
            view()
                .absolute()
                .inset(px(-4.0))
                .rounded(px(OUTLINE_RADIUS + 4.0))
                .border(px(4.0))
                .pointer_events(PointerEvents::None)
                .border_color(move || tok("code-accent").opacity(if lit() { 0.45 } else { 0.25 })),
        )
        .child(
            view()
                .absolute()
                .inset(px(0.0))
                .rounded(px(OUTLINE_RADIUS))
                .border(px(OUTLINE))
                .cursor(Cursor::Pointer)
                .on_press(on_box)
                .border_color(move || tok("code-accent"))
                .bg(move || tok("code-accent").opacity(if lit() { 0.16 } else { 0.0 })),
        )
        .child(
            view()
                .absolute()
                .top(px(r.h + 12.0))
                .left(px(-80.0))
                .right(px(-80.0))
                .items_center()
                .pointer_events(PointerEvents::BoxNone)
                .child(
                    view()
                        .flex_row()
                        .items_center()
                        .gap(px(6.0))
                        .h(px(30.0))
                        .px(px(12.0))
                        .rounded_full()
                        .cursor(Cursor::Pointer)
                        .on_press(on_capsule)
                        .on_layout(move |b: &Bounds<Points>| {
                            let at = Rect { x: row.0 + b.origin.x.0, y: row.1 + b.origin.y.0, w: b.size.width.0, h: b.size.height.0 };
                            capsules.update(|c| {
                                c.retain(|(k, _)| *k != id);
                                c.push((id, at));
                            });
                        })
                        .bg(move || {
                            if existing {
                                tok("glass").opacity(if lit() { 1.0 } else { 0.94 })
                            } else {
                                tok("code-accent").opacity(if lit() { 1.0 } else { 0.92 })
                            }
                        })
                        .child(symbol_weight(if existing { Icon::CheckCircleFill } else { Icon::PlusCircle }, 13.0, 0.4, move || {
                            if existing { tok("success") } else { white(1.0).into() }
                        }))
                        .child(text(label).number_of_lines(1).text_size(px(13.0)).font_weight(FontWeight::SEMIBOLD).text_color(
                            move || {
                                if existing { theme().foreground } else { white(1.0).into() }
                            },
                        )),
                ),
        )
}

/// The frame of a drag: the area stays bright, with viewfinder corners.
fn selection(rect: impl Fn() -> Rect + Copy + 'static) -> impl IntoElement {
    let corner = move |left: bool, top: bool| {
        view()
            .absolute()
            .size(px(18.0))
            .left(move || px(if left { rect().x - 3.0 } else { rect().x + rect().w - 15.0 }))
            .top(move || px(if top { rect().y - 3.0 } else { rect().y + rect().h - 15.0 }))
            .border_color(move || tok("code-accent"))
            .border_t(px(if top { 4.0 } else { 0.0 }))
            .border_b(px(if top { 0.0 } else { 4.0 }))
            .border_l(px(if left { 4.0 } else { 0.0 }))
            .border_r(px(if left { 0.0 } else { 4.0 }))
            .rounded(px(4.0))
    };
    view()
        .absolute()
        .inset(px(0.0))
        .pointer_events(PointerEvents::None)
        .child(
            view()
                .absolute()
                .left(move || px(rect().x))
                .top(move || px(rect().y))
                .w(move || px(rect().w))
                .h(move || px(rect().h))
                .border(px(1.0))
                .border_color(white(0.85)),
        )
        .child(corner(true, true))
        .child(corner(false, true))
        .child(corner(true, false))
        .child(corner(false, false))
}

/// What sits by the pointer (macOS keeps the arrow for an app in the background): the area-selection symbol and Safay's
/// name, whose selection this is.
fn badge(at: impl Fn() -> (f32, f32) + Copy + 'static) -> impl IntoElement {
    view()
        .absolute()
        .left(move || px(at().0 + 10.0))
        .top(move || px(at().1 + 20.0))
        .flex_row()
        .items_center()
        .gap(px(6.0))
        .h(px(26.0))
        .pl(px(6.0))
        .pr(px(9.0))
        .rounded_full()
        .pointer_events(PointerEvents::None)
        .bg(move || tok("glass"))
        .border(px(0.5))
        .border_color(move || tok("glass-border"))
        .child(viewfinder(14.0))
        .child(view().w(px(1.0)).h(px(14.0)).bg(move || tok("glass-border")))
        .child(app_icon(16.0).rounded(px(4.0)))
        .child(text("Safay").text_size(px(12.0)).font_weight(FontWeight::SEMIBOLD).text_color(move || theme().foreground))
}

/// The area-selection symbol: a square viewfinder, four rounded corner strokes `size` points apart.
fn viewfinder(size: f32) -> impl IntoElement {
    const STROKE: f32 = 1.6;
    let arm = size * 0.36;
    let corner = move |left: bool, top: bool| {
        view()
            .absolute()
            .size(px(arm))
            .left(px(if left { 0.0 } else { size - arm }))
            .top(px(if top { 0.0 } else { size - arm }))
            .border_color(move || theme().foreground)
            .border_t(px(if top { STROKE } else { 0.0 }))
            .border_b(px(if top { 0.0 } else { STROKE }))
            .border_l(px(if left { STROKE } else { 0.0 }))
            .border_r(px(if left { 0.0 } else { STROKE }))
            .rounded(px(STROKE * 1.5))
    };
    view().size(px(size)).child(corner(true, true)).child(corner(false, true)).child(corner(true, false)).child(corner(false, false))
}

/// What Safay is doing and what the user can do, at the top of the display where the desktop notice shows: Esc or a
/// click on Cancel cancels.
fn instructions(
    codes: Signal<Option<Vec<Found>>>,
    top: f32,
    language: Language,
    cancel_at: Signal<Option<Rect>>,
    lit: impl Fn() -> bool + Copy + 'static,
    cancel: impl Fn() + 'static,
) -> impl IntoElement {
    // Cancel's place in the overlay: the card's in the row, plus Cancel's in the card.
    let row = top + crate::desktop::NOTICE_TOP;
    let card = signal((0.0_f32, 0.0_f32));
    let button = signal(None::<Rect>);
    effect(move || {
        let (cx, cy) = card.get();
        cancel_at.set(button.get().map(|b| Rect { x: cx + b.x, y: row + cy + b.y, w: b.w, h: b.h }));
    });
    let tr = move |key: &str| i18n::tr(language, key);
    let title = move || -> SharedString {
        match codes.with(|c| c.as_ref().map(Vec::len)) {
            None => tr("Safay is looking for 2FA QR codes…"),
            Some(0) => tr("Select a 2FA QR code"),
            Some(n) => {
                let form = if n == 1 { "scan_found.one" } else { "scan_found.other" };
                tr(form).replacen("{}", &n.to_string(), 1).into()
            }
        }
    };
    let hint = move || -> SharedString {
        match codes.with(|c| c.as_ref().map(Vec::len)) {
            Some(n) if n > 0 => tr("Click a code to add it, or drag over another one"),
            _ => tr("Drag over a QR code to add it to Safay"),
        }
    };
    view().absolute().left(px(0.0)).right(px(0.0)).top(px(row)).items_center().pointer_events(PointerEvents::BoxNone).child(
        crate::components::desktop_card(None, title, move || Some(hint()))
            .on_layout(move |b: &Bounds<Points>| card.set((b.origin.x.0, b.origin.y.0)))
            .child(view().w(px(1.0)).h(px(28.0)).ml(px(4.0)).bg(move || tok("glass-border")))
            .child(
                view()
                    .flex_row()
                    .items_center()
                    .gap(px(6.0))
                    .h(px(30.0))
                    .px(px(8.0))
                    .mr(px(-8.0))
                    .rounded(px(8.0))
                    .bg(move || theme().foreground.opacity(if lit() { 0.07 } else { 0.0 }))
                    .cursor(Cursor::Pointer)
                    .on_layout(move |b: &Bounds<Points>| {
                        button.set(Some(Rect { x: b.origin.x.0, y: b.origin.y.0, w: b.size.width.0, h: b.size.height.0 }))
                    })
                    .on_press(cancel)
                    .child(
                        view()
                            .h(px(20.0))
                            .px(px(6.0))
                            .rounded(px(5.0))
                            .justify_center()
                            .border(px(1.0))
                            .border_color(move || tok("glass-border"))
                            .bg(move || theme().foreground.opacity(0.06))
                            .child(
                                text("esc")
                                    .text_size(px(11.0))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(move || theme().muted_foreground),
                            ),
                    )
                    .child(
                        text(tr("Cancel"))
                            .text_size(px(12.0))
                            .text_color(move || if lit() { theme().foreground } else { theme().muted_foreground }),
                    ),
            ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn area(rects: &[Rect]) -> f32 {
        rects.iter().map(|r| r.w * r.h).sum()
    }
    #[test]
    fn cover_leaves_out_every_hole() {
        let screen = Rect { x: 0.0, y: 0.0, w: 100.0, h: 80.0 };
        let holes = [Rect { x: 10.0, y: 10.0, w: 20.0, h: 20.0 }, Rect { x: 25.0, y: 20.0, w: 30.0, h: 30.0 }];
        let parts = cover(screen, &holes);
        // The union of the holes: 400 + 900 - the 5 × 10 overlap.
        assert_eq!(area(&parts), 100.0 * 80.0 - (400.0 + 900.0 - 50.0));
        for p in &parts {
            for h in &holes {
                let overlap_w = (p.x + p.w).min(h.x + h.w) - p.x.max(h.x);
                let overlap_h = (p.y + p.h).min(h.y + h.h) - p.y.max(h.y);
                assert!(overlap_w <= 0.0 || overlap_h <= 0.0, "{p:?} covers {h:?}");
            }
        }
    }
    #[test]
    fn cover_without_holes_is_the_screen() {
        let screen = Rect { x: 0.0, y: 0.0, w: 50.0, h: 40.0 };
        assert_eq!(cover(screen, &[]), vec![screen]);
    }
    #[test]
    fn pieces_meet_on_whole_points() {
        let screen = Rect { x: 0.0, y: 0.0, w: 100.0, h: 80.0 };
        let parts = cover(screen, &[Rect { x: 10.4, y: 20.6, w: 30.3, h: 15.2 }]);
        for p in &parts {
            for v in [p.x, p.y, p.x + p.w, p.y + p.h] {
                assert_eq!(v, v.round(), "{p:?}");
            }
        }
        // The hole rounds out to (10, 21)–(41, 36): 31 × 15.
        assert_eq!(area(&parts), 100.0 * 80.0 - 31.0 * 15.0);
    }
    #[test]
    fn a_drag_frames_both_ways() {
        assert_eq!(between((30.0, 5.0), (10.0, 25.0)), Rect { x: 10.0, y: 5.0, w: 20.0, h: 20.0 });
    }
}
