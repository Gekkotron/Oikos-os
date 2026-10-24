//! The vacuum card: the activity with the battery level, one primary start / pause button, an
//! icon button per other supported action and a fan speed select.

use leptos::prelude::*;
use oikos_api::EntityView;
use oikos_core::model::{Command, EntityState, StateValue, VacuumState};

use super::text::{vacuum_caption, vacuum_sentence};
use super::{ModeSelect, Sender};
use crate::components::card_frame::{CardFrame, open_entity};
use crate::components::card_place::{CardPlace, use_card_place};
use crate::components::robot::Robot;
use crate::format::{display_value, word};
use crate::i18n::{Key, Lang, text, use_lang, use_text};
use crate::icons::vacuum_action_icon;

/// The short button label for a vacuum action.
fn action_label(action: &str) -> String {
    match action {
        "return_to_base" => "Dock".into(),
        "clean_spot" => "Spot".into(),
        other => word(other),
    }
}

/// The primary action for the current activity and its label: pause while cleaning, resume
/// when paused, start otherwise. `None` when the vacuum supports neither.
fn primary_action(
    lang: Lang,
    activity: Option<&str>,
    actions: &[String],
) -> Option<(&'static str, String)> {
    let has = |a: &str| actions.iter().any(|x| x == a);
    let (action, key) = match activity {
        Some("cleaning") if has("pause") => ("pause", Key::Pause),
        Some("paused") if has("start") => ("start", Key::Resume),
        _ if has("start") => ("start", Key::Start),
        _ => return None,
    };
    Some((action, text(lang, key).to_owned()))
}

#[component]
pub fn VacuumCard(entity: EntityView) -> impl IntoView {
    let sender = Sender::from_context();
    let lang = use_lang();
    let t = use_text();
    let in_sheet = use_card_place() == CardPlace::Sheet;
    let label = display_value(
        crate::i18n::current_lang(),
        &entity.state,
        entity.unit.as_deref(),
    );
    let (activity, battery, fan) = match &entity.state.value {
        StateValue::Vacuum(v) => (v.state.clone(), v.battery, v.fan_speed.clone()),
        _ => (None, None, None),
    };
    let actions = entity.capabilities.vacuum_actions.clone();
    let caption_fan = fan.clone();

    let battery_view = battery.map(|level| {
        let fill = format!("width: {}%", level.min(100));
        view! {
            <span class="vacuum__battery" class:vacuum__battery--low=level < 20 title="Battery">
                <span class="vacuum__battery-glyph" aria-hidden="true">
                    <span class="vacuum__battery-fill" style=fill></span>
                </span>
                {format!("{level}%")}
            </span>
        }
    });

    let primary_view =
        primary_action(Lang::En, activity.as_deref(), &actions).map(|(action, _)| {
            let id = entity.id.clone();
            let (actions, activity) = (actions.clone(), activity.clone());
            let text = move || {
                primary_action(lang(), activity.as_deref(), &actions)
                    .map(|(_, text)| text)
                    .unwrap_or_default()
            };
            view! {
                <button
                    type="button"
                    class="btn vacuum__primary"
                    class:vacuum__primary--pause=action == "pause"
                    on:click=move |_| {
                        sender.send(&id, Command::VacuumAction { action: action.into() }, None);
                    }
                >
                    <span class="vacuum__primary-icon" inner_html=vacuum_action_icon(action)></span>
                    {text}
                </button>
            }
        });
    let home_view = actions.iter().any(|a| a == "return_to_base").then(|| {
        let id = entity.id.clone();
        view! {
            <button
                type="button"
                class="btn btn--round vacuum__home"
                aria-label=move || t(Key::SendHome)
                inner_html=vacuum_action_icon("return_to_base")
                on:click=move |_| {
                    let action = "return_to_base".to_owned();
                    sender.send(&id, Command::VacuumAction { action }, None);
                }
            ></button>
        }
    });

    // The other actions and the fan speed are one tap away, in the sheet.
    let secondary = in_sheet.then(|| {
        actions
            .iter()
            .filter(|a| !matches!(a.as_str(), "start" | "pause" | "return_to_base"))
            .cloned()
            .map(|action| {
                let id = entity.id.clone();
                let text = action_label(&action);
                let icon = vacuum_action_icon(&action);
                view! {
                    <button
                        type="button"
                        class="btn btn--icon"
                        aria-label=text.clone()
                        title=text
                        inner_html=icon
                        on:click=move |_| {
                            sender.send(&id, Command::VacuumAction { action: action.clone() }, None);
                        }
                    ></button>
                }
            })
            .collect_view()
    });
    let fan_select = (in_sheet && !entity.capabilities.fan_speeds.is_empty()).then(|| {
        let id = entity.id.clone();
        let state = entity.state.clone();
        let on_select = Callback::new(move |speed: String| {
            let guess = match &state.value {
                StateValue::Vacuum(v) => Some(EntityState {
                    value: StateValue::Vacuum(VacuumState {
                        fan_speed: Some(speed.clone()),
                        ..v.clone()
                    }),
                    ..state.clone()
                }),
                _ => None,
            };
            sender.send(&id, Command::SetFanSpeed { speed }, guess);
        });
        let options = entity.capabilities.fan_speeds.clone();
        view! {
            <ModeSelect label="Fan speed" options=options selected=fan on_select=on_select captioned=true />
        }
    });

    let sentence_activity = activity.clone();
    let sentence = move || {
        vacuum_sentence(lang(), sentence_activity.as_deref())
            .map(|s| view! { <p class="vacuum__sentence">{s}</p> })
    };
    // Without a sentence the state line says what the hub said.
    let state_line = vacuum_sentence(Lang::En, activity.as_deref())
        .is_none()
        .then(|| Signal::derive(move || label.clone()));
    let caption_activity = activity.clone();
    let caption = move || {
        vacuum_caption(lang(), caption_activity.as_deref(), caption_fan.as_deref())
            .map(|c| view! { <span class="vacuum__caption">{c}</span> })
    };
    let trailing = battery_view.map(|b| b.into_any());

    let pending = sender.pending(&entity.id);
    let on_click = open_entity(&entity.id);
    view! {
        <CardFrame
            entity=entity
            on_click=on_click
            pending=pending
            trailing=trailing.unwrap_or_else(|| ().into_any())
            wide=!in_sheet
        >
            <div class="vacuum">
                <div class="vacuum__main">
                    {state_line.map(|s| view! { <p class="card__state">{move || s.get()}</p> })}
                    {sentence}
                    <div class="vacuum__buttons">{primary_view}{home_view}</div>
                    {secondary.map(|s| view! { <div class="card__controls vacuum__actions">{s}</div> })}
                    {fan_select}
                </div>
                <div class="vacuum__picture">
                    <Robot activity=activity />
                    {caption}
                </div>
            </div>
        </CardFrame>
    }
}

