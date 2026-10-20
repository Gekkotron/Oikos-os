//! The schedule node's editor: five friendly choices as chips, the inputs of the chosen one,
//! a plain-English summary and the next three ticks as the hub reads the expression.

use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api;
use crate::schedule::{SchedulePick, WEEK_CHIPS, describe, from_cron, to_cron};

/// Set while a schedule editor holds an expression the hub rejects; the Save button reads it.
#[derive(Clone, Copy)]
pub struct SaveBlocked(pub RwSignal<bool>);

const CHOICES: [&str; 5] = ["Every N minutes", "Hourly", "Daily", "Weekdays", "Advanced"];
const DEBOUNCE_MS: u32 = 300;

fn choice_index(pick: &SchedulePick) -> usize {
    match pick {
        SchedulePick::EveryMinutes(_) => 0,
        SchedulePick::Hourly { .. } => 1,
        SchedulePick::Daily { .. } => 2,
        SchedulePick::Weekdays { .. } => 3,
        SchedulePick::Advanced(_) => 4,
    }
}

fn hour_minute(pick: &SchedulePick) -> (u8, u8) {
    match pick {
        SchedulePick::Daily { hour, minute } | SchedulePick::Weekdays { hour, minute, .. } => {
            (*hour, *minute)
        }
        SchedulePick::Hourly { minute } => (7, *minute),
        _ => (7, 0),
    }
}

/// The pick of choice `index`, carrying over the time of the current one where it fits.
fn switch_to(index: usize, current: &SchedulePick) -> SchedulePick {
    let (hour, minute) = hour_minute(current);
    match index {
        0 => SchedulePick::EveryMinutes(10),
        1 => SchedulePick::Hourly { minute },
        2 => SchedulePick::Daily { hour, minute },
        3 => SchedulePick::Weekdays {
            days: match current {
                SchedulePick::Weekdays { days, .. } => days.clone(),
                _ => vec![1, 2, 3, 4, 5],
            },
            hour,
            minute,
        },
        _ => SchedulePick::Advanced(to_cron(current)),
    }
}

fn clamped(text: &str, min: u8, max: u8) -> Option<u8> {
    text.trim().parse::<u8>().ok().map(|n| n.clamp(min, max))
}

/// `HH:MM` of a time input from its two numbers.
fn time_text(hour: u8, minute: u8) -> String {
    format!("{hour:02}:{minute:02}")
}

/// A tick as `Mon 21 Oct, 07:30` read off the RFC 3339 text the hub sends (already in the
/// house zone, so no conversion): the date and time are cut from the string.
fn tick_text(rfc3339: &str) -> String {
    match (rfc3339.get(..10), rfc3339.get(11..16)) {
        (Some(date), Some(time)) => format!("{date} {time}"),
        _ => rfc3339.to_owned(),
    }
}

