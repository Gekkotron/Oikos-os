//! The climate ring: a 270° arc around the target temperature, dragged with a finger or the
//! mouse, stepped with the arrow keys. Angles are in degrees, clockwise from the right (as
//! `atan2` gives them on a screen): the arc runs from bottom-left (135°) over the top to
//! bottom-right (405°).

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use gloo_timers::callback::Timeout;
use leptos::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;

/// Where the arc starts (the lowest value) and how far it turns.
pub const START_DEG: f64 = 135.0;
pub const SWEEP_DEG: f64 = 270.0;

/// The angle of `value` on the arc, clamped to its ends.
pub fn angle_of(value: f64, min: f64, max: f64) -> f64 {
    let share = if max > min {
        ((value - min) / (max - min)).clamp(0.0, 1.0)
    } else {
        0.0
    };
    START_DEG + share * SWEEP_DEG
}

/// The value under `angle_deg`, snapped to `step` from `min` and kept in range. A point in the
/// gap under the arc goes to the nearer end (the top one from the middle rightwards).
pub fn value_at(angle_deg: f64, min: f64, max: f64, step: f64) -> f64 {
    let rel = (angle_deg - START_DEG).rem_euclid(360.0);
    let gap_middle = SWEEP_DEG + (360.0 - SWEEP_DEG) / 2.0;
    let rel = if rel <= SWEEP_DEG {
        rel
    } else if rel <= gap_middle {
        SWEEP_DEG
    } else {
        0.0
    };
    let raw = min + rel / SWEEP_DEG * (max - min);
    let step = if step > 0.0 { step } else { 0.5 };
    (min + ((raw - min) / step).round() * step).clamp(min, max)
}

/// The SVG path of the arc from `from_deg` to `to_deg` on a circle of radius `r` centred at
/// (`c`, `c`), drawn clockwise.
pub fn arc_path(from_deg: f64, to_deg: f64, r: f64, c: f64) -> String {
    let point = |deg: f64| {
        let rad = deg.to_radians();
        (round2(c + r * rad.cos()), round2(c + r * rad.sin()))
    };
    let ((sx, sy), (ex, ey)) = (point(from_deg), point(to_deg));
    let large = u8::from(to_deg - from_deg > 180.0);
    format!("M {sx} {sy} A {r} {r} 0 {large} 1 {ex} {ey}")
}

fn round2(n: f64) -> f64 {
    (n * 100.0).round() / 100.0
}

/// The circle the ring is drawn on, in the 160 × 160 view box.
const R: f64 = 70.0;
const C: f64 = 80.0;
/// How long the arrow keys must rest before their value is sent.
const KEY_PAUSE_MS: u32 = 600;

/// A change in progress on the ring of `key` (an entity id). It lives outside the component:
/// a card is rebuilt whenever its entity changes, and a drag or a burst of key presses must
/// survive that.
struct Edit {
    key: String,
    value: f64,
    by_pointer: bool,
}

/// What the ring currently mounted for a key offers the edit in progress.
#[derive(Clone)]
struct Hooks {
    generation: u64,
    show: Rc<dyn Fn(f64)>,
    commit: Rc<dyn Fn(f64)>,
    range: (f64, f64, f64),
}

thread_local! {
    static EDIT: RefCell<Option<Edit>> = const { RefCell::new(None) };
    static HOOKS: RefCell<HashMap<String, Hooks>> = RefCell::new(HashMap::new());
    static KEY_TIMER: RefCell<Option<Timeout>> = const { RefCell::new(None) };
    static GENERATION: Cell<u64> = const { Cell::new(0) };
    static LISTENING: Cell<bool> = const { Cell::new(false) };
}

/// The value being edited on the ring of `key`, if any.
fn draft(key: &str) -> Option<f64> {
    EDIT.with(|e| {
        e.borrow()
            .as_ref()
            .filter(|e| e.key == key)
            .map(|e| e.value)
    })
}

fn hooks(key: &str) -> Option<Hooks> {
    HOOKS.with(|h| h.borrow().get(key).cloned())
}

/// Ends the edit in progress and sends its value through the ring mounted for it now.
fn finish_edit() {
    let Some(edit) = EDIT.with(|e| e.borrow_mut().take()) else {
        return;
    };
    if let Some(hooks) = hooks(&edit.key) {
        (hooks.commit)(edit.value);
    }
}

/// Moves the edit in progress to `value` and shows it on the mounted ring.
fn set_edit(key: &str, value: f64, by_pointer: bool) {
    EDIT.with(|e| {
        *e.borrow_mut() = Some(Edit {
            key: key.to_owned(),
            value,
            by_pointer,
        });
    });
    if let Some(hooks) = hooks(key) {
        (hooks.show)(value);
    }
}

