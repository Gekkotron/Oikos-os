//! The Home panel in Settings: the five home-header settings and the off-peak hours behind one
//! Save.

use leptos::prelude::*;
#[cfg(target_arch = "wasm32")]
use leptos::task::spawn_local;

use oikos_api::{HomeSettings, OffPeakWindow};
use oikos_core::model::Kind;

use crate::climate::entity_field::ThermometerField;
use crate::components::toast::Toasts;
use crate::format::list_label;
use crate::home_header::mood::{COLD_DEFAULT, HOT_DEFAULT};
use crate::house::House;
use crate::i18n::format::parse_decimal;
use crate::i18n::settings::{
    RULE_BAD_TIME, RULE_BETWEEN, RULE_LOWER_THAN, RULE_NOT_A, RULE_START_EQUALS_END,
};
use crate::i18n::{HomeText, Lang, home_text};

/// A setting the panel edits, for pointing an error at its field.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    Eco,
    Mode,
    Indoor,
    Cold,
    Hot,
    OffPeak,
}

/// The fields as typed: numbers as text, entity pickers as ids, off-peak windows as
/// `(start, end)` `HH:MM` pairs.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Draft {
    pub eco: String,
    pub mode: Option<String>,
    pub indoor: Option<String>,
    pub cold: String,
    pub hot: String,
    pub off_peak: Vec<(String, String)>,
}

fn number_text(v: Option<f64>) -> String {
    v.map(|v| v.to_string()).unwrap_or_default()
}

impl From<&HomeSettings> for Draft {
    fn from(s: &HomeSettings) -> Self {
        Draft {
            eco: number_text(s.eco_setpoint),
            mode: s.mode_entity.clone(),
            indoor: s.indoor_entity.clone(),
            cold: number_text(s.cold_below),
            hot: number_text(s.hot_above),
            off_peak: s
                .off_peak
                .iter()
                .map(|w| (w.start.clone(), w.end.clone()))
                .collect(),
        }
    }
}

/// `,` or `.` as the decimal mark; empty is `None`. A text that is no number names its field.
fn parse_number(text: &str, field: Field) -> Result<Option<f64>, Field> {
    if text.trim().is_empty() {
        return Ok(None);
    }
    parse_decimal(text).map(Some).ok_or(field)
}

/// The draft to save: each field from `draft` when it was edited (differs from `base`, the copy the
/// edits started from), else from `latest`, so a change made elsewhere meanwhile is not undone.
pub fn merge(base: &Draft, draft: &Draft, latest: &Draft) -> Draft {
    Draft {
        eco: if draft.eco != base.eco {
            &draft.eco
        } else {
            &latest.eco
        }
        .clone(),
        mode: if draft.mode != base.mode {
            &draft.mode
        } else {
            &latest.mode
        }
        .clone(),
        indoor: if draft.indoor != base.indoor {
            &draft.indoor
        } else {
            &latest.indoor
        }
        .clone(),
        cold: if draft.cold != base.cold {
            &draft.cold
        } else {
            &latest.cold
        }
        .clone(),
        hot: if draft.hot != base.hot {
            &draft.hot
        } else {
            &latest.hot
        }
        .clone(),
        off_peak: if draft.off_peak != base.off_peak {
            &draft.off_peak
        } else {
            &latest.off_peak
        }
        .clone(),
    }
}

/// What sits under a field: our own translated text, or the hub's message when unmapped.
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))] // `Raw` is built by the wasm save path
#[derive(Clone, Debug, PartialEq, Eq)]
enum Message {
    Key(HomeText),
    Raw(String),
}