#[cfg(test)]
mod tests {
    use super::{action_label, primary_action};
    use crate::i18n::Lang;

    #[test]
    fn short_action_labels() {
        assert_eq!(action_label("return_to_base"), "Dock");
        assert_eq!(action_label("clean_spot"), "Spot");
        assert_eq!(action_label("locate"), "Locate");
        assert_eq!(action_label("start"), "Start");
        assert_eq!(action_label("pause"), "Pause");
        assert_eq!(action_label("stop"), "Stop");
        assert_eq!(action_label("custom_thing"), "Custom thing");
    }

    #[test]
    fn primary_follows_the_activity() {
        let en = Lang::En;
        let both = vec!["start".to_owned(), "pause".to_owned()];
        assert_eq!(
            primary_action(en, Some("docked"), &both),
            Some(("start", "Start".into()))
        );
        assert_eq!(
            primary_action(en, None, &both),
            Some(("start", "Start".into()))
        );
        assert_eq!(
            primary_action(en, Some("cleaning"), &both),
            Some(("pause", "Pause".into()))
        );
        assert_eq!(
            primary_action(en, Some("paused"), &both),
            Some(("start", "Resume".into()))
        );
        assert_eq!(
            primary_action(Lang::Fr, Some("docked"), &both),
            Some(("start", "Démarrer".into()))
        );
        let start_only = vec!["start".to_owned()];
        assert_eq!(
            primary_action(en, Some("cleaning"), &start_only),
            Some(("start", "Start".into()))
        );
        assert_eq!(
            primary_action(en, Some("docked"), &["locate".to_owned()]),
            None
        );
    }
}
