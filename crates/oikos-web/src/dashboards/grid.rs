use leptos::prelude::*;
use oikos_core::dashboard::Widget;

use super::edit::EditState;
use super::widget::WidgetView;
use crate::i18n::{DashText, dash_text, use_lang};
use crate::icons::GRIP;

/// Pointer travel in CSS pixels above which a press starts a drag; below, a press on the tile
/// stays a click that opens its settings.
const DRAG_THRESHOLD: f64 = 6.0;

/// A press on a tile or its grip, until it is released.
#[derive(Clone)]
struct Press {
    id: String,
    pointer: i32,
    x: f64,
    y: f64,
    /// Set once the pointer travelled past [`DRAG_THRESHOLD`].
    dragging: bool,
    /// The tile the dragged one last moved onto (see [`drag_step`]).
    last: Option<String>,
}

/// One pointer move of a drag of tile `dragged`, now over tile `hovered` (`None` over a gap):
/// the tile to move onto, if any, and the new `last`. Moving onto the tile it just moved onto
/// is skipped: a taller neighbour can still sit under the pointer after the swap and would
/// swap straight back. Reaching the dragged tile itself clears that, so going back works.
fn drag_step(
    dragged: &str,
    hovered: Option<&str>,
    last: Option<&str>,
) -> (Option<String>, Option<String>) {
    match hovered {
        Some(h) if h == dragged => (None, None),
        Some(h) if Some(h) != last => (Some(h.to_owned()), Some(h.to_owned())),
        _ => (None, last.map(str::to_owned)),
    }
}

/// Drag-to-reorder state shared by every tile of a grid.
#[derive(Clone, Copy)]
struct Drag {
    press: StoredValue<Option<Press>>,
    /// The id of the tile being dragged, for its lifted look.
    dragging: RwSignal<Option<String>>,
    /// Set when a drag ends, so the click that follows the release does not open the settings
    /// of the tile under the pointer.
    swallow_click: StoredValue<bool>,
}

impl Drag {
    fn new() -> Self {
        Self {
            press: StoredValue::new(None),
            dragging: RwSignal::new(None),
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
            last: None,
        }));
    }

    /// Follows the pointer on the window rather than on the tile: the tile moves in the DOM
    /// while it is dragged, which can drop a pointer capture.
    fn moved(&self, ev: &web_sys::PointerEvent, widgets: Signal<Vec<Widget>>, state: EditState) {
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
        }
        ev.prevent_default();
        let hovered = document()
            .element_from_point(x as f32, y as f32)
            .and_then(|el| el.closest("[data-widget-id]").ok().flatten())
            .and_then(|el| el.get_attribute("data-widget-id"));
        let (onto, last) = drag_step(&press.id, hovered.as_deref(), press.last.as_deref());
        press.last = last;
        if let Some(onto) = onto {
            let (from, to) = widgets.with_untracked(|ws| {
                let at = |id: &str| ws.iter().position(|w| w.id == id);
                (at(&press.id), at(&onto))
            });
            if let (Some(from), Some(to)) = (from, to) {
                state.reorder(from, to);
            }
        }
        self.press.set_value(Some(press));
    }

    fn up(&self, ev: &web_sys::PointerEvent) {
        let Some(press) = self.press.get_value() else {
            return;
        };
        if press.pointer != ev.pointer_id() {
            return;
        }
        self.swallow_click.set_value(press.dragging);
        self.press.set_value(None);
        self.dragging.set(None);
    }
}

/// Column grid that places widgets by their declared span. Column counts per breakpoint
/// live in `styles/main.css` (`.dashboard-grid`). No clamp is needed: the widest span is 2
/// columns and the grid never has fewer than 2, so no widget overflows on a phone.
///
/// In edit mode tiles reorder by drag: from the grip in each tile's tool pill (mouse, pen or
/// touch; the arrow keys move a focused grip) or, with a mouse or pen, from anywhere on the
/// tile. A touch on the tile itself still scrolls the page. Pointer events rather than HTML5
/// drag, which has no touch support.
#[component]
pub fn Grid(
    widgets: Signal<Vec<Widget>>,
    #[prop(optional)] edit: Option<EditState>,
) -> impl IntoView {
    let active = move || edit.is_some_and(|e| e.active.get());
    let lang = use_lang();
    let drag = Drag::new();
    if let Some(state) = edit {
        let moved = window_event_listener(leptos::ev::pointermove, move |ev| {
            drag.moved(&ev, widgets, state);
        });
        let up = window_event_listener(leptos::ev::pointerup, move |ev| drag.up(&ev));
        let cancel = window_event_listener(leptos::ev::pointercancel, move |ev| drag.up(&ev));
        on_cleanup(move || {
            moved.remove();
            up.remove();
            cancel.remove();
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
        <div class="dashboard-grid" class:dashboard-grid--dragging=move || drag.dragging.with(Option::is_some)>
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
    use super::drag_step;

    fn s(v: &str) -> Option<String> {
        Some(v.to_owned())
    }

    #[test]
    fn moves_onto_a_new_neighbour() {
        assert_eq!(drag_step("a", Some("b"), None), (s("b"), s("b")));
        assert_eq!(drag_step("a", Some("c"), Some("b")), (s("c"), s("c")));
    }

    #[test]
    fn does_not_swap_back_onto_the_tile_it_just_moved_onto() {
        assert_eq!(drag_step("a", Some("b"), Some("b")), (None, s("b")));
    }

    #[test]
    fn over_itself_clears_last_and_over_a_gap_keeps_it() {
        assert_eq!(drag_step("a", Some("a"), Some("b")), (None, None));
        assert_eq!(drag_step("a", None, Some("b")), (None, s("b")));
    }
}
