use leptos::html::Div;
use leptos::prelude::*;
use oikos_core::dashboard::Widget;

use super::edit::EditState;
use super::widget::WidgetView;
use crate::i18n::{DashText, dash_text, use_lang};
use crate::icons::GRIP;

/// Pointer travel in CSS pixels above which a press starts a drag; below, a press on the tile
/// stays a click that opens its settings.
const DRAG_THRESHOLD: f64 = 6.0;

/// Where a dragged tile lands, shown as a bar on that side of the tile.
#[derive(Clone, Debug, PartialEq)]
enum Slot {
    Before(String),
    After(String),
}

/// A tile's box on screen, in client pixels.
struct TileBox {
    id: String,
    left: f64,
    top: f64,
    right: f64,
    bottom: f64,
}

impl TileBox {
    fn middle(&self) -> f64 {
        (self.left + self.right) / 2.0
    }
}

/// The slot under the pointer at (`x`, `y`) while tile `dragged` is dragged; `tiles` are in
/// grid order. Over a tile, its nearer side. Over a gap or the page around the grid, before the
/// first tile that comes after the pointer in reading order, else after the last tile, so the
/// empty space below the grid moves a tile to the end.
fn slot_at(tiles: &[TileBox], dragged: &str, x: f64, y: f64) -> Option<Slot> {
    let mut others = tiles.iter().filter(|t| t.id != dragged);
    let side = |t: &TileBox| {
        if x < t.middle() {
            Slot::Before(t.id.clone())
        } else {
            Slot::After(t.id.clone())
        }
    };
    if let Some(t) = others
        .clone()
        .find(|t| (t.left..t.right).contains(&x) && (t.top..t.bottom).contains(&y))
    {
        return Some(side(t));
    }
    if let Some(t) = others
        .clone()
        .find(|t| t.top > y || ((t.top..t.bottom).contains(&y) && t.middle() > x))
    {
        return Some(Slot::Before(t.id.clone()));
    }
    others.next_back().map(|t| Slot::After(t.id.clone()))
}

/// The index tile `from` moves to when dropped in `slot`, or `None` when it stays put.
fn drop_index(ids: &[&str], from: usize, slot: &Slot) -> Option<usize> {
    let before = match slot {
        Slot::Before(id) => ids.iter().position(|x| x == id)?,
        Slot::After(id) => ids.iter().position(|x| x == id)? + 1,
    };
    let to = if before > from { before - 1 } else { before };
    (to != from).then_some(to)
}

/// A press on a tile or its grip, until it is released.
#[derive(Clone)]
struct Press {
    id: String,
    pointer: i32,
    x: f64,
    y: f64,
    /// Set once the pointer travelled past [`DRAG_THRESHOLD`].
    dragging: bool,
}

/// Drag-to-reorder state shared by every tile of a grid. Tiles stay in place while a tile is
/// dragged: it follows the pointer and a bar marks where it will land, and the order changes
/// on release. Reordering live made tiles of different sizes jump under the pointer.
#[derive(Clone, Copy)]
struct Drag {
    press: StoredValue<Option<Press>>,
    /// The id of the tile being dragged, for its lifted look.
    dragging: RwSignal<Option<String>>,
    /// How far the dragged tile has travelled from where it was picked up.
    offset: RwSignal<(f64, f64)>,
    /// Where the dragged tile would land if released now; `None` when it would stay put.
    slot: RwSignal<Option<Slot>>,
    /// Set when a drag ends, so the click that follows the release does not open the settings
    /// of the tile under the pointer.
    swallow_click: StoredValue<bool>,
}

impl Drag {
    fn new() -> Self {
        Self {
            press: StoredValue::new(None),
            dragging: RwSignal::new(None),
            offset: RwSignal::new((0.0, 0.0)),
            slot: RwSignal::new(None),
            swallow_click: StoredValue::new(false),
        }
    }

    fn down(&self, ev: &web_sys::PointerEvent, id: &str) {
        if ev.pointer_type() == "mouse" && ev.button() != 0 {
            return;
        }
        self.swallow_click.set_value(false);
        self.press.set_value(Some(Press {
            id: id.to_owned(),
            pointer: ev.pointer_id(),
            x: f64::from(ev.client_x()),
            y: f64::from(ev.client_y()),
            dragging: false,
        }));
    }

