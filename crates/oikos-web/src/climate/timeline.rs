//! The week timeline: seven day rows by 24 hours, the base setpoint as the background and each
//! block as a bar. When editable, a block's ends drag in 15-minute steps and a click opens its
//! popover (times, setpoint, delete) so editing also works without precise dragging.
//!
//! The editing functions are pure: they take a block index into `blocks` and return new rules
//! that still satisfy `ModeRules` validation (from < to, within 00:00 to 24:00, no overlap on a
//! shared day).

use leptos::prelude::*;
use oikos_climate::doc::{Day, HhMm, ModeRules, Setpoint};
use wasm_bindgen::JsCast;

use super::{ShortDay, setpoint_text};

/// The grid blocks snap to, in minutes.
const STEP: u16 = 15;
const DAY_END: u16 = 1440;
/// The temperatures a block can hold (`ModeRules` validation accepts 0 to 40).
const MIN_TEMP: f64 = 0.0;
const MAX_TEMP: f64 = 40.0;

/// Which end of a block.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edge {
    From,
    To,
}

/// `minutes` rounded to the nearest 15 minutes, at most 24:00.
fn snap(minutes: u16) -> u16 {
    ((minutes.saturating_add(STEP / 2)) / STEP * STEP).min(DAY_END)
}

/// "HH:MM" for minutes since midnight.
fn hhmm_text(minutes: u16) -> String {
    format!("{:02}:{:02}", minutes / 60, minutes % 60)
}

/// Moves one end of `rules.blocks[block]` to `to`, snapped to 15 minutes. The end keeps `from`
/// before `to` and stops at the nearest block it shares a day with. An unknown block, or no
/// room to move, returns the rules unchanged.
pub fn move_edge(rules: &ModeRules, block: usize, edge: Edge, to: HhMm) -> ModeRules {
    let mut out = rules.clone();
    let Some(b) = rules.blocks.get(block) else {
        return out;
    };
    let neighbours = rules
        .blocks
        .iter()
        .enumerate()
        .filter(|(i, o)| *i != block && o.days.iter().any(|d| b.days.contains(d)))
        .map(|(_, o)| o);
    let want = snap(to.0);
    let (lo, hi) = match edge {
        Edge::From => (
            neighbours
                .filter(|o| o.to <= b.from)
                .map(|o| o.to.0)
                .max()
                .unwrap_or(0),
            (b.to.0.saturating_sub(1)) / STEP * STEP,
        ),
        Edge::To => (
            (b.from.0 / STEP + 1) * STEP,
            neighbours
                .filter(|o| o.from >= b.to)
                .map(|o| o.from.0)
                .min()
                .unwrap_or(DAY_END),
        ),
    };
    if lo > hi {
        return out;
    }
    let at = HhMm(want.clamp(lo, hi));
    match edge {
        Edge::From => out.blocks[block].from = at,
        Edge::To => out.blocks[block].to = at,
    }
    out
}

/// Sets the setpoint of `rules.blocks[block]`; a temperature is kept within 0 to 40.
pub fn set_setpoint(rules: &ModeRules, block: usize, sp: Setpoint) -> ModeRules {
    let mut out = rules.clone();
    if let Some(b) = out.blocks.get_mut(block) {
        b.setpoint = match sp {
            Setpoint::Temp(t) if t.is_finite() => Setpoint::Temp(t.clamp(MIN_TEMP, MAX_TEMP)),
            Setpoint::Temp(_) => return rules.clone(),
            Setpoint::Off => Setpoint::Off,
        };
    }
    out
}

/// Removes `rules.blocks[block]`; the other blocks keep their order.
pub fn delete_block(rules: &ModeRules, block: usize) -> ModeRules {
    let mut out = rules.clone();
    if block < out.blocks.len() {
        out.blocks.remove(block);
    }
    out
}

/// The minute under the pointer along the day track the handle sits in.
fn minute_at(ev: &web_sys::PointerEvent) -> Option<u16> {
    let handle = ev.current_target()?.dyn_into::<web_sys::Element>().ok()?;
    let track = handle.closest(".climate-timeline__track").ok()??;
    let rect = track.get_bounding_client_rect();
    if rect.width() <= 0.0 {
        return None;
    }
    let fraction = ((f64::from(ev.client_x()) - rect.left()) / rect.width()).clamp(0.0, 1.0);
    Some((fraction * f64::from(DAY_END)).round() as u16)
}

fn capture(ev: &web_sys::PointerEvent) {
    if let Some(target) = ev
        .current_target()
        .and_then(|t| t.dyn_into::<web_sys::Element>().ok())
    {
        let _ = target.set_pointer_capture(ev.pointer_id());
    }
}

