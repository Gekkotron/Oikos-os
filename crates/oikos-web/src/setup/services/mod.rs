//! The Services step: one collapsible card per optional service. A card shows whether the
//! service is set up, skipped or neither, and opens the same form its Settings page uses.

pub mod basic;
pub mod energy;
pub mod keys;

use std::collections::BTreeSet;
use std::future::Future;

use gloo_timers::future::TimeoutFuture;
use leptos::prelude::*;
use oikos_api::{GeoPlace, SetupStep};
use serde_json::{Map, Value};

use super::welcome::place_label;
use super::wizard::WizardNav;
use super::{Applying, SetupState, setup_href};
use crate::api::setup::{SaveError, get_section, put_section, put_step};
use crate::house::House;
use crate::i18n::setup::{SetupText, setup_text, use_setup_text};
use crate::i18n::{Lang, use_lang};
use basic::{AlarmForm, ClimateForm, WeatherForm};
use energy::{EnergyForm, holder};
use keys::{FrigateForm, PushForm, SolcastForm};
use oikos_core::energy::EnergyRole;

/// What a service card says about its service.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CardState {
    NotSetUp,
    SetUp,
    Skipped,
    /// The hub could not be read, or did not answer in time.
    Unknown,
}

/// A service that is set up stays set up, even if the user skipped it earlier. `set_up` is
/// `None` when the hub could not be read; that never reads as "Not set up".
pub fn card_state(set_up: Option<bool>, skipped: bool) -> CardState {
    match set_up {
        Some(true) => CardState::SetUp,
        _ if skipped => CardState::Skipped,
        Some(false) => CardState::NotSetUp,
        None => CardState::Unknown,
    }
}

/// How long a settings read may take before its service reads as unknown.
const READ_TIMEOUT_MS: u32 = 5_000;

/// `fut`'s answer, or `None` when it fails to answer within [`READ_TIMEOUT_MS`].
async fn within<T>(fut: impl Future<Output = Option<T>>) -> Option<T> {
    use futures_util::future::{Either, select};
    match select(Box::pin(fut), TimeoutFuture::new(READ_TIMEOUT_MS)).await {
        Either::Left((answer, _)) => answer,
        Either::Right(_) => None,
    }
}

/// The service cards, in the order the mockup shows them.
pub const SERVICE_IDS: [&str; 7] = [
    "weather", "solar", "energy", "cameras", "push", "climate", "alarm",
];