    /// Follows the pointer on the window rather than on the tile, so the drag goes on over the
    /// gaps and the page around the grid.
    fn moved(&self, ev: &web_sys::PointerEvent, widgets: Signal<Vec<Widget>>, grid: NodeRef<Div>) {
        let Some(mut press) = self.press.get_value() else {
            return;
        };
        if press.pointer != ev.pointer_id() {
            return;
        }
        let (x, y) = (f64::from(ev.client_x()), f64::from(ev.client_y()));
        if !press.dragging {
            if (x - press.x).hypot(y - press.y) < DRAG_THRESHOLD {
                return;
            }
            press.dragging = true;
            self.dragging.set(Some(press.id.clone()));
            self.press.set_value(Some(press.clone()));
        }
        ev.prevent_default();
        self.offset.set((x - press.x, y - press.y));
        let slot = self.slot_under(&press.id, x, y, widgets, grid);
        if self.slot.with_untracked(|s| *s != slot) {
            self.slot.set(slot);
        }
    }

    fn slot_under(
        &self,
        dragged: &str,
        x: f64,
        y: f64,
        widgets: Signal<Vec<Widget>>,
        grid: NodeRef<Div>,
    ) -> Option<Slot> {
        let grid = grid.get_untracked()?;
        // The widget library is not a drop target: releasing over it leaves the order alone.
        let over_library = document()
            .query_selector(".dashboard-library-panel")
            .ok()
            .flatten()
            .is_some_and(|panel| {
                let r = panel.get_bounding_client_rect();
                (r.left()..r.right()).contains(&x) && (r.top()..r.bottom()).contains(&y)
            });
        if over_library {
            return None;
        }
        let (tiles, ids): (Vec<TileBox>, Vec<String>) = widgets.with_untracked(|ws| {
            let tiles = ws
                .iter()
                .filter_map(|w| {
                    let selector = format!("[data-widget-id=\"{}\"]", w.id);
                    let r = grid
                        .query_selector(&selector)
                        .ok()??
                        .get_bounding_client_rect();
                    Some(TileBox {
                        id: w.id.clone(),
                        left: r.left(),
                        top: r.top(),
                        right: r.right(),
                        bottom: r.bottom(),
                    })
                })
                .collect();
            (tiles, ws.iter().map(|w| w.id.clone()).collect())
        });
        let slot = slot_at(&tiles, dragged, x, y)?;
        let ids: Vec<&str> = ids.iter().map(String::as_str).collect();
        let from = ids.iter().position(|id| *id == dragged)?;
        drop_index(&ids, from, &slot).map(|_| slot)
    }

    /// Ends the press; a drag that was over a slot moves its tile there.
    fn up(&self, ev: &web_sys::PointerEvent, widgets: Signal<Vec<Widget>>, state: EditState) {
        let Some(press) = self.press.get_value() else {
            return;
        };
        if press.pointer != ev.pointer_id() {
            return;
        }
        if let Some(slot) = self.slot.get_untracked() {
            let to = widgets.with_untracked(|ws| {
                let ids: Vec<&str> = ws.iter().map(|w| w.id.as_str()).collect();
                let from = ids.iter().position(|id| *id == press.id)?;
                Some((from, drop_index(&ids, from, &slot)?))
            });
            if let Some((from, to)) = to {
                state.reorder(from, to);
            }
        }
        self.swallow_click.set_value(press.dragging);
        self.cancel();
    }

    fn cancel(&self) {
        self.press.set_value(None);
        self.dragging.set(None);
        self.slot.set(None);
        self.offset.set((0.0, 0.0));
    }
}

