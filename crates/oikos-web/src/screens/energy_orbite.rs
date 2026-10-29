//! The Orbite view of the Energy screen: two concentric rings round the house. The inner ring is
//! where the power comes from, the outer ring where it goes; `energy_orbite` lays the arcs out.
//!
//! Every arc and row is a keyed child that reads its own data, so a new reading moves the existing
//! elements and CSS glides the arcs.

use leptos::prelude::*;
use oikos_core::energy::EnergyUi;

use crate::energy::{aria_sentence, power_text};
use crate::energy_orbite::{Arc, Ring, Rings, arc_path, colour_var, rings};
use crate::energy_sankey::{Sankey, SankeyRole, build};
use crate::i18n::energy::{EnergyText, energy_text as word, solar_share_line};
use crate::i18n::{Lang, use_lang};
use crate::icons::HOUSE;
use crate::screens::energy_flux::{fold_at, node_name};

/// The svg's square side, and the ring centre.
const SIZE: f64 = 340.0;
const CENTRE: f64 = SIZE / 2.0;
const INNER_R: f64 = 104.0;
const INNER_W: f64 = 14.0;
const OUTER_R: f64 = 136.0;
const OUTER_W: f64 = 22.0;

fn geometry(ring: Ring) -> (f64, f64) {
    match ring {
        Ring::Inner => (INNER_R, INNER_W),
        Ring::Outer => (OUTER_R, OUTER_W),
    }
}

fn find(r: &Rings, id: &str) -> Option<Arc> {
    r.inner.iter().chain(&r.outer).find(|a| a.id == id).cloned()
}

/// What an arc does besides carrying its name: "production", "↘ import", "↗ export"...
fn note(lang: Lang, role: SankeyRole) -> String {
    let (arrow, key) = match role {
        SankeyRole::Solar => ("", EnergyText::CircleProduction),
        SankeyRole::Grid => ("↘ ", EnergyText::CircleImport),
        SankeyRole::Export => ("↗ ", EnergyText::CircleExport),
        SankeyRole::Battery => ("↘ ", EnergyText::CircleDischarging),
        SankeyRole::BatteryCharge => ("↗ ", EnergyText::CircleCharging),
        _ => return String::new(),
    };
    format!("{arrow}{}", word(lang, key))
}

/// An Others arc opens the full list, any other arc its single sensor, when it has one.
fn activate(arc: &Arc, open: Callback<String>, expanded: RwSignal<bool>) {
    if arc.role == SankeyRole::Others {
        expanded.update(|e| *e = !*e);
    } else if let Some(s) = arc.sensor.clone() {
        open.run(s);
    }
}

fn clickable(arc: &Arc) -> bool {
    arc.sensor.is_some() || arc.role == SankeyRole::Others
}

#[component]
fn OrbiteArc(
    id: String,
    rings: Memo<Rings>,
    open: Callback<String>,
    expanded: RwSignal<bool>,
    hot: RwSignal<Option<String>>,
) -> impl IntoView {
    let arc_id = id.clone();
    let arc = Memo::new(move |_| rings.with(|r| find(r, &arc_id)));
    let Some(first) = arc.get_untracked() else {
        return ().into_any();
    };
    let can_click = clickable(&first);
    let (radius, width) = geometry(first.ring);
    let path = move || {
        arc.with(|a| {
            a.as_ref()
                .map(|a| arc_path(CENTRE, CENTRE, radius, a.a0, a.a1))
                .unwrap_or_default()
        })
    };
    let colour = move || {
        let (role, slot) = arc.with(|a| {
            a.as_ref()
                .map_or((first.role, first.slot), |a| (a.role, a.slot))
        });
        format!("var({})", colour_var(role, slot))
    };
    let style = move || format!("d: path(\"{}\"); color: {}", path(), colour());
    let hot_id = id.clone();
    let enter_id = id.clone();
    view! {
        <path
            class="energy-orbite__arc"
            class:energy-orbite__arc--click=can_click
            class:energy-orbite__arc--hot=move || hot.get().as_deref() == Some(hot_id.as_str())
            data-id=id
            aria-hidden="true"
            fill="none"
            stroke=colour
            stroke-width=width
            d=path
            style=style
            on:mouseenter=move |_| hot.set(Some(enter_id.clone()))
            on:mouseleave=move |_| hot.set(None)
            on:click=move |_| {
                if let Some(a) = arc.get_untracked() {
                    activate(&a, open, expanded);
                }
            }
        />
    }
    .into_any()
}

