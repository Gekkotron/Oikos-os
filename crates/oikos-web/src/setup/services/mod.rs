//! The Services step: one collapsible card per optional service. A card shows whether the
//! service is set up, skipped or neither, and opens the same form its Settings page uses.

pub mod basic;

use std::collections::BTreeSet;
use std::future::Future;

use leptos::prelude::*;
use oikos_api::{GeoPlace, SetupStep};
use serde_json::{Map, Value};

use super::welcome::place_label;
use super::wizard::WizardNav;
use super::{Applying, SetupState, setup_href};
use crate::api::setup::{SaveError, get_section, put_section, put_step};
use crate::i18n::setup::{SetupText, setup_text, use_setup_text};
use crate::i18n::use_lang;
use basic::{AlarmForm, ClimateForm, WeatherForm};

/// What a service card says about its service.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CardState {
    NotSetUp,
    SetUp,
    Skipped,
}

/// A service that is set up stays set up, even if the user skipped it earlier.
pub fn card_state(set_up: bool, skipped: bool) -> CardState {
    if set_up {
        CardState::SetUp
    } else if skipped {
        CardState::Skipped
    } else {
        CardState::NotSetUp
    }
}

/// The service cards, in the order the mockup shows them.
pub const SERVICE_IDS: [&str; 7] = [
    "weather", "solar", "energy", "cameras", "push", "climate", "alarm",
];

const SKIPPED_KEY: &str = "oikos.setup.skipped";

/// The ids in a stored skipped list; anything unreadable reads as empty.
fn parse_skipped(raw: Option<&str>) -> BTreeSet<String> {
    raw.and_then(|r| serde_json::from_str(r).ok())
        .unwrap_or_default()
}

#[cfg(target_arch = "wasm32")]
fn storage() -> Option<web_sys::Storage> {
    web_sys::window().and_then(|w| w.local_storage().ok().flatten())
}

/// The cards the user skipped in this browser. Skipped only labels a card, and the hub has no
/// field for it, so it lives in `localStorage` and any storage failure reads as empty.
pub fn skipped() -> BTreeSet<String> {
    #[cfg(target_arch = "wasm32")]
    {
        let raw = storage().and_then(|s| s.get_item(SKIPPED_KEY).ok().flatten());
        parse_skipped(raw.as_deref())
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        parse_skipped(None)
    }
}