/// The value under a pointer event on the ring element `el`.
fn value_from_pointer(
    el: &web_sys::Element,
    e: &web_sys::PointerEvent,
    range: (f64, f64, f64),
) -> f64 {
    let rect = el.get_bounding_client_rect();
    let dx = f64::from(e.client_x()) - (rect.left() + rect.width() / 2.0);
    let dy = f64::from(e.client_y()) - (rect.top() + rect.height() / 2.0);
    value_at(dy.atan2(dx).to_degrees(), range.0, range.1, range.2)
}

/// The ring element of `key` a pointer event is about: the ring under the pointer (or holding
/// its capture) when it is that one, else the first mounted ring of `key`.
fn ring_element(key: &str, e: &web_sys::PointerEvent) -> Option<web_sys::Element> {
    let under = e
        .target()
        .and_then(|t| t.dyn_into::<web_sys::Element>().ok())
        .and_then(|el| el.closest("[data-dial-key]").ok().flatten())
        .filter(|el| el.get_attribute("data-dial-key").as_deref() == Some(key));
    under.or_else(|| first_ring(key))
}

/// The first mounted ring of `key`.
fn first_ring(key: &str) -> Option<web_sys::Element> {
    let list = document().query_selector_all("[data-dial-key]").ok()?;
    (0..list.length())
        .filter_map(|i| list.item(i)?.dyn_into::<web_sys::Element>().ok())
        .find(|el| el.get_attribute("data-dial-key").as_deref() == Some(key))
}

/// Window listeners shared by every ring, installed once: they follow a drag wherever the
/// pointer goes and whichever ring is mounted when it ends.
fn listen_to_window() {
    if LISTENING.with(|l| l.replace(true)) {
        return;
    }
    let on_move = Closure::<dyn FnMut(web_sys::PointerEvent)>::new(|e: web_sys::PointerEvent| {
        let Some(key) = EDIT.with(|d| {
            d.borrow()
                .as_ref()
                .filter(|d| d.by_pointer)
                .map(|d| d.key.clone())
        }) else {
            return;
        };
        if e.buttons() == 0 {
            return;
        }
        let (Some(hooks), Some(el)) = (hooks(&key), ring_element(&key, &e)) else {
            return;
        };
        set_edit(&key, value_from_pointer(&el, &e, hooks.range), true);
    });
    let on_up = Closure::<dyn FnMut(web_sys::PointerEvent)>::new(|_: web_sys::PointerEvent| {
        let by_pointer = EDIT.with(|d| d.borrow().as_ref().is_some_and(|d| d.by_pointer));
        if by_pointer {
            finish_edit();
        }
    });
    let w = window();
    let _ = w.add_event_listener_with_callback("pointermove", on_move.as_ref().unchecked_ref());
    let _ = w.add_event_listener_with_callback("pointerup", on_up.as_ref().unchecked_ref());
    let _ = w.add_event_listener_with_callback("pointercancel", on_up.as_ref().unchecked_ref());
    on_move.forget();
    on_up.forget();
}

