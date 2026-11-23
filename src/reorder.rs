//! Drag-to-reorder for the accounts and the categories, one session per container: the desktop's cards, rows and
//! sidebar, and the phone's list and "Manage Categories".
//!
//! How an item is picked up is the session's `Pickup`. With a pointer (`Pickup::Move`), an item claims the touches once
//! the pointer has moved 4pt from where it went down (a click still copies). On a touch screen a move scrolls, so an
//! item is picked up by a long press first (`Pickup::LongPress`): the press arms it, and it claims the touches once the
//! finger then moves 4pt. Either way it keeps them against the list's scrolling. At the grant the list and the dragged item are measured once; from those
//! the grid (slot pitch, columns, where slot 0 sits) is known, and every move is hit-tested against it in the list's
//! content coordinates, so nothing is measured again while dragging.
//!
//! The order previews live as the pointer passes over items (only the session's `order` changes, never the store) and
//! is decided on release: inside the list it is saved once, outside it (the toolbar, the sidebar, outside the window)
//! or after Escape the original order comes back. Hit-testing uses enter / leave semantics: nothing is re-evaluated
//! while the pointer stays on the same item, and the dragged item moves after the target when dragged down, before it
//! when dragged up. A stand-in of the item follows the pointer while its slot stays empty. 44pt zones at the top and
//! bottom of the account list scroll it every 160 ms; at the very end the item moves to the first or last place. The
//! account stand-in stays inside the list; the categories' block has no scroll zones, and dropping on the header, on
//! "All" or outside the sidebar reverts.
use crate::store::Store;
use nsh_core::{
    element::{LayoutAnimation, LayoutCurve, LayoutTransition},
    prelude::*,
};
use safay_app::{Uuid, session::Row};
use std::time::Duration;

/// How far the pointer moves before a press becomes a drag.
const SLOP: f32 = 4.0;
/// The auto-scroll zones' height, and the beat while the pointer rests in one.
pub const EDGE_ZONE: f32 = 44.0;
const BEAT: Duration = Duration::from_millis(160);
/// The swap's motion: 0.18 s for accounts, 0.16 s for categories.
pub const SWAP_MS: f32 = 180.0;
pub const CATEGORY_SWAP_MS: f32 = 160.0;
/// The gap between category rows.
pub const CATEGORY_GAP: f32 = 2.0;

/// How an item is picked up for dragging.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Pickup {
    /// Moving the pointer (a mouse, a trackpad) past the slop.
    Move,
    /// A long press, then moving past the slop: a touch screen, where a move alone scrolls.
    LongPress,
}

/// How the container lays its items out, read at the grant: the gap between items and the columns.
pub type Layout = fn(Store) -> (f32, usize);

