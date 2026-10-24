//! The house-wide climate settings: season, outdoor sensor, thresholds, presence and learning.

use leptos::prelude::*;
use leptos::task::spawn_local;
use oikos_climate::doc::{HhMm, HouseSettings, Season};

use crate::api::climate as api;
use crate::climate::entity_field::{EntityField, EntityListField};
use crate::components::toast::Toasts;
use crate::i18n::climate::{ClimateText, could_not_save, use_climate_text};
use crate::i18n::format::{number, parse_decimal};
use crate::i18n::states::{StateCtx, state_word};
use crate::i18n::{Key, use_lang, use_text};

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

/// What a settings number is measured in.
#[derive(Clone, Copy)]
enum Unit {
    Celsius,
    Word(ClimateText),
}

/// A number input ("21.5" or "21,5"); an unparsable edit is ignored so the field keeps its last
/// good value.
#[component]
fn NumField(
    label: ClimateText,
    unit: Unit,
    value: Signal<f64>,
    on_change: Callback<f64>,
) -> impl IntoView {
    let lang = use_lang();
    let t = use_climate_text();
    let unit = move || match unit {
        Unit::Celsius => "°C",
        Unit::Word(word) => t(word),
    };
    view! {
        <label class="climate-field">
            <span class="climate-field__label">{move || t(label)} " (" {unit} ")"</span>
            <input
                class="field"
                type="text"
                inputmode="decimal"
                prop:value=move || number(lang(), value.get())
                on:change=move |ev| {
                    if let Some(v) = parse_decimal(&event_target_value(&ev)) {
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
    let lang = use_lang();
    let t = use_climate_text();
    let text = use_text();
    let draft = RwSignal::new(house);
    let saving = RwSignal::new(false);

    let num = move |label: ClimateText,
                    unit: Unit,
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
                Err(e) => toasts.push(could_not_save(untrack(lang), &e.error)),
            }
            saving.try_set(false);
        });
    };

    view! {
        <section class="climate-settings card" aria-label=move || t(ClimateText::SettingsLabel)>
            <h2 class="climate-settings__title">{move || text(Key::Settings)}</h2>
            <label class="climate-field">
                <span class="climate-field__label">{move || t(ClimateText::Season)}</span>
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
                                    {move || state_word(lang(), StateCtx::Hvac, v)}
                                </option>
                            }
                        })
                        .collect_view()}
                </select>
            </label>
            <EntityField
                label=Signal::derive(move || t(ClimateText::OutdoorTemperature).to_owned())
                value=Signal::derive(move || draft.with(|d| d.outdoor_entity.clone()))
                on_change=Callback::new(move |v| draft.update(|d| d.outdoor_entity = v))
            />
            {num(ClimateText::HeatBelow, Unit::Celsius, |d| d.heat_below, |d, v| d.heat_below = v)}
            {num(ClimateText::CoolAbove, Unit::Celsius, |d| d.cool_above, |d, v| d.cool_above = v)}
            <EntityListField
                label=Signal::derive(move || t(ClimateText::Presence).to_owned())
                value=Signal::derive(move || draft.with(|d| d.presence.clone()))
                on_change=Callback::new(move |v| draft.update(|d| d.presence = v))
            />
            {num(ClimateText::AwayGrace, Unit::Word(ClimateText::UnitMin), |d| f64::from(d.away_grace_min), |d, v| {
                d.away_grace_min = v.max(0.0) as u32
            })}
            <EntityField
                label=Signal::derive(move || t(ClimateText::Holiday).to_owned())
                value=Signal::derive(move || draft.with(|d| d.holiday_entity.clone()))
                on_change=Callback::new(move |v| draft.update(|d| d.holiday_entity = v))
            />
            {num(ClimateText::Frost, Unit::Celsius, |d| d.frost, |d, v| d.frost = v)}
            {num(ClimateText::Overheat, Unit::Celsius, |d| d.overheat, |d, v| d.overheat = v)}
            {num(ClimateText::Overcool, Unit::Celsius, |d| d.overcool, |d, v| d.overcool = v)}
            {num(ClimateText::StaleAfter, Unit::Word(ClimateText::UnitMin), |d| f64::from(d.stale_after_min), |d, v| {
                d.stale_after_min = v.max(0.0) as u32
            })}
            {num(ClimateText::LearningWindow, Unit::Word(ClimateText::UnitDays), |d| f64::from(d.learn_days), |d, v| {
                d.learn_days = v.max(0.0) as u32
            })}
            {num(ClimateText::MinimumDays, Unit::Word(ClimateText::UnitDays), |d| f64::from(d.learn_min_days), |d, v| {
                d.learn_min_days = v.max(0.0) as u32
            })}
            {num(ClimateText::AgreementNeeded, Unit::Word(ClimateText::UnitZeroToOne), |d| d.learn_support, |d, v| d.learn_support = v)}
            <label class="climate-field">
                <span class="climate-field__label">{move || t(ClimateText::LearnAt)}</span>
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
                    {move || t(ClimateText::Cancel)}
                </button>
                <button
                    type="button"
                    class="btn btn--active"
                    prop:disabled=move || saving.get()
                    on:click=save
                >
                    {move || t(ClimateText::Save)}
                </button>
            </div>
        </section>
    }
}