/// Marks `id` skipped or not; a storage failure loses the mark and nothing else.
pub fn set_skipped(id: &str, on: bool) {
    #[cfg(target_arch = "wasm32")]
    {
        let mut ids = skipped();
        if on {
            ids.insert(id.to_owned());
        } else {
            ids.remove(id);
        }
        if let (Some(storage), Ok(text)) = (storage(), serde_json::to_string(&ids)) {
            let _ = storage.set_item(SKIPPED_KEY, &text);
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    let _ = (id, on);
}

/// `base` (a section's GET reply) with the named top-level keys overwritten. Every other key
/// travels back unchanged, because a section PUT replaces the whole section.
pub fn with_fields(base: Value, fields: &[(&str, Value)]) -> Value {
    let mut map = match base {
        Value::Object(m) => m,
        _ => Map::new(),
    };
    for (key, value) in fields {
        map.insert((*key).to_owned(), value.clone());
    }
    Value::Object(map)
}

/// Saves a section. Every successful save restarts the hub, so success (and a 503 that says
/// the restart already began) starts the "Applying…" overlay towards `applying_target`. A
/// failure returns the hub's message. Context is read when called, so call it inside the
/// owner of the form.
pub fn save_section(
    section: &str,
    body: Value,
    applying_target: &str,
) -> impl Future<Output = Result<(), String>> + 'static {
    let applying = expect_context::<Applying>();
    let lang = use_lang()();
    let section = section.to_owned();
    let target = applying_target.to_owned();
    async move {
        match put_section(&section, &body).await {
            Ok(()) | Err(SaveError::Restarting) => {
                applying.start(&target);
                Ok(())
            }
            Err(e) => Err(e.message(lang)),
        }
    }
}

/// A section read once on mount: `None` while loading, then its values or the hub's error.
pub fn load_section(section: &'static str) -> RwSignal<Option<Result<Value, String>>> {
    let loaded = RwSignal::new(None);
    leptos::task::spawn_local(async move {
        let result = get_section(section).await.map_err(|e| e.error);
        loaded.try_set(Some(result));
    });
    loaded
}

/// The enabled switch row of a service form.
#[component]
pub fn EnabledRow(
    on: RwSignal<bool>,
    #[prop(optional)] busy: Option<RwSignal<bool>>,
) -> impl IntoView {
    let t = use_setup_text();
    view! {
        <label class="override__flag svc__switch">
            <span class="override__label">{move || t(SetupText::SvcEnabled)}</span>
            <button
                type="button"
                role="switch"
                class="switch"
                class:switch--on=move || on.get()
                aria-checked=move || on.get().to_string()
                prop:disabled=move || busy.is_some_and(|b| b.get())
                on:click=move |_| on.update(|v| *v = !*v)
            >
                <span class="switch__knob"></span>
            </button>
        </label>
    }
}

/// A collapsed row with a state pill. Opening it shows `children` and a Skip link that marks
/// the service skipped (through `on_skip`, which also keeps the step's own list current) and
/// closes the card.
#[component]
pub fn ServiceCard(
    id: &'static str,
    icon: &'static str,
    title: SetupText,
    desc: Signal<String>,
    state: Signal<CardState>,
    children: ChildrenFn,
    #[prop(optional)] on_skip: Option<Callback<()>>,
) -> impl IntoView {
    let t = use_setup_text();
    let open = RwSignal::new(false);
    let skip = move |_| {
        set_skipped(id, true);
        if let Some(cb) = on_skip {
            cb.run(());
        }
        open.set(false);
    };
    let pill_class = move || match state.get() {
        CardState::SetUp => "svc__pill svc__pill--set",
        CardState::Skipped => "svc__pill svc__pill--skipped",
        CardState::NotSetUp => "svc__pill",
    };
    let pill_text = move || {
        t(match state.get() {
            CardState::SetUp => SetupText::CardSetUp,
            CardState::Skipped => SetupText::CardSkipped,
            CardState::NotSetUp => SetupText::CardNotSetUp,
        })
    };
    view! {
        <li class="svc" id=format!("svc-{id}")>
            <button
                type="button"
                class="svc__head"
                aria-expanded=move || open.get().to_string()
                on:click=move |_| open.update(|o| *o = !*o)
            >
                <span class="svc__icon" inner_html=icon></span>
                <span class="svc__text">
                    <span class="svc__title">{move || t(title)}</span>
                    <span class="svc__desc">{move || desc.get()}</span>
                </span>
                <span class=pill_class>{pill_text}</span>
            </button>
            <Show when=move || open.get()>
                <div class="svc__body">
                    {children()}
                    <button type="button" class="chip svc__skip" on:click=skip>
                        {move || t(SetupText::Skip)}
                    </button>
                </div>
            </Show>
        </li>
    }
}

/// A card body whose form is not built yet.
#[component]
fn Later() -> impl IntoView {
    let t = use_setup_text();
    view! { <p class="settings__hint">{move || t(SetupText::ComingSoon)}</p> }
}

/// Whether a settings section reads `enabled: true`; a failed read counts as off.
async fn section_enabled(section: &'static str) -> bool {
    get_section(section)
        .await
        .ok()
        .and_then(|v| v.get("enabled").and_then(Value::as_bool))
        .unwrap_or(false)
}

/// The Services step. Needs [`Applying`] and [`WizardNav`] in context.
#[component]
pub fn ServicesStep() -> impl IntoView {
    let nav = expect_context::<WizardNav>();
    let t = use_setup_text();
    let lang = use_lang();
    let state = use_context::<SetupState>();
    let skipped_ids = RwSignal::new(skipped());
    let weather_on = RwSignal::new(false);
    let climate_on = RwSignal::new(false);
    let alarm_on = RwSignal::new(false);
    leptos::task::spawn_local(async move {
        weather_on.try_set(section_enabled("weather").await);
        climate_on.try_set(section_enabled("climate").await);
        let alarm = crate::api::alarm::get_alarm()
            .await
            .is_ok_and(|a| a.enabled);
        alarm_on.try_set(alarm);
    });
    let failure = RwSignal::new(None::<String>);
    let advancing = RwSignal::new(false);
    let go_next = move |_| {
        if advancing.get_untracked() {
            return;
        }
        advancing.set(true);
        leptos::task::spawn_local(async move {
            match put_step(SetupStep::Done).await {
                Ok(_) => {
                    nav.step.try_set(SetupStep::Done);
                }
                Err(e) => {
                    failure.try_set(Some(e.message(lang())));
                }
            }
            advancing.try_set(false);
        });
    };
    let card_state_of = move |id: &'static str, on: Option<RwSignal<bool>>| {
        Signal::derive(move || {
            card_state(
                on.is_some_and(|o| o.get()),
                skipped_ids.with(|s| s.contains(id)),
            )
        })
    };
    let on_skip = move |id: &'static str| {
        Callback::new(move |()| {
            skipped_ids.update(|s| {
                s.insert(id.to_owned());
            });
        })
    };
    let fixed = move |k: SetupText| Signal::derive(move || setup_text(lang(), k).to_owned());
    let weather_desc = Signal::derive(move || {
        let place = state.and_then(|s| {
            s.0.with(|st| {
                st.as_ref().and_then(|st| st.location.as_ref()).map(|l| {
                    place_label(&GeoPlace {
                        name: l.city.clone(),
                        admin: None,
                        country: l.country.clone(),
                        latitude: l.latitude,
                        longitude: l.longitude,
                        timezone: Some(l.timezone.clone()),
                    })
                })
            })
        });
        match place {
            Some(p) => setup_text(lang(), SetupText::DescWeatherPlace).replace("{place}", &p),
            None => setup_text(lang(), SetupText::DescWeatherNoPlace).to_owned(),
        }
    });
    view! {
        <div class="setup__body">
            <h1 class="setup__title">{move || t(SetupText::ServicesTitle)}</h1>
            <p class="setup__lead">{move || t(SetupText::ServicesLead)}</p>
            <ul class="svc__list">
                <ServiceCard
                    id="weather"
                    icon=crate::icons::PARTLY_CLOUDY_DAY
                    title=SetupText::SvcWeather
                    desc=weather_desc
                    state=card_state_of("weather", Some(weather_on))
                    on_skip=on_skip("weather")
                >
                    <WeatherForm applying_target=setup_href(SetupStep::Services) />
                </ServiceCard>
                <ServiceCard
                    id="solar"
                    icon=crate::icons::SUN
                    title=SetupText::SvcSolar
                    desc=fixed(SetupText::DescSolar)
                    state=card_state_of("solar", None)
                    on_skip=on_skip("solar")
                >
                    <Later />
                </ServiceCard>
                <ServiceCard
                    id="energy"
                    icon=crate::icons::BOLT
                    title=SetupText::SvcEnergy
                    desc=fixed(SetupText::DescEnergy)
                    state=card_state_of("energy", None)
                    on_skip=on_skip("energy")
                >
                    <Later />
                </ServiceCard>
                <ServiceCard
                    id="cameras"
                    icon=crate::icons::CAMERA
                    title=SetupText::SvcCameras
                    desc=fixed(SetupText::DescCameras)
                    state=card_state_of("cameras", None)
                    on_skip=on_skip("cameras")
                >
                    <Later />
                </ServiceCard>
                <ServiceCard
                    id="push"
                    icon=crate::icons::BELL
                    title=SetupText::SvcPush
                    desc=fixed(SetupText::DescPush)
                    state=card_state_of("push", None)
                    on_skip=on_skip("push")
                >
                    <Later />
                </ServiceCard>
                <ServiceCard
                    id="climate"
                    icon=crate::icons::THERMOMETER
                    title=SetupText::SvcClimate
                    desc=fixed(SetupText::DescClimate)
                    state=card_state_of("climate", Some(climate_on))
                    on_skip=on_skip("climate")
                >
                    <ClimateForm applying_target=setup_href(SetupStep::Services) />
                </ServiceCard>
                <ServiceCard
                    id="alarm"
                    icon=crate::icons::SHIELD
                    title=SetupText::SvcAlarm
                    desc=fixed(SetupText::DescAlarm)
                    state=card_state_of("alarm", Some(alarm_on))
                    on_skip=on_skip("alarm")
                >
                    <AlarmForm set_up=alarm_on />
                </ServiceCard>
            </ul>
            <p class="loc__problem" role="alert">{move || failure.get()}</p>
            <div class="loc__actions devices__actions">
                <button
                    type="button"
                    class="btn"
                    prop:disabled=move || advancing.get()
                    on:click=move |_| nav.step.set(SetupStep::Devices)
                >
                    {move || t(SetupText::Back)}
                </button>
                <button
                    type="button"
                    class="btn btn--primary loc__save"
                    prop:disabled=move || advancing.get()
                    on:click=go_next
                >
                    {move || t(SetupText::Next)}
                </button>
            </div>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn set_up_wins_over_skipped() {
        assert_eq!(card_state(true, true), CardState::SetUp);
        assert_eq!(card_state(false, true), CardState::Skipped);
        assert_eq!(card_state(false, false), CardState::NotSetUp);
    }

    #[test]
    fn with_fields_keeps_the_other_keys() {
        assert_eq!(
            with_fields(
                json!({"enabled": false, "prefix": "oikos/climate"}),
                &[("enabled", json!(true))]
            ),
            json!({"enabled": true, "prefix": "oikos/climate"})
        );
    }

    #[test]
    fn unreadable_skipped_lists_read_as_empty() {
        assert!(parse_skipped(None).is_empty());
        assert!(parse_skipped(Some("not json")).is_empty());
        assert_eq!(
            parse_skipped(Some(r#"["weather","solar"]"#)),
            BTreeSet::from(["weather".to_owned(), "solar".to_owned()])
        );
    }

    #[test]
    fn service_ids_follow_the_mockup() {
        assert_eq!(SERVICE_IDS[0], "weather");
        assert_eq!(SERVICE_IDS.len(), 7);
    }
}
