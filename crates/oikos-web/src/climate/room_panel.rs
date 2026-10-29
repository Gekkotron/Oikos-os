//! One room of the Climate screen: its status (reading, target, state, holds), its week, its
//! proposal, learning, thermometer and decisions.
//!
//! The panel is built once per zone and reads its data from a signal, so a refetch updates the
//! text in place and keeps what the person is doing (the open decision list, the hold input).

use chrono_tz::Tz;
use leptos::prelude::*;
use leptos::task::spawn_local;
use oikos_api::ApiError;
use oikos_climate::doc::{Mode, Override, OverrideOrigin, Setpoint, ZoneState};
use oikos_climate::view::{DecisionLog, ZoneView};

use crate::api::climate::{self as api, OverrideBody};
use crate::climate::correction_card::CorrectionCard;
use crate::climate::entity_field::ThermometerField;
use crate::climate::proposal::{ProposalPanel, RulesPanel};
use crate::climate::{
    age_text, can_hold, decision_line, degrees_text, eco_end_text, eco_until, learn_status_text,
    minutes_to_next_change, mode_word, row_line, setpoint_text, source_word, until_text,
    week_minute,
};
use crate::components::now::{Now, unix_now};
use crate::components::toast::Toasts;
use crate::i18n::Lang;
use crate::i18n::climate::{
    ClimateText, climate_text, eco_until_line, hold_until, now_with_age, use_climate_text,
};
use crate::i18n::format::{input_number, parse_decimal};
use crate::i18n::{current_lang, use_lang};

/// How many recent decisions the card lists before the older rows.
const RECENT: usize = 10;
/// How many decisions the hub is asked for when the list opens.
const FETCH: u32 = 50;

/// A thermometer reading this old shows its age under the reading (15 min).
const THERMOMETER_AGE_SHOWN_S: u64 = 15 * 60;

const STATES: [(ZoneState, ClimateText); 3] = [
    (ZoneState::Off, ClimateText::ZoneOff),
    (ZoneState::Pending, ClimateText::ZonePending),
    (ZoneState::Active, ClimateText::ZoneActive),
];

fn override_text(lang: Lang, o: &Override, tz: Tz) -> String {
    let what = match (o.setpoint, o.mode) {
        (Some(sp), Some(m)) => format!(
            "{} {}",
            mode_word(lang, Some(m)),
            setpoint_text(lang, Some(sp))
        ),
        (Some(sp), None) => setpoint_text(lang, Some(sp)),
        (None, m) => mode_word(lang, m),
    };
    let by = match o.origin {
        OverrideOrigin::Hand => ClimateText::SetOnDevice,
        OverrideOrigin::User => ClimateText::SetHere,
    };
    hold_until(lang, &what, &until_text(lang, o.until, unix_now(), tz), by)
}

