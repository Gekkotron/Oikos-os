//! One zone of the Climate screen: reading, target, state, overrides, learning and decisions.
//!
//! The card is built once per zone and reads its data from a signal, so a refetch updates the
//! text in place and keeps what the person is doing (the open decision list, the hold input).

use chrono_tz::Tz;
use leptos::prelude::*;
use leptos::task::spawn_local;
use oikos_api::ApiError;
use oikos_climate::doc::{Mode, Override, OverrideOrigin, Setpoint, ZoneState};
use oikos_climate::view::{DecisionLog, ZoneView};

use crate::api::climate::{self as api, OverrideBody};
use crate::climate::{
    age_text, decision_line, degrees_text, learn_status_text, minutes_to_next_change, mode_word,
    row_line, setpoint_text, source_word, until_text, week_minute,
};
use crate::components::now::unix_now;
use crate::components::toast::Toasts;

/// How many recent decisions the card lists before the older rows.
const RECENT: usize = 10;
/// How many decisions the hub is asked for when the list opens.
const FETCH: u32 = 50;

const STATES: [(ZoneState, &str); 3] = [
    (ZoneState::Off, "Off"),
    (ZoneState::Pending, "Pending"),
    (ZoneState::Active, "Active"),
];

fn override_text(o: &Override, tz: Tz) -> String {
    let what = match (o.setpoint, o.mode) {
        (Some(sp), Some(m)) => format!("{} {}", mode_word(Some(m)), setpoint_text(Some(sp))),
        (Some(sp), None) => setpoint_text(Some(sp)),
        (None, m) => mode_word(m).to_owned(),
    };
    let by = match o.origin {
        OverrideOrigin::Hand => "set on a device",
        OverrideOrigin::User => "set here",
    };
    format!(
        "{what} until {} · {by}",
        until_text(o.until, unix_now(), tz)
    )
}

/// A typed temperature, when it is a usable number.
fn parse_setpoint(text: &str) -> Option<f64> {
    text.trim().parse::<f64>().ok().filter(|t| t.is_finite())
}

/// Runs `work`, then refreshes the screen; a failure becomes a toast.
fn act<F>(toasts: Toasts, on_changed: Callback<()>, work: F)
where
    F: std::future::Future<Output = Result<(), ApiError>> + 'static,
{
    spawn_local(async move {
        match work.await {
            Ok(()) => on_changed.run(()),
            Err(e) => toasts.push(e.error),
        }
    });
}

