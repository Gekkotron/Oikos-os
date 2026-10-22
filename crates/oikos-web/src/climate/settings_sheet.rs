//! The house-wide climate settings: season, outdoor sensor, thresholds, presence and learning.

use leptos::prelude::*;
use leptos::task::spawn_local;
use oikos_climate::doc::{HhMm, HouseSettings, Season};

use crate::api::climate as api;
use crate::climate::entity_field::{EntityField, EntityListField};
use crate::components::toast::Toasts;

fn season_value(s: Season) -> &'static str {
    match s {
        Season::Auto => "auto",
        Season::Heat => "heat",
        Season::Cool => "cool",
        Season::Off => "off",
    }
}

fn season_from(s: &str) -> Season {
    match s {
        "heat" => Season::Heat,
        "cool" => Season::Cool,
        "off" => Season::Off,
        _ => Season::Auto,
    }
}

/// `HH:MM` for a time input.
fn hhmm_text(t: HhMm) -> String {
    format!("{:02}:{:02}", t.0 / 60, t.0 % 60)
}

/// A number input; an unparsable edit is ignored so the field keeps its last good value.
#[component]
fn NumField(
    label: &'static str,
    unit: &'static str,
    value: Signal<f64>,
    on_change: Callback<f64>,
) -> impl IntoView {
    view! {
        <label class="climate-field">
            <span class="climate-field__label">{label} " (" {unit} ")"</span>
            <input
                class="field"
                type="number"
                step="any"
                prop:value=move || value.get().to_string()
                on:change=move |ev| {
                    if let Ok(v) = event_target_value(&ev).trim().parse::<f64>() {
                        on_change.run(v);
                    }
                }
            />
        </label>
    }
}

/// Edits a copy of `house`; Save validates it, sends it and calls `on_saved`.
#[component]
pub fn SettingsSheet(
    house: HouseSettings,
    on_saved: Callback<()>,
    on_close: Callback<()>,
) -> impl IntoView {
    let toasts = expect_context::<Toasts>();
    let draft = RwSignal::new(house);
    let saving = RwSignal::new(false);

    let num = move |label: &'static str,
                    unit: &'static str,
                    get: fn(&HouseSettings) -> f64,
                    set: fn(&mut HouseSettings, f64)| {
        view! {
            <NumField
                label=label
                unit=unit
                value=Signal::derive(move || draft.with(get))
                on_change=Callback::new(move |v| draft.update(|d| set(d, v)))
            />
        }
    };

    let save = move |_| {
        let settings = draft.get_untracked();
        if let Err(e) = settings.validate() {
            toasts.push(e);
            return;
        }
        saving.set(true);
        spawn_local(async move {
            match api::put_house(&settings).await {
                Ok(()) => on_saved.run(()),
                Err(e) => toasts.push(format!("Could not save: {}", e.error)),
            }
            saving.try_set(false);
        });
    };

    view! {
        <section class="climate-settings card" aria-label="Climate settings">
            <h2 class="climate-settings__title">"Settings"</h2>
            <label class="climate-field">
                <span class="climate-field__label">"Season"</span>
                <select
                    class="field"
                    on:change=move |ev| {
                        let s = season_from(&event_target_value(&ev));
                        draft.update(|d| d.season = s);
                    }
                >
                    {[Season::Auto, Season::Heat, Season::Cool, Season::Off]
                        .into_iter()
                        .map(|s| {
                            let v = season_value(s);
                            view! {
                                <option value=v selected=move || draft.with(|d| d.season == s)>
                                    {crate::format::word(v)}
                                </option>
                            }
                        })
                        .collect_view()}
                </select>
            </label>
            <EntityField
                label="Outdoor temperature"
                value=Signal::derive(move || draft.with(|d| d.outdoor_entity.clone()))
                on_change=Callback::new(move |v| draft.update(|d| d.outdoor_entity = v))
            />
            {num("Heat below", "°C", |d| d.heat_below, |d, v| d.heat_below = v)}
            {num("Cool above", "°C", |d| d.cool_above, |d, v| d.cool_above = v)}
            <EntityListField
                label="Presence"
                value=Signal::derive(move || draft.with(|d| d.presence.clone()))
                on_change=Callback::new(move |v| draft.update(|d| d.presence = v))
            />
            {num("Away grace", "min", |d| f64::from(d.away_grace_min), |d, v| {
                d.away_grace_min = v.max(0.0) as u32
            })}
            <EntityField
                label="Holiday"
                value=Signal::derive(move || draft.with(|d| d.holiday_entity.clone()))
                on_change=Callback::new(move |v| draft.update(|d| d.holiday_entity = v))
            />
            {num("Frost", "°C", |d| d.frost, |d, v| d.frost = v)}
            {num("Overheat", "°C", |d| d.overheat, |d, v| d.overheat = v)}
            {num("Overcool", "°C", |d| d.overcool, |d, v| d.overcool = v)}
            {num("Stale after", "min", |d| f64::from(d.stale_after_min), |d, v| {
                d.stale_after_min = v.max(0.0) as u32
            })}
            {num("Learning window", "days", |d| f64::from(d.learn_days), |d, v| {
                d.learn_days = v.max(0.0) as u32
            })}
            {num("Minimum days", "days", |d| f64::from(d.learn_min_days), |d, v| {
                d.learn_min_days = v.max(0.0) as u32
            })}
            {num("Agreement needed", "0 to 1", |d| d.learn_support, |d, v| d.learn_support = v)}
            <label class="climate-field">
                <span class="climate-field__label">"Learn at"</span>
                <input
                    class="field"
                    type="time"
                    prop:value=move || draft.with(|d| hhmm_text(d.learn_at))
                    on:change=move |ev| {
                        if let Some(t) = HhMm::parse(&event_target_value(&ev)) {
                            draft.update(|d| d.learn_at = t);
                        }
                    }
                />
            </label>
            <div class="segmented">
                <button type="button" class="btn" on:click=move |_| on_close.run(())>
                    "Cancel"
                </button>
                <button
                    type="button"
                    class="btn btn--active"
                    prop:disabled=move || saving.get()
                    on:click=save
                >
                    "Save"
                </button>
            </div>
        </section>
    }
}