/// Column grid that places widgets by their declared span. Column counts per breakpoint
/// live in `styles/main.css` (`.dashboard-grid`). No clamp is needed: the widest span is 2
/// columns and the grid never has fewer than 2, so no widget overflows on a phone.
///
/// In edit mode tiles reorder by drag: from the grip in each tile's tool pill (mouse, pen or
/// touch; the arrow keys move a focused grip) or, with a mouse or pen, from anywhere on the
/// tile. A touch on the tile itself still scrolls the page. Escape cancels a drag. Pointer
/// events rather than HTML5 drag, which has no touch support.
#[component]
pub fn Grid(
    widgets: Signal<Vec<Widget>>,
    #[prop(optional)] edit: Option<EditState>,
) -> impl IntoView {
    let active = move || edit.is_some_and(|e| e.active.get());
    let lang = use_lang();
    let drag = Drag::new();
    let grid = NodeRef::<Div>::new();
    if let Some(state) = edit {
        let moved = window_event_listener(leptos::ev::pointermove, move |ev| {
            drag.moved(&ev, widgets, grid);
        });
        let up = window_event_listener(leptos::ev::pointerup, move |ev| {
            drag.up(&ev, widgets, state);
        });
        let cancel = window_event_listener(leptos::ev::pointercancel, move |_| drag.cancel());
        let escape = window_event_listener(leptos::ev::keydown, move |ev| {
            if ev.key() == "Escape" && drag.dragging.with_untracked(Option::is_some) {
                drag.cancel();
            }
        });
        on_cleanup(move || {
            moved.remove();
            up.remove();
            cancel.remove();
            escape.remove();
        });
    }
    // The tile of a widget just added is brought into view once it has rendered.
    #[cfg(target_arch = "wasm32")]
    if let Some(state) = edit {
        Effect::new(move |_| {
            if let Some(id) = state.fresh.get() {
                request_animation_frame(move || {
                    let selector = format!("[data-widget-id=\"{id}\"]");
                    if let Ok(Some(tile)) = document().query_selector(&selector) {
                        let options = web_sys::ScrollIntoViewOptions::new();
                        options.set_block(web_sys::ScrollLogicalPosition::Nearest);
                        options.set_behavior(web_sys::ScrollBehavior::Smooth);
                        tile.scroll_into_view_with_scroll_into_view_options(&options);
                    }
                });
            }
        });
    }
    view! {
        <div
            node_ref=grid
            class="dashboard-grid"
            class:dashboard-grid--dragging=move || drag.dragging.with(Option::is_some)
        >
            <For
                each=move || widgets.get()
                key=|w| w.id.clone()
                children=move |w: Widget| {
                    // Keyed by id, so the tile survives a span change or reorder; the class and
                    // index must therefore be looked up reactively.
                    let id = w.id.clone();
                    let position = {
                        let id = id.clone();
                        // Callback is Copy, so the handlers can each capture it.
                        Callback::new(move |()| {
                            widgets.with(|ws| ws.iter().position(|x| x.id == id))
                        })
                    };
                    let class = {
                        let id = id.clone();
                        move || {
                            let span = widgets
                                .with(|ws| ws.iter().find(|x| x.id == id).map(|x| x.span))
                                .unwrap_or(w.span);
                            // `concat!` keeps the bare-text guard from reading the class list as text.
                            format!(concat!("widget", " widget-span-{}"), span.as_str())
                        }
                    };
                    let current = {
                        let id = id.clone();
                        let initial = w.clone();
                        Signal::derive(move || {
                            widgets
                                .with(|ws| ws.iter().find(|x| x.id == id).cloned())
                                .unwrap_or_else(|| initial.clone())
                        })
                    };
                    let is_fresh = {
                        let id = id.clone();
                        move || edit.is_some_and(|e| e.fresh.with(|f| f.as_deref() == Some(id.as_str())))
                    };
                    let is_dragged = {
                        let id = id.clone();
                        move || drag.dragging.with(|d| d.as_deref() == Some(id.as_str()))
                    };
                    // Only the dragged tile reads the offset, so a pointer move re-renders it alone.
                    let lifted = {
                        let is_dragged = is_dragged.clone();
                        move || {
                            is_dragged().then(|| {
                                let (dx, dy) = drag.offset.get();
                                format!("translate({dx}px, {dy}px) scale(1.02)")
                            })
                        }
                    };
                    let drop_side = |after: bool| {
                        let id = id.clone();
                        move || {
                            drag.slot.with(|s| match s {
                                Some(Slot::Before(x)) => !after && *x == id,
                                Some(Slot::After(x)) => after && *x == id,
                                None => false,
                            })
                        }
                    };
                    let tools = edit.map(|state| {
                        let grip_id = id.clone();
                        view! {
                            <Show when=active>
                                // Stop bubbling so the tile's open-config click does not fire
                                // for the grip/resize buttons.
                                <div class="widget__tools" on:click=|ev| ev.stop_propagation()>
                                    <button
                                        type="button"
                                        class="widget__grip"
                                        aria-label=move || dash_text(lang(), DashText::Move)
                                        inner_html=GRIP
                                        on:pointerdown={
                                            let grip_id = grip_id.clone();
                                            move |ev: web_sys::PointerEvent| drag.down(&ev, &grip_id)
                                        }
                                        on:keydown=move |ev: web_sys::KeyboardEvent| {
                                            let Some(i) = position.run(()) else { return };
                                            let to = match ev.key().as_str() {
                                                "ArrowLeft" | "ArrowUp" => i.checked_sub(1),
                                                "ArrowRight" | "ArrowDown" => Some(i + 1),
                                                _ => return,
                                            };
                                            ev.prevent_default();
                                            if let Some(to) = to {
                                                state.reorder(i, to);
                                            }
                                        }
                                    ></button>
                                    <button
                                        type="button"
                                        class="widget__resize"
                                        aria-label=move || dash_text(lang(), DashText::Resize)
                                        on:click=move |_| {
                                            if let Some(i) = position.run(()) {
                                                state.cycle_span(i);
                                            }
                                        }
                                    >
                                        "⤢"
                                    </button>
                                </div>
                            </Show>
                        }
                    });
                    let overlay_id = id.clone();
                    view! {
                        <div
                            class=class
                            class:widget--fresh=is_fresh
                            class:widget--dragging=is_dragged
                            class:widget--drop-before=drop_side(false)
                            class:widget--drop-after=drop_side(true)
                            style:transform=lifted
                            data-widget-id=id
                        >
                            <WidgetView widget=w current=current />
                            // An overlay (not a tile click handler) so the inner card's own
                            // controls, e.g. a light toggle, cannot fire while editing. It sits
                            // above the card and below `.widget__tools` (z-index 2).
                            {edit.map(|state| view! {
                                <Show when=active>
                                    <div
                                        class="widget__edit-overlay"
                                        on:pointerdown={
                                            let overlay_id = overlay_id.clone();
                                            move |ev: web_sys::PointerEvent| {
                                                // A drag that ended over another tile leaves the flag
                                                // set, as its click went to the grid; a new press clears it.
                                                drag.swallow_click.set_value(false);
                                                // A touch on the tile scrolls the page; the grip drags.
                                                if ev.pointer_type() != "touch" {
                                                    drag.down(&ev, &overlay_id);
                                                }
                                            }
                                        }
                                        on:click=move |_| {
                                            if drag.swallow_click.get_value() {
                                                drag.swallow_click.set_value(false);
                                                return;
                                            }
                                            if let Some(i) = position.run(()) {
                                                state.selected_widget.set(Some(i));
                                            }
                                        }
                                    />
                                </Show>
                            })}
                            {tools}
                        </div>
                    }
                }
            />
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::{Slot, TileBox, drop_index, slot_at};

    fn tile(id: &str, left: f64, top: f64, right: f64, bottom: f64) -> TileBox {
        TileBox {
            id: id.to_owned(),
            left,
            top,
            right,
            bottom,
        }
    }

    /// Two rows of 100×100 tiles with 10 px gaps: a b c, then d.
    fn tiles() -> Vec<TileBox> {
        vec![
            tile("a", 0.0, 0.0, 100.0, 100.0),
            tile("b", 110.0, 0.0, 210.0, 100.0),
            tile("c", 220.0, 0.0, 320.0, 100.0),
            tile("d", 0.0, 110.0, 100.0, 210.0),
        ]
    }

    fn before(id: &str) -> Option<Slot> {
        Some(Slot::Before(id.to_owned()))
    }

    fn after(id: &str) -> Option<Slot> {
        Some(Slot::After(id.to_owned()))
    }

    #[test]
    fn over_a_tile_picks_its_nearer_side() {
        assert_eq!(slot_at(&tiles(), "a", 130.0, 50.0), before("b"));
        assert_eq!(slot_at(&tiles(), "a", 190.0, 50.0), after("b"));
    }

    #[test]
    fn the_dragged_tile_is_never_a_target() {
        // Over a's own box: the next tile in reading order.
        assert_eq!(slot_at(&tiles(), "a", 50.0, 50.0), before("b"));
    }

    #[test]
    fn a_gap_lands_before_the_next_tile_in_reading_order() {
        assert_eq!(slot_at(&tiles(), "d", 105.0, 50.0), before("b"));
        assert_eq!(slot_at(&tiles(), "a", 330.0, 50.0), before("d"));
        assert_eq!(slot_at(&tiles(), "d", 330.0, 50.0), after("c"));
    }

    #[test]
    fn empty_space_after_the_last_tile_moves_to_the_end() {
        assert_eq!(slot_at(&tiles(), "a", 300.0, 160.0), after("d"));
        assert_eq!(slot_at(&tiles(), "a", 50.0, 600.0), after("d"));
        // Dragging the last tile itself: after the one before it, which drop_index skips.
        assert_eq!(slot_at(&tiles(), "d", 50.0, 600.0), after("c"));
    }

    #[test]
    fn above_the_grid_lands_first() {
        assert_eq!(slot_at(&tiles(), "c", 50.0, -40.0), before("a"));
    }

    #[test]
    fn drop_index_moves_or_stays() {
        let ids = ["a", "b", "c", "d"];
        let slot = |s: Option<Slot>| s.unwrap();
        assert_eq!(drop_index(&ids, 0, &slot(after("d"))), Some(3));
        assert_eq!(drop_index(&ids, 0, &slot(before("c"))), Some(1));
        assert_eq!(drop_index(&ids, 3, &slot(before("a"))), Some(0));
        assert_eq!(drop_index(&ids, 1, &slot(before("b"))), None);
        assert_eq!(drop_index(&ids, 1, &slot(before("c"))), None);
        assert_eq!(drop_index(&ids, 1, &slot(after("a"))), None);
        assert_eq!(drop_index(&ids, 0, &slot(before("x"))), None);
    }
}