/// A typed temperature ("19.5" or "19,5"), when it is a usable number.
fn parse_setpoint(text: &str) -> Option<f64> {
    parse_decimal(text)
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
pub fn RoomPanel(
    zone: Signal<ZoneView>,
    tz: Signal<Tz>,
    /// Called after any change so the screen refetches.
    on_changed: Callback<()>,
    /// Whether the panel is on screen; a hidden panel leaves its decisions unfetched. Shown when
    /// absent.
    #[prop(optional, into)]
    shown: Option<Signal<bool>>,
) -> impl IntoView {
    let toasts = expect_context::<Toasts>();
    let lang = use_lang();
    let t = use_climate_text();
    let id = zone.with_untracked(|z| z.zone.clone());
    let clock = expect_context::<Now>().0;
    // A memo, so the calendars redraw their now line only when the minute or the day changes.
    let now = Memo::new(move |_| week_minute(clock.get(), tz.get()));

    let reading = move || {
        zone.with(|z| {
            z.reading.map_or_else(
                || t(ClimateText::NoReading).to_owned(),
                |r| degrees_text(lang(), r),
            )
        })
    };
    let reading_label = move || {
        zone.with(|z| match (z.reading, z.reading_age_s) {
            (Some(_), Some(age)) => now_with_age(lang(), &age_text(lang(), age)),
            _ => t(ClimateText::Now).to_owned(),
        })
    };
    // The room thermometer's own value and age, when it is older than the reading's freshness.
    let thermometer_line = move || {
        zone.with(|z| {
            z.thermometer
                .as_ref()
                .filter(|th| th.age_s >= THERMOMETER_AGE_SHOWN_S)
                .map(|th| {
                    format!(
                        "{} · {}",
                        degrees_text(lang(), th.value),
                        age_text(lang(), th.age_s)
                    )
                })
        })
        .map(|line| view! { <span class="climate-status__label">{line}</span> })
    };
    let target = move || {
        zone.with(|z| {
            z.decision.as_ref().map_or_else(
                || t(ClimateText::NoDecision).to_owned(),
                |d| {
                    format!(
                        "{} {}",
                        mode_word(lang(), d.mode),
                        setpoint_text(lang(), d.setpoint)
                    )
                },
            )
        })
    };
    let source = move || {
        zone.with(|z| {
            z.decision
                .as_ref()
                .map(|d| (source_word(lang(), d.source), d.reason.clone()))
        })
    };

    // State switch: a group of toggle buttons, the current one pressed.
    let state_buttons = STATES
        .into_iter()
        .map(|(state, word)| {
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
                    {move || t(word)}
                </button>
            }
        })
        .collect_view();

    // Room thermometer: none on the zone lets the hub pick one in the room.
    let sensor_id = id.clone();
    let set_sensor = Callback::new(move |sensor: Option<String>| {
        let mut next = zone.with_untracked(|z| z.settings.clone());
        next.sensor = sensor;
        let id = sensor_id.clone();
        act(toasts, on_changed, async move {
            api::put_zone(&id, &next).await
        });
    });

    // Overrides.
    let clear_id = id.clone();
    let overrides = move || {
        let list = zone.with(|z| z.overrides.clone());
        let tz = tz.get();
        let lang = lang();
        (!list.is_empty()).then(|| {
            let lines = list
                .iter()
                .map(|o| view! { <li>{override_text(lang, o, tz)}</li> })
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
                        {move || t(ClimateText::Clear)}
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
    let setpoint = RwSignal::new(
        start
            .map(|v| input_number(current_lang(), v))
            .unwrap_or_default(),
    );
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
                toasts.push(climate_text(untrack(lang), ClimateText::EnterTarget));
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
    // A memo, so a refetch that keeps the state does not rebuild the hold input being typed in.
    let holdable = Memo::new(move |_| zone.with(|z| can_hold(z.settings.state)));

    // Eco until a date: the room holds the house eco setpoint, then its schedule again.
    let eco_open = RwSignal::new(false);
    let eco_date = RwSignal::new(String::new());
    let eco_time = RwSignal::new("00:00".to_owned());
    let eco_allowed = Memo::new(move |_| zone.with(|z| z.settings.state != ZoneState::Off));
    let eco_live = move || {
        let now = clock.get();
        zone.with(|z| z.eco.clone()).filter(|e| e.until > now)
    };
    let start_eco = {
        let id = id.clone();
        move || {
            let until = eco_until(
                &eco_date.get_untracked(),
                &eco_time.get_untracked(),
                tz.get_untracked(),
            )
            .filter(|until| *until > unix_now());
            let Some(until) = until else {
                toasts.push(climate_text(untrack(lang), ClimateText::EcoPickDate));
                return;
            };
            eco_open.set(false);
            let id = id.clone();
            act(
                toasts,
                on_changed,
                async move { api::set_eco(&id, until).await },
            );
        }
    };
    let cancel_eco = {
        let id = id.clone();
        move || {
            let id = id.clone();
            act(toasts, on_changed, async move { api::clear_eco(&id).await });
        }
    };
    let open_eco = move || {
        // Tomorrow by default, in the house's time zone.
        let tomorrow = unix_now() + 86_400;
        let date = chrono::DateTime::from_timestamp(i64::try_from(tomorrow).unwrap_or(0), 0)
            .map(|t| {
                t.with_timezone(&tz.get_untracked())
                    .format("%Y-%m-%d")
                    .to_string()
            })
            .unwrap_or_default();
        eco_date.set(date);
        eco_time.set("00:00".to_owned());
        eco_open.set(true);
    };
    let eco_row = move || {
        eco_allowed.get().then(|| {
            let start_eco = start_eco.clone();
            let cancel_eco = cancel_eco.clone();
            let body = match eco_live() {
                Some(eco) => {
                    let line = eco_until_line(lang(), &eco_end_text(lang(), eco.until, tz.get()));
                    view! {
                        <span class="chip climate-eco__line">{line}</span>
                        <button
                            type="button"
                            class="btn btn--compact"
                            on:click=move |_| cancel_eco()
                        >
                            {move || t(ClimateText::CancelEco)}
                        </button>
                    }
                    .into_any()
                }
                None if eco_open.get() => view! {
                    <label class="climate-eco__field">
                        <span>{move || t(ClimateText::EcoUntilDate)}</span>
                        <input
                            class="field"
                            type="date"
                            prop:value=move || eco_date.get()
                            on:input=move |ev| eco_date.set(event_target_value(&ev))
                        />
                    </label>
                    <label class="climate-eco__field">
                        <span>{move || t(ClimateText::EcoUntilTime)}</span>
                        <input
                            class="field"
                            type="time"
                            prop:value=move || eco_time.get()
                            on:input=move |ev| eco_time.set(event_target_value(&ev))
                        />
                    </label>
                    <button type="button" class="btn btn--active" on:click=move |_| start_eco()>
                        {move || t(ClimateText::StartEco)}
                    </button>
                    <button type="button" class="btn" on:click=move |_| eco_open.set(false)>
                        {move || t(ClimateText::Cancel)}
                    </button>
                }
                .into_any(),
                None => view! {
                    <button type="button" class="btn" on:click=move |_| open_eco()>
                        {move || t(ClimateText::EcoUntilButton)}
                    </button>
                }
                .into_any(),
            };
            view! { <div class="climate-eco">{body}</div> }
        })
    };

    // Learning.
    let learn_line = move || zone.with(|z| learn_status_text(lang(), &z.learn));
    let can_learn = move || zone.with(|z| z.settings.state != ZoneState::Off);
    let learn_id = id.clone();

    // Decisions: fetched when the list opens, and again whenever the zone's data changes while
    // it is open. A hidden panel waits, and fetches again when it is shown.
    let open = RwSignal::new(false);
    let log = RwSignal::new(None::<DecisionLog>);
    let decisions_id = id.clone();
    Effect::new(move |_| {
        zone.track();
        let visible = shown.is_none_or(|s| s.get());
        if open.get() && visible {
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
        let lang = lang();
        let now = clock.get();
        log.get().map(|l| {
            let recent = l
                .recent
                .iter()
                .take(RECENT)
                .map(|d| view! { <li>{decision_line(lang, d, now, tz)}</li> })
                .collect_view();
            let older = l
                .older
                .iter()
                .map(
                    |r| view! { <li class="climate-decisions__older">{row_line(lang, r, now, tz)}</li> },
                )
                .collect_view();
            view! { <ul class="climate-decisions__list">{recent}{older}</ul> }
        })
    };

    // The proposal panel is rebuilt only when the proposal itself changes (id or fingerprint),
    // so a refetch keeps an edit in progress.
    let proposal_key = Memo::new(move |_| {
        zone.with(|z| {
            z.proposal
                .as_ref()
                .map(|p| (p.id.clone(), p.fingerprint.clone()))
        })
    });
    let proposal = move || {
        proposal_key.get().and_then(|_| {
            zone.with_untracked(|z| {
                z.proposal.clone().map(|p| {
                    let approved = Signal::derive(move || zone.with(|z| z.settings.rules.clone()));
                    view! { <ProposalPanel zone=z.zone.clone() proposal=p approved=approved on_changed=on_changed now=now.into() /> }
                })
            })
        })
    };

    view! {
        <section class="climate-panel" class:climate-panel--orphan=move || zone.with(|z| z.orphan)>
            <div class="card climate-status">
                <header class="climate-status__header">
                    <h2 class="climate-status__name">{move || zone.with(|z| z.name.clone())}</h2>
                    {move || {
                        zone.with(|z| z.orphan)
                            .then(|| view! { <span class="badge">{move || t(ClimateText::NoClimateDevice)}</span> })
                    }}
                </header>
                {move || {
                    zone.with(|z| z.settings_invalid.clone())
                        .map(|e| view! { <p class="badge climate-status__invalid">{e}</p> })
                }}
                <div class="climate-status__figures">
                    <div class="climate-status__figure">
                        <span class="climate-status__label">{reading_label}</span>
                        <strong class="climate-status__reading">{reading}</strong>
                        {thermometer_line}
                    </div>
                    <div class="climate-status__figure">
                        <span class="climate-status__label">{move || t(ClimateText::Target)}</span>
                        <strong class="climate-status__target">{target}</strong>
                        {move || {
                            source()
                                .map(|(word, reason)| {
                                    view! {
                                        <span class="climate-status__why">
                                            <span class="chip climate-status__source">{word}</span>
                                            <span class="climate-status__reason">{reason}</span>
                                        </span>
                                    }
                                })
                        }}
                    </div>
                </div>
                <div class="segmented climate-status__state" role="group" aria-label=move || t(ClimateText::ZoneStateLabel)>
                    {state_buttons}
                </div>
                {move || {
                    holdable
                        .get()
                        .then(|| {
                            let hold_1h = hold_1h.clone();
                            let hold_2h = hold_2h.clone();
                            let hold_next = hold_next.clone();
                            view! {
                                <div class="climate-hold">
                                    <input
                                        class="field climate-hold__setpoint"
                                        type="text"
                                        inputmode="decimal"
                                        aria-label=move || t(ClimateText::HoldTemperature)
                                        prop:value=move || setpoint.get()
                                        on:input=move |ev| setpoint.set(event_target_value(&ev))
                                    />
                                    <button type="button" class="btn" on:click=move |_| hold_1h(60)>
                                        "1 h"
                                    </button>
                                    <button type="button" class="btn" on:click=move |_| hold_2h(120)>
                                        "2 h"
                                    </button>
                                    {move || {
                                        until_change()
                                            .map(|m| {
                                                let hold_next = hold_next.clone();
                                                view! {
                                                    <button
                                                        type="button"
                                                        class="btn climate-hold__next"
                                                        on:click=move |_| hold_next(m)
                                                    >
                                                        <span class="climate-hold__long">
                                                            {move || t(ClimateText::UntilNextChange)}
                                                        </span>
                                                        <span class="climate-hold__short">
                                                            {move || t(ClimateText::NextChange)}
                                                        </span>
                                                    </button>
                                                }
                                            })
                                    }}
                                </div>
                            }
                        })
                }}
                {overrides}
                {eco_row}
            </div>
            <div class="card climate-week">
                <RulesPanel zone=zone on_changed=on_changed now=now.into() />
                <p class="climate-legend">
                    <span><i class="climate-legend__swatch climate-temp--low"></i>"≤ 16°"</span>
                    <span><i class="climate-legend__swatch climate-temp--mid"></i>"17 – 19°"</span>
                    <span><i class="climate-legend__swatch climate-temp--high"></i>"≥ 20°"</span>
                </p>
            </div>
            {move || proposal().map(|p| view! { <div class="card climate-proposal-card">{p}</div> })}
            <div class="climate-panel__lower">
                <div class="card climate-learn-card">
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
                                            {move || t(ClimateText::LearnNow)}
                                        </button>
                                    }
                                })
                        }}
                    </div>
                    <ThermometerField
                        value=Signal::derive(move || zone.with(|z| z.settings.sensor.clone()))
                        auto=Signal::derive(move || {
                            zone.with(|z| z.settings.sensor.is_none().then(|| z.room_sensor.clone()).flatten())
                        })
                        on_change=set_sensor
                    />
                </div>
                <CorrectionCard zone=zone on_changed=on_changed />
                <div class="card climate-decisions">
                    <button
                        type="button"
                        class="chip"
                        aria-expanded=move || open.get().to_string()
                        on:click=move |_| open.update(|o| *o = !*o)
                    >
                        {move || {
                            t(if open.get() { ClimateText::HideDecisions } else { ClimateText::RecentDecisions })
                        }}
                    </button>
                    <Show when=move || open.get()>{list}</Show>
                </div>
            </div>
        </section>
    }
}