/// The translated text for a hub validation message on `field`; `None` when it is not one of the
/// known rules.
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))] // only the wasm save path calls it
fn message_key(field: Field, message: &str) -> Option<HomeText> {
    if field == Field::OffPeak {
        return if message.contains("overlap") {
            Some(HomeText::WindowsOverlap)
        } else if message.contains(RULE_START_EQUALS_END) {
            Some(HomeText::StartEqualsEnd)
        } else if message.contains(RULE_BAD_TIME) {
            Some(HomeText::BadTime)
        } else {
            None
        };
    }
    if message.contains(RULE_BETWEEN) {
        match field {
            Field::Eco => Some(HomeText::EcoRange),
            Field::Cold | Field::Hot => Some(HomeText::ThresholdRange),
            _ => None,
        }
    } else if message.contains(RULE_LOWER_THAN) {
        Some(HomeText::OrderRule)
    } else if message.contains(RULE_NOT_A) {
        matches!(field, Field::Mode | Field::Indoor).then_some(HomeText::EntityRule)
    } else {
        None
    }
}

/// The settings document the draft stands for, or the first number that does not parse.
pub fn draft_to_settings(draft: &Draft) -> Result<HomeSettings, Field> {
    Ok(HomeSettings {
        eco_setpoint: parse_number(&draft.eco, Field::Eco)?,
        mode_entity: draft.mode.clone(),
        indoor_entity: draft.indoor.clone(),
        cold_below: parse_number(&draft.cold, Field::Cold)?,
        hot_above: parse_number(&draft.hot, Field::Hot)?,
        off_peak: draft
            .off_peak
            .iter()
            .filter(|(start, end)| !(start.trim().is_empty() && end.trim().is_empty()))
            .map(|(start, end)| OffPeakWindow {
                start: start.trim().to_owned(),
                end: end.trim().to_owned(),
            })
            .collect(),
        off_peak_history: Vec::new(),
    })
}

/// The field a hub validation message names; `None` when it names none of them.
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))] // only the wasm save path calls it
fn field_of_message(message: &str) -> Option<Field> {
    [
        ("eco_setpoint", Field::Eco),
        ("mode_entity", Field::Mode),
        ("indoor_entity", Field::Indoor),
        ("cold_below", Field::Cold),
        ("hot_above", Field::Hot),
        ("off_peak", Field::OffPeak),
    ]
    .into_iter()
    .find(|(name, _)| message.contains(name))
    .map(|(_, field)| field)
}

/// The primary `select` entities as `(id, label)`, plus `keep` (the chosen one) even when it is
/// not primary: the hub's virtual ones first, then by label.
fn mode_options(house: House, lang: Lang, keep: Option<&str>) -> Vec<(String, String)> {
    let mut list: Vec<(String, String)> = house.entities.with(|m| {
        m.values()
            .filter(|e| e.kind == Kind::Select && (e.is_primary() || keep == Some(e.id.as_str())))
            .map(|e| (e.id.clone(), list_label(lang, e)))
            .collect()
    });
    list.sort_by_key(|(id, label)| (!id.starts_with("virtual_"), label.to_lowercase()));
    list
}

