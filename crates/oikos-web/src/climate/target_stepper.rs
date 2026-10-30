//! A room's target with − and +: each press moves it half a degree, and once the presses stop
//! one hold is sent, until the next schedule change (two hours when there is none). Chips then
//! offer the same target for 1 h, 2 h or until the next change, for a few seconds.
//!
//! From the first press until the first refetch after its hold is sent, the shown target is the
//! pressed one, so a refetch in between neither moves it back nor loses a press.

use gloo_timers::callback::Timeout;
use leptos::prelude::*;
use leptos::task::spawn_local;
use oikos_climate::doc::{Day, Mode, Setpoint};
use oikos_climate::view::ZoneView;

use crate::api::climate::{self as api, OverrideBody};
use crate::climate::{
    TARGET_STEP, can_hold, degrees_text, hold_minutes, minutes_to_next_change, start_target,
    step_target,
};
use crate::components::toast::Toasts;
use crate::i18n::climate::{ClimateText, use_climate_text};
use crate::i18n::use_lang;

/// How long after the last press the hold is sent.
const SEND_AFTER_MS: u32 = 1_500;
/// How long the duration chips stay after a hold is sent.
const CHIPS_FOR_MS: u32 = 10_000;

#[component]
pub fn TargetStepper(
    zone: Signal<ZoneView>,
    /// (day, minute) in the house time zone, as `RoomPanel`'s `now` memo.
    now: Signal<(Day, u16)>,
    on_changed: Callback<()>,
) -> impl IntoView {
    let lang = use_lang();
    let t = use_climate_text();
    let toasts = expect_context::<Toasts>();

    // The pressed target, from the first press until the refetch after its send.
    let draft = RwSignal::new(None::<f64>);
    // A hold was sent: the next refetch shows the hub's target again.
    let awaiting = RwSignal::new(false);
    let last_sent = RwSignal::new(None::<f64>);
    let chips_open = RwSignal::new(false);
    let send_timer = StoredValue::new_local(None::<Timeout>);
    let chips_timer = StoredValue::new_local(None::<Timeout>);
    on_cleanup(move || {
        send_timer.try_set_value(None);
        chips_timer.try_set_value(None);
    });

    let shown = Memo::new(move |_| draft.get().or_else(|| zone.with(start_target)));
    let holdable = Memo::new(move |_| zone.with(|z| can_hold(z.settings.state)));
    let next_change = move || {
        zone.with(|z| {
            let mode = z
                .decision
                .as_ref()
                .and_then(|d| d.mode)
                .unwrap_or(Mode::Heat);
            let (day, minute) = now.get();
            minutes_to_next_change(&z.settings.rules, mode, day, minute)
        })
    };

    Effect::new(move |_| {
        zone.track();
        if awaiting.get_untracked() {
            awaiting.set(false);
            draft.set(None);
        }
    });

    let close_chips = move || {
        chips_open.try_set(false);
        chips_timer.try_set_value(None);
    };
    // `offer_chips`: a hold from the presses offers the chips; one from a chip does not.
    let send = move |v: f64, minutes: u32, offer_chips: bool| {
        let id = zone.with_untracked(|z| z.zone.clone());
        let body = OverrideBody {
            setpoint: Some(Setpoint::Temp(v)),
            minutes: Some(minutes),
            ..OverrideBody::default()
        };
        spawn_local(async move {
            match api::set_override(&id, body).await {
                Ok(()) => {
                    last_sent.try_set(Some(v));
                    if offer_chips {
                        chips_open.try_set(true);
                        chips_timer.try_set_value(Some(Timeout::new(CHIPS_FOR_MS, close_chips)));
                    }
                    awaiting.try_set(true);
                    on_changed.run(());
                }
                Err(e) => {
                    toasts.push(e.error);
                    draft.try_set(None);
                }
            }
        });
    };
    let press = move |delta: f64| {
        let Some(from) = shown.get_untracked() else {
            return;
        };
        let v = step_target(from, delta);
        draft.set(Some(v));
        // A new press starts a new change: a refetch now must not drop it.
        awaiting.set(false);
        close_chips();
        let timer = Timeout::new(SEND_AFTER_MS, move || {
            send(v, hold_minutes(untrack(next_change)), true);
        });
        send_timer.set_value(Some(timer));
    };
    let chip = move |minutes: u32| {
        close_chips();
        if let Some(v) = last_sent.get_untracked() {
            send(v, minutes, false);
        }
    };

    let disabled = move || !holdable.get() || shown.with(Option::is_none);
    let value = move || {
        shown
            .get()
            .map_or_else(|| "—".to_owned(), |v| degrees_text(lang(), v))
    };
    let chips = move || {
        chips_open.get().then(|| {
            let until_next = next_change().map(|m| {
                view! {
                    <button type="button" class="chip" on:click=move |_| chip(m)>
                        {move || t(ClimateText::UntilNextChange)}
                    </button>
                }
            });
            view! {
                <div class="climate-stepper__chips">
                    <button type="button" class="chip" on:click=move |_| chip(60)>
                        "1 h"
                    </button>
                    <button type="button" class="chip" on:click=move |_| chip(120)>
                        "2 h"
                    </button>
                    {until_next}
                </div>
            }
        })
    };
    view! {
        <div class="climate-stepper">
            <button
                type="button"
                class="climate-stepper__step"
                aria-label=move || t(ClimateText::LowerTarget)
                disabled=disabled
                on:click=move |_| press(-TARGET_STEP)
            >
                "−"
            </button>
            <strong class="climate-stepper__value">{value}</strong>
            <button
                type="button"
                class="climate-stepper__step"
                aria-label=move || t(ClimateText::RaiseTarget)
                disabled=disabled
                on:click=move |_| press(TARGET_STEP)
            >
                "+"
            </button>
        </div>
        {chips}
    }
}