/// A draggable end of a block. The element stays in the DOM while the rules change, so the
/// pointer capture survives the drag.
#[component]
fn Handle(
    block: usize,
    edge: Edge,
    rules: Signal<ModeRules>,
    on_change: Callback<ModeRules>,
) -> impl IntoView {
    let dragging = RwSignal::new(false);
    let label = match edge {
        Edge::From => "Drag the start",
        Edge::To => "Drag the end",
    };
    view! {
        <span
            class="climate-timeline__handle"
            class:climate-timeline__handle--from=edge == Edge::From
            class:climate-timeline__handle--to=edge == Edge::To
            aria-label=label
            on:pointerdown=move |ev: web_sys::PointerEvent| {
                ev.stop_propagation();
                ev.prevent_default();
                capture(&ev);
                dragging.set(true);
            }
            on:pointermove=move |ev: web_sys::PointerEvent| {
                if !dragging.get_untracked() {
                    return;
                }
                if let Some(minute) = minute_at(&ev) {
                    let current = rules.get_untracked();
                    let next = move_edge(&current, block, edge, HhMm(minute));
                    if next != current {
                        on_change.run(next);
                    }
                }
            }
            on:pointerup=move |_| dragging.set(false)
            on:pointercancel=move |_| dragging.set(false)
            on:click=|ev| ev.stop_propagation()
        ></span>
    }
}

/// One block on one day row. Built when the block covers the day; its text and position follow
/// the rules in place.
#[component]
fn Bar(
    day: Day,
    block: usize,
    rules: Signal<ModeRules>,
    editable: bool,
    on_change: Callback<ModeRules>,
    selected: RwSignal<Option<usize>>,
) -> impl IntoView {
    let covers = Memo::new(move |_| {
        rules.with(|r| r.blocks.get(block).is_some_and(|b| b.days.contains(&day)))
    });
    let style = move || {
        rules.with(|r| {
            r.blocks.get(block).map_or_else(String::new, |b| {
                let left = f64::from(b.from.0) / f64::from(DAY_END) * 100.0;
                let width = f64::from(b.to.0.saturating_sub(b.from.0)) / f64::from(DAY_END) * 100.0;
                format!("left:{left:.3}%;width:{width:.3}%")
            })
        })
    };
    let text = move || {
        rules.with(|r| {
            r.blocks.get(block).map(|b| {
                let setpoint = setpoint_text(Some(b.setpoint));
                match &b.evidence {
                    Some(e) => format!("{setpoint} {}/{}", e.agree, e.of),
                    None => setpoint,
                }
            })
        })
    };
    view! {
        <Show when=move || covers.get()>
            <div
                class="climate-timeline__bar"
                class:climate-timeline__bar--editable=editable
                class:climate-timeline__bar--selected=move || selected.get() == Some(block)
                class:climate-timeline__bar--off=move || {
                    rules.with(|r| r.blocks.get(block).is_some_and(|b| b.setpoint == Setpoint::Off))
                }
                style=style
                role=editable.then_some("button")
                tabindex=editable.then_some("0")
                on:click=move |_| {
                    if editable {
                        selected.update(|s| *s = if *s == Some(block) { None } else { Some(block) });
                    }
                }
                on:keydown=move |ev: web_sys::KeyboardEvent| {
                    if editable && (ev.key() == "Enter" || ev.key() == " ") {
                        ev.prevent_default();
                        selected.set(Some(block));
                    }
                }
            >
                <span class="climate-timeline__label">{text}</span>
                {editable
                    .then(|| {
                        view! {
                            <Handle block=block edge=Edge::From rules=rules on_change=on_change />
                            <Handle block=block edge=Edge::To rules=rules on_change=on_change />
                        }
                    })}
            </div>
        </Show>
    }
}

fn time_options(selected: u16, from: u16) -> impl IntoView {
    (from..=DAY_END)
        .step_by(usize::from(STEP))
        .map(|m| view! { <option value=m.to_string() selected=m == selected>{hhmm_text(m)}</option> })
        .collect_view()
}