/// The editor of a schedule starting from the expression `initial`; each change hands the new
/// expression to `write`.
#[component]
pub fn ScheduleField(initial: String, write: Callback<String>) -> impl IntoView {
    let blocked = use_context::<SaveBlocked>();
    let pick = RwSignal::new(from_cron(&initial));
    let ticks = RwSignal::new(Vec::<String>::new());
    let zone = RwSignal::new(String::new());
    let error = RwSignal::new(None::<String>);
    let generation = RwSignal::new(0_u32);

    // Each change writes the cron and asks the hub for the next ticks (after a short pause; a
    // reply for a stale expression is dropped).
    Effect::new(move |_| {
        let current = pick.get();
        let cron = to_cron(&current);
        let empty_days = matches!(&current, SchedulePick::Weekdays { days, .. } if days.is_empty());
        if empty_days {
            error.set(Some("Pick at least one day".into()));
            ticks.set(Vec::new());
            if let Some(b) = blocked {
                b.0.set(true);
            }
            return;
        }
        write.run(cron.clone());
        let ticket = generation.get_untracked() + 1;
        generation.set(ticket);
        spawn_local(async move {
            gloo_timers::future::TimeoutFuture::new(DEBOUNCE_MS).await;
            if generation.get_untracked() != ticket {
                return;
            }
            let result = api::schedule_preview(&cron).await;
            if generation.get_untracked() != ticket {
                return;
            }
            match result {
                Ok(preview) => {
                    error.set(None);
                    zone.set(preview.timezone);
                    ticks.set(preview.next);
                    if let Some(b) = blocked {
                        b.0.set(false);
                    }
                }
                Err(e) => {
                    error.set(Some(e.error));
                    ticks.set(Vec::new());
                    if let Some(b) = blocked {
                        b.0.set(true);
                    }
                }
            }
        });
    });

    let chips = CHOICES
        .iter()
        .enumerate()
        .map(|(i, label)| {
            view! {
                <button
                    type="button"
                    class="chip"
                    class:chip--selected=move || pick.with(|p| choice_index(p) == i)
                    aria-pressed=move || pick.with(|p| choice_index(p) == i).to_string()
                    on:click=move |_| {
                        if pick.with_untracked(|p| choice_index(p) != i) {
                            pick.update(|p| *p = switch_to(i, p));
                        }
                    }
                >
                    {*label}
                </button>
            }
        })
        .collect_view();

    let minutes_input = move || {
        let SchedulePick::EveryMinutes(n) = pick.get_untracked() else {
            return ().into_any();
        };
        view! {
            <label class="node-editor__field">
                <span class="node-editor__label">"Every (minutes, 1–59)"</span>
                <input
                    type="number"
                    class="field"
                    min="1"
                    max="59"
                    inputmode="numeric"
                    prop:value=n.to_string()
                    on:input=move |ev| {
                        if let Some(n) = clamped(&event_target_value(&ev), 1, 59) {
                            pick.set(SchedulePick::EveryMinutes(n));
                        }
                    }
                />
            </label>
        }
        .into_any()
    };

    let minute_input = move || {
        let SchedulePick::Hourly { minute } = pick.get_untracked() else {
            return ().into_any();
        };
        view! {
            <label class="node-editor__field">
                <span class="node-editor__label">"At minute (:MM, 0–59)"</span>
                <input
                    type="number"
                    class="field"
                    min="0"
                    max="59"
                    inputmode="numeric"
                    prop:value=minute.to_string()
                    on:input=move |ev| {
                        if let Some(minute) = clamped(&event_target_value(&ev), 0, 59) {
                            pick.set(SchedulePick::Hourly { minute });
                        }
                    }
                />
            </label>
        }
        .into_any()
    };

    let time_input = move || {
        let current = pick.get_untracked();
        if !matches!(
            current,
            SchedulePick::Daily { .. } | SchedulePick::Weekdays { .. }
        ) {
            return ().into_any();
        }
        let (hour, minute) = hour_minute(&current);
        view! {
            <label class="node-editor__field">
                <span class="node-editor__label">"At (HH:MM)"</span>
                <input
                    type="time"
                    class="field"
                    prop:value=time_text(hour, minute)
                    on:input=move |ev| {
                        let text = event_target_value(&ev);
                        let Some((h, m)) = text.split_once(':') else { return };
                        let (Some(hour), Some(minute)) = (clamped(h, 0, 23), clamped(m, 0, 59))
                        else {
                            return;
                        };
                        pick.update(|p| match p {
                            SchedulePick::Daily { hour: ph, minute: pm }
                            | SchedulePick::Weekdays { hour: ph, minute: pm, .. } => {
                                *ph = hour;
                                *pm = minute;
                            }
                            _ => {}
                        });
                    }
                />
            </label>
        }
        .into_any()
    };

    let days_input = move || {
        if !matches!(pick.get_untracked(), SchedulePick::Weekdays { .. }) {
            return ().into_any();
        }
        let day_chips = WEEK_CHIPS
            .iter()
            .map(|(day, name)| {
                let day = *day;
                let on = move || {
                    pick.with(
                        |p| matches!(p, SchedulePick::Weekdays { days, .. } if days.contains(&day)),
                    )
                };
                view! {
                    <button
                        type="button"
                        class="chip"
                        class:chip--selected=on
                        aria-pressed=move || on().to_string()
                        on:click=move |_| {
                            pick.update(|p| {
                                if let SchedulePick::Weekdays { days, .. } = p {
                                    if let Some(at) = days.iter().position(|d| *d == day) {
                                        days.remove(at);
                                    } else {
                                        days.push(day);
                                        days.sort_unstable();
                                    }
                                }
                            });
                        }
                    >
                        {*name}
                    </button>
                }
            })
            .collect_view();
        view! {
            <div class="node-editor__field">
                <span class="node-editor__label">"On"</span>
                <div class="sched-chips">{day_chips}</div>
            </div>
        }
        .into_any()
    };

    let advanced_input = move || {
        let SchedulePick::Advanced(text) = pick.get_untracked() else {
            return ().into_any();
        };
        view! {
            <label class="node-editor__field">
                <span class="node-editor__label">"Cron (minute hour day month weekday)"</span>
                <input
                    type="text"
                    class="field"
                    spellcheck="false"
                    autocomplete="off"
                    placeholder="*/5 * * * *"
                    aria-invalid=move || error.with(Option::is_some).to_string()
                    prop:value=text
                    on:input=move |ev| {
                        pick.set(SchedulePick::Advanced(event_target_value(&ev).trim().to_owned()));
                    }
                />
            </label>
        }
        .into_any()
    };

    // The inputs are rebuilt only when the choice changes, so typing keeps focus.
    let choice = Memo::new(move |_| pick.with(choice_index));
    view! {
        <div class="sched">
            <div class="sched-chips" role="group" aria-label="Schedule">{chips}</div>
            {move || match choice.get() {
                0 => minutes_input(),
                1 => minute_input(),
                2 => time_input(),
                3 => view! { {time_input()} {days_input()} }.into_any(),
                _ => advanced_input(),
            }}
            <p class="sched-summary">{move || describe(&to_cron(&pick.get()))}</p>
            {move || {
                error
                    .get()
                    .map(|message| view! { <p class="node-editor__error" role="alert">{message}</p> })
            }}
            <Show when=move || error.with(Option::is_none) && ticks.with(|t| !t.is_empty())>
                <div class="sched-next">
                    <span class="node-editor__label">
                        {move || format!("Next runs ({})", zone.get())}
                    </span>
                    <ul>
                        {move || {
                            ticks
                                .get()
                                .iter()
                                .map(|t| view! { <li>{tick_text(t)}</li> })
                                .collect_view()
                        }}
                    </ul>
                </div>
            </Show>
        </div>
    }
}