/// What a session orders.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    /// The shown accounts (cards or rows) of the current section.
    Accounts,
    /// The sidebar's categories.
    Categories,
}
impl Kind {
    pub fn swap_ms(self) -> f32 {
        match self {
            Kind::Accounts => SWAP_MS,
            Kind::Categories => CATEGORY_SWAP_MS,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Edge {
    Up,
    Down,
}

/// The list's slots, measured at the grant, in the list's content coordinates (scrolled offsets included).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Grid {
    /// Slot 0's top left corner.
    pub origin: (f32, f32),
    /// One slot's size.
    pub item: (f32, f32),
    /// From one slot to the next across and down (the size plus the gaps).
    pub pitch: (f32, f32),
    pub columns: usize,
    /// The list's top left corner in the surface, and its size.
    pub page: (f32, f32),
    pub size: (f32, f32),
}

impl Grid {
    /// The slot under a content point; `None` in the gaps and past the last slot.
    pub fn slot_at(&self, x: f32, y: f32, count: usize) -> Option<usize> {
        let (rx, ry) = (x - self.origin.0, y - self.origin.1);
        if rx < 0.0 || ry < 0.0 {
            return None;
        }
        let column = (rx / self.pitch.0.max(1.0)).floor() as usize;
        let row = (ry / self.pitch.1.max(1.0)).floor() as usize;
        if column >= self.columns.max(1) || rx - column as f32 * self.pitch.0 > self.item.0 || ry - row as f32 * self.pitch.1 > self.item.1
        {
            return None;
        }
        Some(row * self.columns.max(1) + column).filter(|i| *i < count)
    }
    /// How far the list scrolls at most, for `count` slots: its content (the padding on both ends) past its height.
    pub fn max_scroll(&self, count: usize) -> f32 {
        let rows = count.div_ceil(self.columns.max(1)) as f32;
        let gap = self.pitch.1 - self.item.1;
        (2.0 * self.origin.1 + rows * self.pitch.1 - gap - self.size.1).max(0.0)
    }
    /// The zone the pointer (in the list) is resting in; none while the list is too short for two zones.
    pub fn edge(&self, x: f32, y: f32) -> Option<Edge> {
        if self.size.1 <= EDGE_ZONE * 2.0 || x < 0.0 || x > self.size.0 {
            return None;
        }
        if (0.0..EDGE_ZONE).contains(&y) {
            Some(Edge::Up)
        } else if y > self.size.1 - EDGE_ZONE && y <= self.size.1 {
            Some(Edge::Down)
        } else {
            None
        }
    }
    /// The stand-in's frame for a corner at `at`, kept within the container's width and, when `inside`, its height.
    pub fn standin(&self, at: (f32, f32), inside: bool) -> (f32, f32, f32, f32) {
        let clamp = |v: f32, room: f32| v.min(room).max(0.0);
        let y = if inside { clamp(at.1, self.size.1 - self.item.1) } else { at.1 };
        (clamp(at.0, self.size.0 - self.item.0), y, self.item.0, self.item.1)
    }
    fn contains(&self, x: f32, y: f32) -> bool {
        (0.0..=self.size.0).contains(&x) && (0.0..=self.size.1).contains(&y)
    }
}

/// `dragged` moved to `target`'s place: after it when dragging down, before it when dragging up.
pub fn moved(order: &[Uuid], dragged: Uuid, target: Uuid) -> Option<Vec<Uuid>> {
    let from = order.iter().position(|id| *id == dragged)?;
    let to = order.iter().position(|id| *id == target)?;
    if from == to {
        return None;
    }
    let mut next = order.to_vec();
    let id = next.remove(from);
    next.insert(to, id);
    Some(next)
}

/// The rows' ids in the preview order; rows the preview does not name (added while dragging) follow at the end.
pub fn applying(order: &[Uuid], ids: &[Uuid]) -> Vec<Uuid> {
    let mut shown: Vec<Uuid> = order.iter().copied().filter(|id| ids.contains(id)).collect();
    shown.extend(ids.iter().copied().filter(|id| !order.contains(id)));
    shown
}

/// One container's drag session. Only the stand-in reads the pointer, so moves never re-render the items.
#[derive(Clone, Copy)]
pub struct Reorder {
    s: Store,
    kind: Kind,
    layout: Layout,
    pickup: Pickup,
    /// The item a long press picked up, until the finger lifts or the drag starts (`Pickup::LongPress`).
    armed: Stored<Option<Uuid>>,
    pub dragged: Signal<Option<Uuid>>,
    pub target: Signal<Option<Uuid>>,
    /// The preview order while dragging (and until the saved order arrives).
    order: Signal<Option<Vec<Uuid>>>,
    /// The dragged account as it was at the grant: the stand-in shows it without ticking (accounts only).
    pub standin: Signal<Option<Row>>,
    /// The stand-in's frame in the container: its top left corner and size.
    pub pointer: Signal<Option<(f32, f32, f32, f32)>>,
    /// The container: the account list (its scroll view, for auto-scrolling) or the categories' block.
    pub list: ElementRef,
    grid: Stored<Option<Grid>>,
    grab: Stored<(f32, f32)>,
    last: Stored<Option<(f32, f32)>>,
    hovered: Stored<Option<Uuid>>,
    original: Stored<Vec<Uuid>>,
    scroll: Stored<f32>,
    edge: Stored<Option<Edge>>,
    ticker: Stored<Option<Spawned<()>>>,
    /// After Escape the gesture still reports moves; nothing starts again until the release.
    suppressed: Stored<bool>,
    /// The saved order is on its way: the preview stays until the store has it.
    pending: Stored<bool>,
    owner: Stored<Owner>,
}

impl Reorder {
    pub fn new(s: Store, kind: Kind, layout: Layout, pickup: Pickup) -> Self {
        let r = Self {
            s,
            kind,
            layout,
            pickup,
            armed: stored(None),
            dragged: signal(None),
            target: signal(None),
            order: signal(None),
            standin: signal(None),
            pointer: signal(None),
            list: element_ref(),
            grid: stored(None),
            grab: stored((0.0, 0.0)),
            last: stored(None),
            hovered: stored(None),
            original: stored(Vec::new()),
            scroll: stored(0.0),
            edge: stored(None),
            ticker: stored(None),
            suppressed: stored(false),
            pending: stored(false),
            owner: stored(Owner::current()),
        };
        effect(move || {
            if !s.busy.get() && r.pending.get() {
                r.pending.replace(false);
                // A drag that started meanwhile keeps its own preview; the projection has the saved order now.
                if untracked(|| r.dragged.get().is_none()) {
                    untracked(|| r.order.set(None));
                }
            }
        });
        r
    }

    /// The shown items' ids, in the preview order while there is one.
    pub fn ids(self) -> Vec<Uuid> {
        let ids: Vec<Uuid> = self.s.projection.with(|p| match self.kind {
            Kind::Accounts => p.rows.iter().map(|r| r.id).collect(),
            Kind::Categories => p.categories.iter().map(|c| c.id).collect(),
        });
        self.order.with(|o| o.as_ref().map(|o| applying(o, &ids))).unwrap_or(ids)
    }
    /// The row shown at `index`.
    pub fn row(self, index: usize) -> Option<Row> {
        if self.order.with(|o| o.is_none()) {
            return self.s.projection.with(|p| p.rows.get(index).cloned());
        }
        let id = *self.ids().get(index)?;
        self.s.projection.with(|p| p.rows.iter().find(|r| r.id == id).cloned())
    }
    /// The categories in the shown order.
    pub fn categories(self) -> Vec<safay_app::session::CategoryRow> {
        let categories = self.s.projection.with(|p| p.categories.clone());
        if self.order.with(|o| o.is_none()) {
            return categories;
        }
        self.ids().iter().filter_map(|id| categories.iter().find(|c| c.id == *id).cloned()).collect()
    }
    pub fn count(self) -> usize {
        self.s.projection.with(|p| match self.kind {
            Kind::Accounts => p.rows.len(),
            Kind::Categories => p.categories.len(),
        })
    }
    pub fn is_dragging(self, id: Option<Uuid>) -> bool {
        id.is_some() && self.dragged.get() == id
    }
    pub fn is_target(self, id: Option<Uuid>) -> bool {
        id.is_some() && self.target.get() == id
    }
    pub fn scrolled(self, offset: f32) {
        self.scroll.replace(offset);
    }

    /// Picks up `id` for a drag that starts once the finger moves (`Pickup::LongPress`): the item's long press calls it.
    pub fn arm(self, id: Uuid) {
        self.armed.replace(Some(id));
    }

    /// Wires an item (a card or a row) to the session.
    pub fn item<E: Styled + Interactive>(self, el: E, me: ElementRef, id: impl Fn() -> Option<Uuid> + Copy + 'static) -> E {
        el.element_ref(&me)
            // A new press lifts whatever an earlier long press picked up.
            .on_press_down(move |_: &PressEvent| {
                self.armed.replace(None);
            })
            .claim_touches(TouchClaim::on_move().ask(move |request: &ClaimRequest| {
                let d = request.event.gesture.delta;
                let picked = match self.pickup {
                    Pickup::Move => true,
                    Pickup::LongPress => id().is_some() && self.armed.get() == id(),
                };
                if picked && !self.suppressed.get() && self.count() > 1 && d.x.0.hypot(d.y.0) >= SLOP {
                    request.claim();
                }
            }))
            .keep_touches(true)
            .on_responder_grant(move |e: &ResponderEvent| {
                if let Some(id) = id() {
                    let start = (e.page.x.0 - e.gesture.delta.x.0, e.page.y.0 - e.gesture.delta.y.0);
                    self.begin(id, me, start, (e.page.x.0, e.page.y.0));
                }
            })
            .on_responder_move(move |e: &ResponderEvent| self.moved_to((e.page.x.0, e.page.y.0)))
            .on_responder_release(move |e: &ResponderEvent| self.release((e.page.x.0, e.page.y.0)))
            .on_responder_terminate(move |_: &ResponderEvent| {
                self.suppressed.replace(false);
                self.cancel();
            })
    }

    fn begin(self, id: Uuid, me: ElementRef, start: (f32, f32), at: (f32, f32)) {
        if self.suppressed.get() || self.dragged.get_untracked().is_some() {
            return;
        }
        let order = self.ids();
        let Some(index) = order.iter().position(|o| *o == id) else { return };
        self.original.replace(order.clone());
        self.order.set(Some(order));
        self.hovered.replace(Some(id));
        self.last.replace(Some(at));
        self.grid.replace(None);
        if self.kind == Kind::Accounts {
            self.standin.set(self.s.projection.with(|p| p.rows.iter().find(|r| r.id == id).cloned()));
        }
        self.dragged.set(Some(id));
        self.target.set(None);
        self.s.controller.get().activity();
        self.armed.replace(None);
        let (gap, columns) = (self.layout)(self.s);
        let list = self.list;
        self.owner.get().run(|| {
            spawn(async move {
                let (Some(l), Some(m)) = (list.measure().await, me.measure().await) else {
                    self.cancel();
                    return;
                };
                if self.dragged.get_untracked() != Some(id) {
                    return;
                }
                let (w, h) = (m.frame.size.width.0, m.frame.size.height.0);
                let pitch = (w + gap, h + gap);
                let scroll = self.scroll.get();
                let content = (m.page.x.0 - l.page.x.0, m.page.y.0 - l.page.y.0 + scroll);
                self.grid.replace(Some(Grid {
                    origin: (content.0 - (index % columns) as f32 * pitch.0, content.1 - (index / columns) as f32 * pitch.1),
                    item: (w, h),
                    pitch,
                    columns,
                    page: (l.page.x.0, l.page.y.0),
                    size: (l.frame.size.width.0, l.frame.size.height.0),
                }));
                self.grab.replace((start.0 - m.page.x.0, start.1 - m.page.y.0));
                if let Some(at) = self.last.get() {
                    self.moved_to(at);
                }
            });
        });
    }

    fn moved_to(self, at: (f32, f32)) {
        if self.dragged.get_untracked().is_none() {
            return;
        }
        self.last.replace(Some(at));
        let Some(grid) = self.grid.get() else { return };
        let (x, y) = (at.0 - grid.page.0, at.1 - grid.page.1);
        let grab = self.grab.get();
        self.pointer.set(Some(self.standin_frame(&grid, (x - grab.0, y - grab.1))));
        let edge = if self.kind == Kind::Accounts { grid.edge(x, y) } else { None };
        if edge != self.edge.get() {
            self.edge.replace(edge);
            self.stop_ticker();
            if edge.is_some() {
                let r = self;
                let ticker = self.owner.get().run(|| set_interval(BEAT, move || r.tick()));
                self.ticker.replace(Some(ticker));
            }
        }
        // The zones cover the first and last rows: while resting in one the list only scrolls.
        let hovered = match edge {
            Some(_) => None,
            None => {
                let ids = self.ids();
                grid.slot_at(x, y + self.scroll.get(), ids.len()).map(|i| ids[i])
            }
        };
        self.hover(hovered);
    }

    /// Where the stand-in goes for a corner at `at`: a category stays in the sidebar's width; an account stays inside
    /// the list, never over the toolbar or the sidebar.
    fn standin_frame(self, grid: &Grid, at: (f32, f32)) -> (f32, f32, f32, f32) {
        grid.standin(at, self.kind == Kind::Accounts)
    }

    fn hover(self, id: Option<Uuid>) {
        if id == self.hovered.get() {
            return;
        }
        if self.hovered.get().is_some() && self.target.get_untracked() == self.hovered.get() {
            self.target.set(None);
        }
        self.hovered.replace(id);
        let (Some(id), Some(dragged)) = (id, self.dragged.get_untracked()) else { return };
        if id != dragged {
            self.target.set(Some(id));
            self.preview(dragged, id);
        }
    }

    fn preview(self, dragged: Uuid, over: Uuid) {
        let Some(next) = self.order.with_untracked(|o| o.as_ref().and_then(|o| moved(o, dragged, over))) else { return };
        if !reduce_motion() {
            animate_layout(LayoutAnimation {
                update: Some(LayoutTransition::new(LayoutCurve::EaseOut, self.kind.swap_ms())),
                ..LayoutAnimation::default()
            });
        }
        self.order.set(Some(next));
    }

    /// One auto-scroll beat: scroll a step (two rows, or one row of cards), or at the end move the item to the first
    /// or last place.
    fn tick(self) {
        let (Some(edge), Some(grid), Some(dragged)) = (self.edge.get(), self.grid.get(), self.dragged.get_untracked()) else {
            self.stop_ticker();
            return;
        };
        let ids = self.ids();
        let step = if grid.columns > 1 { 1.0 } else { 2.0 } * grid.pitch.1;
        let now = self.scroll.get();
        let next = match edge {
            Edge::Up => (now - step).max(0.0),
            Edge::Down => (now + step).min(grid.max_scroll(ids.len())),
        };
        if (next - now).abs() < 0.5 {
            let end = match edge {
                Edge::Up => ids.first(),
                Edge::Down => ids.last(),
            };
            if let Some(&end) = end.filter(|e| **e != dragged) {
                self.target.set(Some(end));
                self.preview(dragged, end);
            }
            return;
        }
        self.scroll.replace(next);
        self.list.scroll_to_offset(px(next), true);
    }

    fn release(self, at: (f32, f32)) {
        self.suppressed.replace(false);
        if self.dragged.get_untracked().is_none() {
            return;
        }
        match self.grid.get() {
            Some(grid) if grid.contains(at.0 - grid.page.0, at.1 - grid.page.1) => self.commit(),
            _ => self.cancel(),
        }
    }

    /// Escape: back to the original order; the gesture only ends on release, and its moves are ignored until then.
    pub fn escape(self) {
        if self.dragged.get_untracked().is_some() {
            self.suppressed.replace(true);
            self.cancel();
        }
    }

    fn commit(self) {
        let order = self.order.get_untracked();
        let changed = order.as_ref().is_some_and(|o| *o != self.original.get());
        if let (true, Some(order)) = (changed, order) {
            match self.kind {
                Kind::Accounts => self.s.reorder_rows(order),
                Kind::Categories => self.s.reorder_categories(order),
            }
            // Saving runs off the main thread: the preview stays until the new order is in the projection, so
            // nothing jumps back in between.
            self.pending.replace(self.s.busy.get_untracked());
        }
        self.finish();
    }

    fn cancel(self) {
        if self.dragged.get_untracked().is_none() && self.order.with_untracked(|o| o.is_none()) {
            return;
        }
        // A save still on its way: back to the order this drag started from, which is that save's (the projection
        // does not have it yet). The preview goes once the save is in.
        if self.pending.get() {
            let original = self.original.get();
            if !original.is_empty() {
                self.order.set(Some(original));
            }
        }
        self.finish();
    }

    fn finish(self) {
        self.stop_ticker();
        self.edge.replace(None);
        self.hovered.replace(None);
        self.grid.replace(None);
        self.last.replace(None);
        self.original.replace(Vec::new());
        self.dragged.set(None);
        self.target.set(None);
        self.pointer.set(None);
        self.standin.set(None);
        if !self.pending.get() && self.order.with_untracked(|o| o.is_some()) {
            if !reduce_motion() {
                animate_layout(LayoutAnimation {
                    update: Some(LayoutTransition::new(LayoutCurve::EaseOut, self.kind.swap_ms())),
                    ..LayoutAnimation::default()
                });
            }
            self.order.set(None);
        }
    }

    fn stop_ticker(self) {
        // The beat runs under the page's owner and goes with it; this only stops it earlier.
        if let Some(t) = self.ticker.try_with(|t| t.take()).flatten() {
            t.cancel();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn id(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }
    #[test]
    fn moves_after_the_target_down_and_before_it_up() {
        let order = [id(1), id(2), id(3), id(4)];
        assert_eq!(moved(&order, id(1), id(3)).unwrap(), vec![id(2), id(3), id(1), id(4)]);
        assert_eq!(moved(&order, id(4), id(2)).unwrap(), vec![id(1), id(4), id(2), id(3)]);
        assert!(moved(&order, id(2), id(2)).is_none());
    }
    #[test]
    fn rows_missing_from_the_preview_follow() {
        assert_eq!(applying(&[id(2), id(1)], &[id(1), id(2), id(3)]), vec![id(2), id(1), id(3)]);
        assert_eq!(applying(&[id(2), id(9), id(1)], &[id(1), id(2)]), vec![id(2), id(1)]);
    }
    fn cards() -> Grid {
        Grid { origin: (16.0, 16.0), item: (200.0, 94.0), pitch: (212.0, 106.0), columns: 3, page: (0.0, 0.0), size: (668.0, 400.0) }
    }
    #[test]
    fn slots_skip_the_gaps() {
        let g = cards();
        assert_eq!(g.slot_at(20.0, 20.0, 9), Some(0));
        assert_eq!(g.slot_at(230.0, 20.0, 9), Some(1));
        assert_eq!(g.slot_at(220.0, 20.0, 9), None);
        assert_eq!(g.slot_at(20.0, 115.0, 9), None);
        assert_eq!(g.slot_at(450.0, 130.0, 9), Some(5));
        assert_eq!(g.slot_at(450.0, 130.0, 5), None);
        assert_eq!(g.slot_at(700.0, 20.0, 9), None);
    }
    #[test]
    fn the_standin_stays_in_its_container() {
        let g = cards();
        assert_eq!(g.standin((-30.0, -50.0), true), (0.0, 0.0, 200.0, 94.0));
        assert_eq!(g.standin((600.0, 390.0), true), (468.0, 306.0, 200.0, 94.0));
        assert_eq!(g.standin((100.0, 120.0), true), (100.0, 120.0, 200.0, 94.0));
        assert_eq!(g.standin((-30.0, -50.0), false), (0.0, -50.0, 200.0, 94.0));
    }
    #[test]
    fn edge_zones_and_scroll_range() {
        let g = cards();
        assert_eq!(g.edge(100.0, 10.0), Some(Edge::Up));
        assert_eq!(g.edge(100.0, 380.0), Some(Edge::Down));
        assert_eq!(g.edge(100.0, 200.0), None);
        assert_eq!(g.edge(-1.0, 10.0), None);
        // 10 rows of cards: 16 + 10 * 106 - 12 + 16 = 1080, minus the 400pt viewport.
        assert_eq!(g.max_scroll(30), 680.0);
        assert_eq!(g.max_scroll(3), 0.0);
    }
}