#[component]
pub fn HomeSettingsPanel() -> impl IntoView {
    let house = expect_context::<House>();
    let lang = use_context::<RwSignal<Lang>>();
    let t = move |k: HomeText| home_text(lang.map_or(Lang::En, |l| l.get()), k);

    let fresh_draft = house.home_settings.with_untracked(|s| Draft::from(s));
    let draft = RwSignal::new(fresh_draft.clone());
    // The copy the edits started from, to tell edited fields from untouched ones on Save.
    let base = RwSignal::new(fresh_draft);
    let error = RwSignal::new(None::<(Field, Message)>);
    let saving = RwSignal::new(false);

    // A change from elsewhere (another tab, the MCP, our own save) refills the form, unless
    // there are edits in progress.
    Effect::new(move |_| {
        let fresh = house.home_settings.with(|s| Draft::from(s));
        if draft.get_untracked() == base.get_untracked() {
            draft.set(fresh.clone());
            base.set(fresh);
        }
    });

    let save = move |_| {
        let latest = house.home_settings.with_untracked(|s| Draft::from(s));
        let merged = merge(&base.get_untracked(), &draft.get_untracked(), &latest);
        let settings = match draft_to_settings(&merged) {
            Ok(s) => s,
            Err(field) => {
                error.set(Some((field, Message::Key(HomeText::NotANumber))));
                return;
            }
        };
        error.set(None);
        saving.set(true);
        #[cfg(target_arch = "wasm32")]
        {
            let toasts = use_context::<Toasts>();
            spawn_local(async move {
                match crate::api::home::put_home_settings(&settings).await {
                    Ok(stored) => {
                        let fresh = Draft::from(&stored);
                        draft.try_set(fresh.clone());
                        base.try_set(fresh);
                        house.home_settings.set(stored);
                        if let Some(toasts) = toasts {
                            toasts.push(t(HomeText::Saved));
                        }
                    }
                    Err(e) => match field_of_message(&e.error) {
                        Some(field) => {
                            let message = message_key(field, &e.error)
                                .map_or(Message::Raw(e.error), Message::Key);
                            error.try_set(Some((field, message)));
                        }
                        None => {
                            if let Some(toasts) = toasts {
                                toasts.push(format!("{}: {}", t(HomeText::CouldNotSave), e.error));
                            }
                        }
                    },
                }
                saving.try_set(false);
            });
        }
        #[cfg(not(target_arch = "wasm32"))]
        let _ = (settings, use_context::<Toasts>());
    };

    let error_for = move |field: Field| {
        move || {
            error.with(|e| {
                e.as_ref().filter(|(f, _)| *f == field).map(|(_, message)| {
                    let text = match message {
                        Message::Key(k) => t(*k).to_owned(),
                        Message::Raw(raw) => raw.clone(),
                    };
                    view! { <p class="home-settings__error">{text}</p> }
                })
            })
        }
    };

    let number = move |field: Field,
                       label: HomeText,
                       placeholder: String,
                       get: fn(&Draft) -> String,
                       set: fn(&mut Draft, String)| {
        view! {
            <div class="climate-field">
                <label class="climate-field__label" for=format!("home-{field:?}")>
                    {move || t(label)}
                </label>
                <input
                    id=format!("home-{field:?}")
                    class="field"
                    type="text"
                    inputmode="decimal"
                    placeholder=placeholder
                    prop:value=move || draft.with(get)
                    on:input=move |ev| draft.update(|d| set(d, event_target_value(&ev)))
                />
                {error_for(field)}
            </div>
        }
    };

    let mode_value = Signal::derive(move || draft.with(|d| d.mode.clone()));

    view! {
        <section class="sheet__panel home-settings">
            <h2 class="sheet__heading">{move || t(HomeText::Title)}</h2>
            <p class="settings__hint">{move || t(HomeText::Hint)}</p>
            {number(
                Field::Eco,
                HomeText::EcoSetpoint,
                String::new(),
                |d| d.eco.clone(),
                |d, v| d.eco = v,
            )}
            <div class="climate-field">
                <label class="climate-field__label" for="home-Mode">
                    {move || t(HomeText::ModeSelector)}
                </label>
                <select
                    id="home-Mode"
                    class="field"
                    on:change=move |ev| {
                        let v = event_target_value(&ev);
                        draft.update(|d| d.mode = (!v.is_empty()).then_some(v));
                    }
                >
                    <option value="" selected=move || mode_value.get().is_none()>
                        {move || t(HomeText::NoneChoice)}
                    </option>
                    {move || {
                        let current = mode_value.get();
                        let mut list =
                            mode_options(house, lang.map_or(Lang::En, |l| l.get()), current.as_deref());
                        if let Some(id) = current.as_ref()
                            && !list.iter().any(|(i, _)| i == id)
                        {
                            list.insert(0, (id.clone(), id.clone()));
                        }
                        list.into_iter()
                            .map(|(id, text)| {
                                let selected = current.as_deref() == Some(id.as_str());
                                view! { <option value=id selected=selected>{text}</option> }
                            })
                            .collect_view()
                    }}
                </select>
                {error_for(Field::Mode)}
            </div>
            <div class="home-settings__field">
                <ThermometerField
                    value=Signal::derive(move || draft.with(|d| d.indoor.clone()))
                    auto=Signal::derive(|| None)
                    on_change=Callback::new(move |v| draft.update(|d| d.indoor = v))
                    label=Signal::derive(move || t(HomeText::IndoorSensor).to_owned())
                    auto_text=Signal::derive(move || t(HomeText::AutoMean).to_owned())
                />
                {error_for(Field::Indoor)}
            </div>
            {number(
                Field::Cold,
                HomeText::ColdBelow,
                COLD_DEFAULT.to_string(),
                |d| d.cold.clone(),
                |d, v| d.cold = v,
            )}
            {number(
                Field::Hot,
                HomeText::HotAbove,
                HOT_DEFAULT.to_string(),
                |d| d.hot.clone(),
                |d, v| d.hot = v,
            )}
            <div class="climate-field home-settings__off-peak">
                <span class="climate-field__label">{move || t(HomeText::OffPeakHours)}</span>
                <p class="settings__hint">{move || t(HomeText::OffPeakHint)}</p>
                {move || {
                    let rows = draft.with(|d| d.off_peak.len());
                    (0..rows)
                        .map(|i| {
                            let get = move |end: bool| {
                                move || {
                                    draft.with(|d| {
                                        d.off_peak
                                            .get(i)
                                            .map(|(a, b)| if end { b.clone() } else { a.clone() })
                                            .unwrap_or_default()
                                    })
                                }
                            };
                            let set = move |end: bool| {
                                move |ev| {
                                    let v = event_target_value(&ev);
                                    draft.update(|d| {
                                        if let Some(row) = d.off_peak.get_mut(i) {
                                            if end { row.1 = v } else { row.0 = v }
                                        }
                                    });
                                }
                            };
                            view! {
                                <div class="home-settings__window">
                                    <input
                                        class="field"
                                        type="time"
                                        aria-label=move || format!("{} {}", t(HomeText::OffPeakHours), i + 1)
                                        prop:value=get(false)
                                        on:change=set(false)
                                    />
                                    <span aria-hidden="true">"–"</span>
                                    <input
                                        class="field"
                                        type="time"
                                        prop:value=get(true)
                                        on:change=set(true)
                                    />
                                    <button
                                        type="button"
                                        class="btn btn--compact"
                                        on:click=move |_| {
                                            draft.update(|d| {
                                                if i < d.off_peak.len() {
                                                    d.off_peak.remove(i);
                                                }
                                            })
                                        }
                                    >
                                        {move || t(HomeText::RemoveWindow)}
                                    </button>
                                </div>
                            }
                        })
                        .collect_view()
                }}
                <button
                    type="button"
                    class="btn btn--compact home-settings__add"
                    on:click=move |_| draft.update(|d| d.off_peak.push((String::new(), String::new())))
                >
                    {move || t(HomeText::AddWindow)}
                </button>
                {error_for(Field::OffPeak)}
            </div>
            <button
                type="button"
                class="chip home-settings__save"
                prop:disabled=move || saving.get()
                on:click=save
            >
                {move || t(HomeText::Save)}
            </button>
        </section>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draft(eco: &str, cold: &str, hot: &str) -> Draft {
        Draft {
            eco: eco.into(),
            mode: None,
            indoor: None,
            cold: cold.into(),
            hot: hot.into(),
            off_peak: vec![],
        }
    }

    #[test]
    fn draft_parses_numbers() {
        let s = draft_to_settings(&draft("15,5", "", "27.5")).unwrap();
        assert_eq!(s.eco_setpoint, Some(15.5));
        assert_eq!(s.cold_below, None);
        assert_eq!(s.hot_above, Some(27.5));
        let field = draft_to_settings(&draft("abc", "", "")).unwrap_err();
        assert_eq!(field, Field::Eco);
        let field = draft_to_settings(&draft("", "", "x")).unwrap_err();
        assert_eq!(field, Field::Hot);
    }

    fn full(eco: &str, mode: Option<&str>) -> Draft {
        Draft {
            eco: eco.into(),
            mode: mode.map(Into::into),
            indoor: None,
            cold: "15".into(),
            hot: "26".into(),
            off_peak: vec![],
        }
    }

    #[test]
    fn merge_untouched_field_takes_the_latest() {
        let base = full("15", None);
        let draft = full("15", Some("virtual_mode"));
        let latest = full("17", None);
        let got = merge(&base, &draft, &latest);
        assert_eq!(got.eco, "17");
        assert_eq!(got.mode.as_deref(), Some("virtual_mode"));
    }

    #[test]
    fn merge_edited_field_keeps_the_edit() {
        let base = full("15", None);
        let draft = full("16", None);
        assert_eq!(merge(&base, &draft, &base).eco, "16");
    }

    #[test]
    fn merge_edit_wins_over_a_change_elsewhere() {
        let base = full("15", None);
        let draft = full("16", None);
        let latest = full("18", None);
        assert_eq!(merge(&base, &draft, &latest).eco, "16");
    }

    #[test]
    fn hub_messages_map_to_translated_rules() {
        let between = "eco_setpoint must be between 5 and 30 °C";
        assert_eq!(message_key(Field::Eco, between), Some(HomeText::EcoRange));
        assert_eq!(
            message_key(Field::Hot, "hot_above must be between 5 and 35 °C"),
            Some(HomeText::ThresholdRange)
        );
        assert_eq!(
            message_key(Field::Cold, "cold_below must be lower than hot_above"),
            Some(HomeText::OrderRule)
        );
        assert_eq!(
            message_key(Field::Mode, "mode_entity `x` is not a select"),
            Some(HomeText::EntityRule)
        );
        assert_eq!(
            message_key(Field::Indoor, "indoor_entity `x` is not an entity"),
            Some(HomeText::EntityRule)
        );
        assert_eq!(message_key(Field::Eco, "boom"), None);
    }

    #[test]
    fn hub_messages_name_their_field() {
        assert_eq!(
            field_of_message("cold_below must be below hot_above"),
            Some(Field::Cold)
        );
        assert_eq!(
            field_of_message("eco_setpoint must be between 5 and 30"),
            Some(Field::Eco)
        );
        assert_eq!(field_of_message("mode_entity: unknown"), Some(Field::Mode));
        assert_eq!(
            field_of_message("indoor_entity: unknown"),
            Some(Field::Indoor)
        );
        assert_eq!(field_of_message("boom"), None);
    }

    fn window(start: &str, end: &str) -> OffPeakWindow {
        OffPeakWindow {
            start: start.into(),
            end: end.into(),
        }
    }

    #[test]
    fn draft_round_trips_off_peak() {
        let d = Draft {
            off_peak: vec![("01:34".into(), "07:04".into())],
            ..draft("", "", "")
        };
        let s = draft_to_settings(&d).unwrap();
        assert_eq!(s.off_peak, vec![window("01:34", "07:04")]);
        assert!(s.off_peak_history.is_empty());
        assert_eq!(Draft::from(&s).off_peak, d.off_peak);
    }

    #[test]
    fn draft_drops_blank_windows() {
        let d = Draft {
            off_peak: vec![("".into(), "".into()), ("22:00".into(), "06:00".into())],
            ..draft("", "", "")
        };
        assert_eq!(
            draft_to_settings(&d).unwrap().off_peak,
            vec![window("22:00", "06:00")]
        );
    }

    #[test]
    fn merge_keeps_edited_windows() {
        let base = full("15", None);
        let draft = Draft {
            off_peak: vec![("12:05".into(), "14:34".into())],
            ..full("15", None)
        };
        let latest = full("17", None);
        let got = merge(&base, &draft, &latest);
        assert_eq!(got.off_peak, draft.off_peak);
        assert_eq!(got.eco, "17");
        let untouched = merge(&base, &base, &draft);
        assert_eq!(untouched.off_peak, draft.off_peak);
    }

    #[test]
    fn off_peak_messages_map_to_field() {
        let overlap = "off_peak windows 1 and 2 overlap";
        assert_eq!(field_of_message(overlap), Some(Field::OffPeak));
        assert_eq!(
            message_key(Field::OffPeak, overlap),
            Some(HomeText::WindowsOverlap)
        );
        assert_eq!(
            message_key(Field::OffPeak, "off_peak window 1: bad time"),
            Some(HomeText::BadTime)
        );
        assert_eq!(
            message_key(Field::OffPeak, "off_peak window 2: start equals end"),
            Some(HomeText::StartEqualsEnd)
        );
    }
}