#[cfg(target_arch = "wasm32")]
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
        CardState::Unknown => "svc__pill svc__pill--unknown",
    };
    let pill_text = move || {
        t(match state.get() {
            CardState::SetUp => SetupText::CardSetUp,
            CardState::Skipped => SetupText::CardSkipped,
            CardState::NotSetUp => SetupText::CardNotSetUp,
            CardState::Unknown => SetupText::CardUnknown,
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

/// `Uses your home location: {place}`, or a note that the location is missing. The place comes
/// from the setup status, which carries the home's location.
pub fn place_line(state: Option<SetupState>, lang: Lang) -> String {
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
        Some(p) => setup_text(lang, SetupText::DescWeatherPlace).replace("{place}", &p),
        None => setup_text(lang, SetupText::DescWeatherNoPlace).to_owned(),
    }
}

/// Whether a settings section reads `enabled: true`; `None` when it could not be read.
async fn section_enabled(section: &'static str) -> Option<bool> {
    let v = get_section(section).await.ok()?;
    Some(v.get("enabled").and_then(Value::as_bool).unwrap_or(false))
}

/// Runs `read` on its own task, bounded by [`READ_TIMEOUT_MS`], and stores its answer.
fn load_flag(target: RwSignal<Option<bool>>, read: impl Future<Output = Option<bool>> + 'static) {
    leptos::task::spawn_local(async move {
        if let Some(answer) = within(read).await {
            target.try_set(Some(answer));
        }
    });
}

/// Which services are set up, read from the same sources the cards use. A flag is `None`
/// until its read answers, and stays `None` when the read fails or hangs. Built by
/// [`ServiceFlags::load`], which starts the reads; call it inside the owner of the view.
#[derive(Clone, Copy)]
pub struct ServiceFlags {
    pub weather: RwSignal<Option<bool>>,
    pub solar: RwSignal<Option<bool>>,
    pub energy: RwSignal<Option<bool>>,
    pub cameras: RwSignal<Option<bool>>,
    pub push: RwSignal<Option<bool>>,
    pub climate: RwSignal<Option<bool>>,
    pub alarm: RwSignal<Option<bool>>,
    pub push_project: RwSignal<Option<String>>,
    pub skipped: RwSignal<BTreeSet<String>>,
}

impl ServiceFlags {
    /// Starts the reads. Energy follows the house's grid sensor; the others come from their
    /// settings sections, each read on its own so one slow answer holds back no other.
    pub fn load() -> Self {
        let flags = Self {
            weather: RwSignal::new(None),
            solar: RwSignal::new(None),
            energy: RwSignal::new(Some(false)),
            cameras: RwSignal::new(None),
            push: RwSignal::new(None),
            climate: RwSignal::new(None),
            alarm: RwSignal::new(None),
            push_project: RwSignal::new(None),
            skipped: RwSignal::new(skipped()),
        };
        if let Some(house) = use_context::<House>() {
            Effect::new(move |_| {
                let set_up = house
                    .entities
                    .with(|m| holder(&m.values().cloned().collect::<Vec<_>>(), EnergyRole::Grid))
                    .is_some();
                flags.energy.set(Some(set_up));
            });
        }
        load_flag(flags.weather, section_enabled("weather"));
        load_flag(flags.climate, section_enabled("climate"));
        load_flag(flags.alarm, async {
            crate::api::alarm::get_alarm().await.ok().map(|a| a.enabled)
        });
        load_flag(flags.solar, async {
            let v = get_section("solcast").await.ok()?;
            Some(
                v["enabled"].as_bool().unwrap_or(false)
                    && v["api_key"]["set"].as_bool().unwrap_or(false),
            )
        });
        load_flag(flags.cameras, async {
            let v = get_section("frigate").await.ok()?;
            Some(v["url"].as_str().is_some())
        });
        leptos::task::spawn_local(async move {
            let read = async {
                let v = get_section("push").await.ok()?;
                let project = v["project_id"].as_str().map(str::to_owned);
                Some((
                    v["service_account"]["set"].as_bool().unwrap_or(false),
                    project,
                ))
            };
            if let Some((set, project)) = within(read).await {
                flags.push.try_set(Some(set));
                flags.push_project.try_set(project);
            }
        });
        flags
    }

    fn on(&self, id: &str) -> Option<bool> {
        match id {
            "weather" => self.weather,
            "solar" => self.solar,
            "energy" => self.energy,
            "cameras" => self.cameras,
            "push" => self.push,
            "climate" => self.climate,
            "alarm" => self.alarm,
            _ => return Some(false),
        }
        .get()
    }

    /// What the card for `id` shows right now.
    pub fn state(&self, id: &str) -> CardState {
        card_state(self.on(id), self.skipped.with(|s| s.contains(id)))
    }
}

/// Every service's card state, in [`SERVICE_IDS`] order, for the Done summary. A read still
/// pending or failed reads as unknown, never as "Not set up". Call it inside the owner of the
/// view.
pub fn service_states() -> Signal<Vec<(&'static str, CardState)>> {
    let flags = ServiceFlags::load();
    Signal::derive(move || {
        SERVICE_IDS
            .into_iter()
            .map(|id| (id, flags.state(id)))
            .collect()
    })
}

/// The Services step. Needs [`Applying`] and [`WizardNav`] in context.
#[component]
pub fn ServicesStep() -> impl IntoView {
    let nav = expect_context::<WizardNav>();
    let t = use_setup_text();
    let lang = use_lang();
    let state = use_context::<SetupState>();
    let flags = ServiceFlags::load();
    let skipped_ids = flags.skipped;
    let weather_on = flags.weather;
    let climate_on = flags.climate;
    let alarm_on = flags.alarm;
    let energy_on = flags.energy;
    let solar_on = flags.solar;
    let cameras_on = flags.cameras;
    let push_on = flags.push;
    let push_project = flags.push_project;
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
    let card_state_of = move |id: &'static str, on: Option<RwSignal<Option<bool>>>| {
        Signal::derive(move || {
            card_state(
                on.and_then(|o| o.get()),
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
    let push_desc = Signal::derive(move || match push_project.get() {
        Some(p) => setup_text(lang(), SetupText::PushProject).replace("{project}", &p),
        None => setup_text(lang(), SetupText::DescPush).to_owned(),
    });
    let weather_desc = Signal::derive(move || place_line(state, lang()));
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
                    state=card_state_of("solar", Some(solar_on))
                    on_skip=on_skip("solar")
                >
                    <SolcastForm applying_target=setup_href(SetupStep::Services) />
                </ServiceCard>
                <ServiceCard
                    id="energy"
                    icon=crate::icons::BOLT
                    title=SetupText::SvcEnergy
                    desc=fixed(SetupText::DescEnergy)
                    state=card_state_of("energy", Some(energy_on))
                    on_skip=on_skip("energy")
                >
                    <EnergyForm />
                </ServiceCard>
                <ServiceCard
                    id="cameras"
                    icon=crate::icons::CAMERA
                    title=SetupText::SvcCameras
                    desc=fixed(SetupText::DescCameras)
                    state=card_state_of("cameras", Some(cameras_on))
                    on_skip=on_skip("cameras")
                >
                    <FrigateForm applying_target=setup_href(SetupStep::Services) />
                </ServiceCard>
                <ServiceCard
                    id="push"
                    icon=crate::icons::BELL
                    title=SetupText::SvcPush
                    desc=push_desc
                    state=card_state_of("push", Some(push_on))
                    on_skip=on_skip("push")
                >
                    <PushForm applying_target=setup_href(SetupStep::Services) />
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
        assert_eq!(card_state(Some(true), true), CardState::SetUp);
        assert_eq!(card_state(Some(false), true), CardState::Skipped);
        assert_eq!(card_state(Some(false), false), CardState::NotSetUp);
    }

    #[test]
    fn an_unread_service_is_unknown_unless_skipped() {
        assert_eq!(card_state(None, false), CardState::Unknown);
        assert_eq!(card_state(None, true), CardState::Skipped);
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