/// The popover of the selected block: its times, setpoint and delete.
#[component]
fn BlockEditor(
    block: usize,
    rules: Signal<ModeRules>,
    on_change: Callback<ModeRules>,
    selected: RwSignal<Option<usize>>,
) -> impl IntoView {
    let setpoint = move || rules.with(|r| r.blocks.get(block).map(|b| b.setpoint));
    let edge_at = move |edge: Edge| {
        rules.with_untracked(|r| {
            r.blocks.get(block).map_or(0, |b| match edge {
                Edge::From => b.from.0,
                Edge::To => b.to.0,
            })
        })
    };
    let pick = move |edge: Edge| {
        move |ev: web_sys::Event| {
            if let Ok(minute) = event_target_value(&ev).parse::<u16>() {
                let current = rules.get_untracked();
                let next = move_edge(&current, block, edge, HhMm(minute));
                if next != current {
                    on_change.run(next);
                }
            }
        }
    };
    let is_off = move || setpoint() == Some(Setpoint::Off);
    let temp = move || match setpoint() {
        Some(Setpoint::Temp(t)) => t.to_string(),
        _ => String::new(),
    };
    view! {
        <div class="climate-timeline__editor card">
            <label class="climate-field">
                <span class="climate-field__label">"From"</span>
                // A block can start up to one step before its end.
                <select class="field" on:change=pick(Edge::From)>
                    {time_options(edge_at(Edge::From), 0)}
                </select>
            </label>
            <label class="climate-field">
                <span class="climate-field__label">"To"</span>
                <select class="field" on:change=pick(Edge::To)>
                    {time_options(edge_at(Edge::To), STEP)}
                </select>
            </label>
            <label class="climate-field">
                <span class="climate-field__label">"Temperature"</span>
                <input
                    class="field"
                    type="number"
                    step="0.5"
                    min="0"
                    max="40"
                    disabled=is_off
                    prop:value=temp
                    on:change=move |ev| {
                        if let Ok(t) = event_target_value(&ev).trim().parse::<f64>() {
                            let next = set_setpoint(&rules.get_untracked(), block, Setpoint::Temp(t));
                            on_change.run(next);
                        }
                    }
                />
            </label>
            <label class="climate-field climate-field--inline">
                <input
                    type="checkbox"
                    prop:checked=is_off
                    on:change=move |ev| {
                        let sp = if event_target_checked(&ev) {
                            Setpoint::Off
                        } else {
                            Setpoint::Temp(20.0)
                        };
                        on_change.run(set_setpoint(&rules.get_untracked(), block, sp));
                    }
                />
                <span>"Off"</span>
            </label>
            <div class="climate-timeline__editor-actions">
                <button
                    type="button"
                    class="btn btn--compact"
                    on:click=move |_| {
                        selected.set(None);
                        on_change.run(delete_block(&rules.get_untracked(), block));
                    }
                >
                    "Delete block"
                </button>
                <button type="button" class="btn btn--compact" on:click=move |_| selected.set(None)>
                    "Done"
                </button>
            </div>
        </div>
    }
}