/// The ring. `value` is the target (`None`: nothing to show); `draft_key` names the entity, so
/// an edit in progress carries over when the card is rebuilt. `on_commit` receives the value
/// once: when the pointer is released, or [`KEY_PAUSE_MS`] after the last arrow key.
#[component]
pub fn Dial(
    #[prop(into)] draft_key: String,
    value: Option<f64>,
    min: f64,
    max: f64,
    step: f64,
    disabled: bool,
    #[prop(into)] aria_label: String,
    /// The centre text and screen-reader text for a value ("15,5°").
    value_text: Callback<f64, String>,
    /// The centre text with no value to show, or while disabled ("—", "Off").
    #[prop(into)]
    placeholder: String,
    #[prop(into)] caption: String,
    /// The colour of the arc: `heat`, `cool`, `other` or `off`.
    #[prop(into)]
    tone: String,
    on_commit: Callback<f64>,
) -> impl IntoView {
    listen_to_window();
    let step = if step > 0.0 { step } else { 0.5 };
    let range = (min, max, step);
    let shown = RwSignal::new(draft(&draft_key).or(value));
    let generation = GENERATION.with(|g| {
        g.set(g.get() + 1);
        g.get()
    });
    HOOKS.with(|h| {
        h.borrow_mut().insert(
            draft_key.clone(),
            Hooks {
                generation,
                show: Rc::new(move |v| {
                    shown.try_set(Some(v));
                }),
                commit: Rc::new(move |v| on_commit.run(v)),
                range,
            },
        );
    });
    {
        let key = draft_key.clone();
        on_cleanup(move || {
            HOOKS.with(|h| {
                let mut h = h.borrow_mut();
                if h.get(&key)
                    .is_some_and(|hooks| hooks.generation == generation)
                {
                    h.remove(&key);
                }
            });
        });
    }

    let pointer_key = draft_key.clone();
    let on_pointerdown = move |e: web_sys::PointerEvent| {
        if disabled || value.is_none() {
            return;
        }
        e.prevent_default();
        KEY_TIMER.with(|t| t.borrow_mut().take());
        let Some(el) = e
            .current_target()
            .and_then(|t| t.dyn_into::<web_sys::Element>().ok())
        else {
            return;
        };
        let _ = el.set_pointer_capture(e.pointer_id());
        set_edit(&pointer_key, value_from_pointer(&el, &e, range), true);
    };
    let keys_key = draft_key.clone();
    let on_keydown = move |e: web_sys::KeyboardEvent| {
        if disabled {
            return;
        }
        let Some(current) = shown.get_untracked() else {
            return;
        };
        let next = match e.key().as_str() {
            "ArrowUp" | "ArrowRight" => current + step,
            "ArrowDown" | "ArrowLeft" => current - step,
            "PageUp" => current + 1.0,
            "PageDown" => current - 1.0,
            "Home" => min,
            "End" => max,
            _ => return,
        };
        e.prevent_default();
        set_edit(&keys_key, next.clamp(min, max), false);
        let timer = Timeout::new(KEY_PAUSE_MS, finish_edit);
        KEY_TIMER.with(|t| *t.borrow_mut() = Some(timer));
    };

    let fill = move || {
        shown.get().filter(|_| !disabled).map(|v| {
            let angle = angle_of(v, min, max);
            let rad = angle.to_radians();
            let (kx, ky) = (round2(C + R * rad.cos()), round2(C + R * rad.sin()));
            view! {
                <path class="dial__fill" d=arc_path(START_DEG, angle, R, C)></path>
                <circle class="dial__knob" cx=kx cy=ky r="8"></circle>
            }
        })
    };
    let centre = move || match shown.get() {
        Some(v) if !disabled => value_text.run(v),
        _ => placeholder.clone(),
    };
    let now = move || shown.get().map(|v| v.to_string());
    let now_text = move || shown.get().map(|v| value_text.run(v));

    view! {
        <svg
            class=format!("dial dial--{tone}")
            data-dial-key=draft_key
            viewBox="0 0 160 160"
            role="slider"
            tabindex="0"
            aria-label=aria_label
            aria-valuemin=min.to_string()
            aria-valuemax=max.to_string()
            aria-valuenow=now
            aria-valuetext=now_text
            aria-disabled=disabled.to_string()
            on:pointerdown=on_pointerdown
            on:keydown=on_keydown
        >
            <path class="dial__track" d=arc_path(START_DEG, START_DEG + SWEEP_DEG, R, C)></path>
            {fill}
            <text class="dial__value" x="80" y="86" text-anchor="middle">{centre}</text>
            <text class="dial__caption" x="80" y="106" text-anchor="middle">{caption}</text>
        </svg>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn angle_of_ends() {
        assert_eq!(angle_of(7.0, 7.0, 30.0), 135.0);
        assert_eq!(angle_of(30.0, 7.0, 30.0), 405.0);
        assert_eq!(angle_of(18.5, 7.0, 30.0), 270.0);
        assert_eq!(angle_of(40.0, 7.0, 30.0), 405.0);
        assert_eq!(angle_of(0.0, 7.0, 30.0), 135.0);
    }

    #[test]
    fn value_at_snaps_and_clamps() {
        assert_eq!(value_at(135.0, 7.0, 30.0, 0.5), 7.0);
        assert_eq!(value_at(45.0, 7.0, 30.0, 0.5), 30.0);
        assert_eq!(value_at(405.0, 7.0, 30.0, 0.5), 30.0);
        assert_eq!(value_at(270.0, 7.0, 30.0, 0.5), 18.5);
        assert_eq!(value_at(-90.0, 7.0, 30.0, 0.5), 18.5);
        // The gap at the bottom: right of centre goes to the top end, left of it to the bottom.
        assert_eq!(value_at(90.0, 7.0, 30.0, 0.5), 30.0);
        assert_eq!(value_at(100.0, 7.0, 30.0, 0.5), 7.0);
        assert_eq!(value_at(271.0, 7.0, 30.0, 1.0), 19.0);
    }

    #[test]
    fn arc_path_large_flag() {
        let full = arc_path(135.0, 405.0, 70.0, 80.0);
        assert!(
            full.starts_with("M 30.5 129.5 A 70 70 0 1 1 129.5 129.5"),
            "{full}"
        );
        let short = arc_path(135.0, 200.0, 70.0, 80.0);
        assert!(short.contains(" 0 0 1 "), "{short}");
    }
}