#[component]
pub fn ZoneCard(
    zone: Signal<ZoneView>,
    tz: Signal<Tz>,
    /// Called after any change so the screen refetches.
    on_changed: Callback<()>,
    /// Task 12 puts the week timeline and the proposal panel here.
    #[prop(optional)]
    children: Option<Children>,
) -> impl IntoView {
    let toasts = expect_context::<Toasts>();
    let id = zone.with_untracked(|z| z.zone.clone());

    let reading = move || {
        zone.with(|z| match (z.reading, z.reading_age_s) {
            (Some(t), Some(age)) => format!("{} · {}", degrees_text(t), age_text(age)),
            (Some(t), None) => degrees_text(t),
            _ => "No reading".to_owned(),
        })
    };
    let target = move || {
        zone.with(|z| {
            z.decision.as_ref().map_or_else(
                || "No decision yet".to_owned(),
                |d| format!("{} {}", mode_word(d.mode), setpoint_text(d.setpoint)),
            )
        })
    };
    let source = move || {
        zone.with(|z| {
            z.decision
                .as_ref()
                .map(|d| (source_word(d.source), d.reason.clone()))
        })
    };

    // State switch: a group of toggle buttons, the current one pressed.
    let state_buttons = STATES
        .into_iter()
        .map(|(state, text)| {
            let id = id.clone();
            let selected = move || zone.with(|z| z.settings.state == state);
            view! {
                <button
                    type="button"
                    class="chip segmented__option"
                    class:chip--selected=selected
                    aria-pressed=move || selected().to_string()
                    on:click=move |_| {
                        if selected() {
                            return;
                        }
                        let mut next = zone.with_untracked(|z| z.settings.clone());
                        next.state = state;
                        let id = id.clone();
                        act(toasts, on_changed, async move { api::put_zone(&id, &next).await });
                    }
                >
                    {text}
                </button>
            }
        })
        .collect_view();

    // Overrides.
    let clear_id = id.clone();
    let overrides = move || {
        let list = zone.with(|z| z.overrides.clone());
        let tz = tz.get();
        (!list.is_empty()).then(|| {
            let lines = list
                .iter()
                .map(|o| view! { <li>{override_text(o, tz)}</li> })
                .collect_view();
            let id = clear_id.clone();
            view! {
                <div class="climate-overrides">
                    <ul class="climate-overrides__list">{lines}</ul>
                    <button
                        type="button"
                        class="btn btn--compact"
                        on:click=move |_| {
                            let id = id.clone();
                            act(toasts, on_changed, async move { api::clear_override(&id).await });
                        }
                    >
                        "Clear"
                    </button>
                </div>
            }
        })
    };

    // Quick override: a setpoint, held for a while.
    let start = zone.with_untracked(|z| match z.decision.as_ref().and_then(|d| d.setpoint) {
        Some(Setpoint::Temp(t)) => Some(t),
        _ => z.reading.map(f64::round),
    });
    let setpoint = RwSignal::new(start.map(|t| t.to_string()).unwrap_or_default());
    let until_change = move || {
        zone.with(|z| {
            let mode = z
                .decision
                .as_ref()
                .and_then(|d| d.mode)
                .unwrap_or(Mode::Heat);
            let (day, minute) = week_minute(unix_now(), tz.get());
            minutes_to_next_change(&z.settings.rules, mode, day, minute)
        })
    };
    let hold = {
        let id = id.clone();
        move |minutes: u32| {
            let Some(t) = parse_setpoint(&setpoint.get_untracked()) else {
                toasts.push("Enter a target temperature first");
                return;
            };
            let id = id.clone();
            let body = OverrideBody {
                setpoint: Some(Setpoint::Temp(t)),
                minutes: Some(minutes),
                ..OverrideBody::default()
            };
            act(toasts, on_changed, async move {
                api::set_override(&id, body).await
            });
        }
    };
    let hold_1h = hold.clone();
    let hold_2h = hold.clone();
    let hold_next = hold;

    // Learning.
    let learn_line = move || zone.with(|z| learn_status_text(&z.learn));
    let can_learn = move || zone.with(|z| z.settings.state != ZoneState::Off);
    let learn_id = id.clone();

    // Decisions: fetched when the list opens, and again whenever the zone's data changes while
    // it is open.
    let open = RwSignal::new(false);
    let log = RwSignal::new(None::<DecisionLog>);
    let decisions_id = id.clone();
    Effect::new(move |_| {
        zone.track();
        if open.get() {
            let id = decisions_id.clone();
            spawn_local(async move {
                match api::decisions(&id, FETCH).await {
                    Ok(l) => {
                        log.try_set(Some(l));
                    }
                    Err(e) => toasts.push(e.error),
                }
            });
        }
    });
    let list = move || {
        let tz = tz.get();
        log.get().map(|l| {
            let recent = l
                .recent
                .iter()
                .take(RECENT)
                .map(|d| view! { <li>{decision_line(d, tz)}</li> })
                .collect_view();
            let older = l
                .older
                .iter()
                .map(|r| view! { <li class="climate-decisions__older">{row_line(r, tz)}</li> })
                .collect_view();
            view! { <ul class="climate-decisions__list">{recent}{older}</ul> }
        })
    };

    view! {
        <article class="card climate-zone" class:climate-zone--orphan=move || zone.with(|z| z.orphan)>
            <header class="climate-zone__header">
                <h2 class="climate-zone__name">{move || zone.with(|z| z.name.clone())}</h2>
                {move || {
                    zone.with(|z| z.orphan).then(|| view! { <span class="badge">"No climate device"</span> })
                }}
                <span class="climate-zone__reading">{reading}</span>
            </header>
            {move || {
                zone.with(|z| z.settings_invalid.clone())
                    .map(|e| view! { <p class="badge climate-zone__invalid">{e}</p> })
            }}
            <div class="climate-zone__target">
                <span class="climate-zone__target-label">"Target"</span>
                <strong>{target}</strong>
                {move || {
                    source()
                        .map(|(word, reason)| {
                            view! {
                                <span class="chip climate-zone__source">{word}</span>
                                <span class="climate-zone__reason">{reason}</span>
                            }
                        })
                }}
            </div>
            <div class="segmented climate-zone__state" role="group" aria-label="Zone state">
                {state_buttons}
            </div>
            {overrides}
            <div class="climate-hold">
                <input
                    class="field climate-hold__setpoint"
                    type="number"
                    step="0.5"
                    aria-label="Hold temperature"
                    prop:value=move || setpoint.get()
                    on:input=move |ev| setpoint.set(event_target_value(&ev))
                />
                <button type="button" class="btn" on:click=move |_| hold_1h(60)>"1 h"</button>
                <button type="button" class="btn" on:click=move |_| hold_2h(120)>"2 h"</button>
                {move || {
                    until_change()
                        .map(|m| {
                            let hold_next = hold_next.clone();
                            view! {
                                <button type="button" class="btn" on:click=move |_| hold_next(m)>
                                    "Until next change"
                                </button>
                            }
                        })
                }}
            </div>
            <div class="climate-learn">
                <span class="climate-learn__text">{learn_line}</span>
                {move || {
                    can_learn()
                        .then(|| {
                            let learn_id = learn_id.clone();
                            view! {
                                <button
                                    type="button"
                                    class="btn btn--compact"
                                    on:click=move |_| {
                                        let id = learn_id.clone();
                                        act(
                                            toasts,
                                            on_changed,
                                            async move { api::learn(&id).await.map(|_| ()) },
                                        );
                                    }
                                >
                                    "Learn now"
                                </button>
                            }
                        })
                }}
            </div>
            // Task 12: the week timeline and the proposal panel go into this slot.
            {children.map(|c| view! { <div class="climate-zone__extra">{c()}</div> })}
            <div class="climate-decisions">
                <button
                    type="button"
                    class="chip"
                    aria-expanded=move || open.get().to_string()
                    on:click=move |_| open.update(|o| *o = !*o)
                >
                    {move || if open.get() { "Hide decisions" } else { "Recent decisions" }}
                </button>
                <Show when=move || open.get()>{list}</Show>
            </div>
        </article>
    }
}