/// Seven rows (Mon to Sun) by 24 hours for one mode's rules.
#[component]
pub fn WeekTimeline(
    rules: Signal<ModeRules>,
    editable: bool,
    on_change: Callback<ModeRules>,
) -> impl IntoView {
    let selected = RwSignal::new(None::<usize>);
    let count = move || rules.with(|r| r.blocks.len());
    let base = move || rules.with(|r| setpoint_text(Some(r.base)));
    let rows = Day::ALL
        .into_iter()
        .map(|day| {
            view! {
                <div class="climate-timeline__row">
                    <span class="climate-timeline__day">{day.short()}</span>
                    <div class="climate-timeline__track">
                        <For each=move || 0..count() key=|block| *block let:block>
                            <Bar
                                day=day
                                block=block
                                rules=rules
                                editable=editable
                                on_change=on_change
                                selected=selected
                            />
                        </For>
                    </div>
                </div>
            }
        })
        .collect_view();
    let hours = [0u16, 6, 12, 18, 24]
        .into_iter()
        .map(|h| view! { <span>{format!("{h:02}")}</span> })
        .collect_view();
    view! {
        <div class="climate-timeline">
            <p class="climate-timeline__base">
                "Base " <strong>{base}</strong> " outside the blocks"
            </p>
            <div class="climate-timeline__row climate-timeline__row--axis">
                <span class="climate-timeline__day"></span>
                <div class="climate-timeline__hours">{hours}</div>
            </div>
            {rows}
            {move || {
                selected
                    .get()
                    .filter(|_| editable)
                    .map(|block| {
                        view! {
                            <BlockEditor block=block rules=rules on_change=on_change selected=selected />
                        }
                    })
            }}
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oikos_climate::doc::{Block, Rules};

    fn block(days: &[Day], from: &str, to: &str, t: f64) -> Block {
        Block {
            days: days.to_vec(),
            from: HhMm::parse(from).unwrap(),
            to: HhMm::parse(to).unwrap(),
            setpoint: Setpoint::Temp(t),
            evidence: None,
        }
    }

    fn mode(blocks: Vec<Block>) -> ModeRules {
        ModeRules {
            base: Setpoint::Temp(17.0),
            blocks,
        }
    }

    fn valid(m: &ModeRules) {
        Rules {
            heat: Some(m.clone()),
            cool: None,
        }
        .validate()
        .expect("still valid");
    }

    fn at(s: &str) -> HhMm {
        HhMm::parse(s).unwrap()
    }

    #[test]
    fn move_edge_snaps_to_15_min() {
        let m = mode(vec![block(&[Day::Mon], "07:00", "09:00", 21.0)]);
        let a = move_edge(&m, 0, Edge::From, at("06:53"));
        assert_eq!(a.blocks[0].from, at("07:00"));
        let b = move_edge(&m, 0, Edge::From, at("06:38"));
        assert_eq!(b.blocks[0].from, at("06:45"));
        let c = move_edge(&m, 0, Edge::To, at("10:07"));
        assert_eq!(c.blocks[0].to, at("10:00"));
        let d = move_edge(&m, 0, Edge::To, at("23:59"));
        assert_eq!(d.blocks[0].to, at("24:00"));
        for r in [a, b, c, d] {
            valid(&r);
        }
    }

    #[test]
    fn move_edge_clamps_to_neighbour() {
        // Block 0 shares Monday with both neighbours; block 3 is on another day.
        let m = mode(vec![
            block(&[Day::Mon, Day::Tue], "09:00", "11:00", 21.0),
            block(&[Day::Mon], "06:00", "08:10", 19.0),
            block(&[Day::Mon], "12:00", "14:00", 19.0),
            block(&[Day::Wed], "08:30", "08:45", 19.0),
        ]);
        let earlier = move_edge(&m, 0, Edge::From, at("05:00"));
        assert_eq!(earlier.blocks[0].from, at("08:10"));
        let later = move_edge(&m, 0, Edge::To, at("13:00"));
        assert_eq!(later.blocks[0].to, at("12:00"));
        // Wednesday has no neighbour that blocks the move.
        let free = move_edge(&m, 3, Edge::To, at("20:00"));
        assert_eq!(free.blocks[3].to, at("20:00"));
        // Block 1 shares Monday with block 0, so its end stops at 09:00.
        let open = move_edge(&m, 1, Edge::To, at("08:55"));
        assert_eq!(open.blocks[1].to, at("09:00"));
        for r in [earlier, later, free, open] {
            valid(&r);
        }
    }

    #[test]
    fn move_edge_keeps_from_before_to() {
        let m = mode(vec![block(&[Day::Mon], "07:00", "09:00", 21.0)]);
        let a = move_edge(&m, 0, Edge::From, at("12:00"));
        assert_eq!(a.blocks[0].from, at("08:45"));
        let b = move_edge(&m, 0, Edge::To, at("03:00"));
        assert_eq!(b.blocks[0].to, at("07:15"));
        let c = move_edge(&m, 0, Edge::From, at("00:00"));
        assert_eq!(c.blocks[0].from, at("00:00"));
        // Unknown block: unchanged.
        assert_eq!(move_edge(&m, 5, Edge::To, at("10:00")), m);
        for r in [a, b, c] {
            valid(&r);
        }
    }

    #[test]
    fn set_setpoint_off() {
        let m = mode(vec![
            block(&[Day::Mon], "07:00", "09:00", 21.0),
            block(&[Day::Tue], "07:00", "09:00", 21.0),
        ]);
        let off = set_setpoint(&m, 1, Setpoint::Off);
        assert_eq!(off.blocks[1].setpoint, Setpoint::Off);
        assert_eq!(off.blocks[0].setpoint, Setpoint::Temp(21.0));
        let hot = set_setpoint(&m, 0, Setpoint::Temp(99.0));
        assert_eq!(hot.blocks[0].setpoint, Setpoint::Temp(40.0));
        assert_eq!(set_setpoint(&m, 0, Setpoint::Temp(f64::NAN)), m);
        valid(&off);
        valid(&hot);
    }

    #[test]
    fn delete_block_keeps_others() {
        let m = mode(vec![
            block(&[Day::Mon], "07:00", "09:00", 21.0),
            block(&[Day::Tue], "08:00", "10:00", 20.0),
            block(&[Day::Wed], "09:00", "11:00", 19.0),
        ]);
        let out = delete_block(&m, 1);
        assert_eq!(out.blocks, vec![m.blocks[0].clone(), m.blocks[2].clone()]);
        assert_eq!(out.base, m.base);
        assert_eq!(delete_block(&m, 9), m);
        valid(&out);
    }
}