#[component]
fn OrbiteRow(
    id: String,
    model: Memo<Sankey>,
    rings: Memo<Rings>,
    open: Callback<String>,
    expanded: RwSignal<bool>,
    hot: RwSignal<Option<String>>,
) -> impl IntoView {
    let lang = use_lang();
    let arc_id = id.clone();
    let arc = Memo::new(move |_| rings.with(|r| find(r, &arc_id)));
    let Some(first) = arc.get_untracked() else {
        return ().into_any();
    };
    let can_click = clickable(&first);
    let swatch = move || {
        let (role, slot) = arc.with(|a| {
            a.as_ref()
                .map_or((first.role, first.slot), |a| (a.role, a.slot))
        });
        format!("background: var({})", colour_var(role, slot))
    };
    let name_id = id.clone();
    let name = move || model.with(|m| node_name(lang(), m, &name_id, false));
    let note_text = move || {
        let role = arc.with(|a| a.as_ref().map_or(first.role, |a| a.role));
        note(lang(), role)
    };
    let value = move || {
        let watts = arc.with(|a| a.as_ref().map_or(0.0, |a| a.watts));
        power_text(lang(), Some(watts))
    };
    let share = move || arc.with(|a| a.as_ref().map_or(0, |a| a.share));
    let hot_id = id.clone();
    let enter_id = id.clone();
    let focus_id = id.clone();
    view! {
        <button
            type="button"
            class="energy-orbite__row"
            class:energy-orbite__row--hot=move || hot.get().as_deref() == Some(hot_id.as_str())
            data-id=id
            class:energy-orbite__row--static=!can_click
            on:mouseenter=move |_| hot.set(Some(enter_id.clone()))
            on:mouseleave=move |_| hot.set(None)
            on:focus=move |_| hot.set(Some(focus_id.clone()))
            on:blur=move |_| hot.set(None)
            on:click=move |_| {
                if let Some(a) = arc.get_untracked() {
                    activate(&a, open, expanded);
                }
            }
        >
            <span class="energy-orbite__swatch" style=swatch></span>
            <span class="energy-orbite__who">
                <span class="energy-orbite__name">{name}</span>
                <span class="energy-orbite__note">{note_text}</span>
            </span>
            <span class="energy-orbite__nums">
                <span class="energy-orbite__val">{value}</span>
                <span class="energy-orbite__pct">{move || format!("{}\u{A0}%", share())}</span>
            </span>
        </button>
    }
    .into_any()
}

#[component]
pub fn OrbiteDiagram(
    ui: Memo<EnergyUi>,
    narrow: bool,
    open: Callback<String>,
    expanded: RwSignal<bool>,
) -> impl IntoView {
    let lang = use_lang();
    let model = Memo::new(move |_| ui.with(|u| build(u, fold_at(narrow))));
    let ring_set = Memo::new(move |_| model.with(rings));
    let hot = RwSignal::new(None::<String>);
    let description = move || ui.with(|u| aria_sentence(lang(), u));
    let empty = move || model.with(Sankey::is_empty);

    let inner = move || ring_set.with(|r| r.inner.iter().map(|a| a.id.clone()).collect::<Vec<_>>());
    let outer = move || ring_set.with(|r| r.outer.iter().map(|a| a.id.clone()).collect::<Vec<_>>());

    let house = move || {
        model.with(|m| {
            m.house_w.map(
                |w| view! { <b class="energy-orbite__house">{power_text(lang(), Some(w))}</b> },
            )
        })
    };
    let mix = move || {
        model.with(|m| {
            m.solar_share.filter(|n| *n > 0).map(|n| {
                view! { <span class="energy-orbite__mix">{solar_share_line(lang(), n)}</span> }
            })
        })
    };

    let row = move |id: String| {
        view! { <OrbiteRow id=id model=model rings=ring_set open=open expanded=expanded hot=hot /> }
    };
    let arc = move |id: String| {
        view! { <OrbiteArc id=id rings=ring_set open=open expanded=expanded hot=hot /> }
    };

    view! {
        <div
            class="energy-orbite"
            class:energy-orbite--narrow=narrow
            class:energy-orbite--empty=empty
            class:energy-orbite--dim=move || hot.with(Option::is_some)
        >
            <div class="energy-orbite__ring">
                <svg
                    class="energy-orbite__svg"
                    viewBox=format!("0 0 {SIZE} {SIZE}")
                    role="group"
                    aria-label=description
                >
                    <title>{description}</title>
                    <circle
                        class="energy-orbite__track"
                        cx=CENTRE
                        cy=CENTRE
                        r=INNER_R
                        stroke-width=INNER_W
                    />
                    <circle
                        class="energy-orbite__track"
                        cx=CENTRE
                        cy=CENTRE
                        r=OUTER_R
                        stroke-width=OUTER_W
                    />
                    <For each=inner key=|id| id.clone() children=arc />
                    <For each=outer key=|id| id.clone() children=arc />
                    <g class="energy-orbite__dots"></g>
                </svg>
                <div class="energy-orbite__centre">
                    <span class="energy-orbite__ico" inner_html=HOUSE></span>
                    {house}
                    <Show when=move || model.with(|m| m.house_w.is_some())>
                        <span class="energy-orbite__small">
                            {move || word(lang(), EnergyText::InTheHouse)}
                        </span>
                    </Show>
                    {mix}
                </div>
            </div>
            <Show
                when=move || !empty()
                fallback=move || {
                    view! {
                        <p class="energy-orbite__empty">
                            {move || word(lang(), EnergyText::NoLivePower)}
                        </p>
                    }
                }
            >
                <section class="energy-orbite__list energy-orbite__list--in">
                    <h3>{move || word(lang(), EnergyText::ComingIn)}</h3>
                    <For each=inner key=|id| id.clone() children=row />
                </section>
                <section class="energy-orbite__list energy-orbite__list--out">
                    <h3>{move || word(lang(), EnergyText::GoingOut)}</h3>
                    <For each=outer key=|id| id.clone() children=row />
                </section>
            </Show>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_say_what_each_arc_does() {
        assert_eq!(note(Lang::En, SankeyRole::Solar), "production");
        assert_eq!(note(Lang::En, SankeyRole::Grid), "↘ import");
        assert_eq!(note(Lang::En, SankeyRole::Export), "↗ export");
        assert_eq!(note(Lang::En, SankeyRole::Consumer), "");
    }

    #[test]
    fn rings_have_room_for_their_strokes() {
        let (r, w) = geometry(Ring::Outer);
        assert!(r + w / 2.0 <= CENTRE);
    }
}
