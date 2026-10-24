//! The alarm panel card: the current state, one button per arming mode and Disarm, with an
//! inline code entry when the panel asks for one.

use leptos::prelude::*;
use oikos_api::EntityView;
use oikos_core::model::{AlarmMode, CodeFormat, Command};

use super::Sender;
use crate::components::card_frame::{CardFrame, open_entity};
use crate::format::display_value;

/// The button label for an arming mode.
pub fn mode_label(mode: AlarmMode) -> &'static str {
    match mode {
        AlarmMode::Home => "Home",
        AlarmMode::Away => "Away",
        AlarmMode::Night => "Night",
        AlarmMode::Vacation => "Vacation",
        AlarmMode::CustomBypass => "Custom bypass",
    }
}

/// The code to send: the typed one when `required` and not blank, else none.
fn code_for(required: bool, typed: &str) -> Option<String> {
    let typed = typed.trim();
    (required && !typed.is_empty()).then(|| typed.to_owned())
}

#[component]
pub fn AlarmPanelCard(entity: EntityView) -> impl IntoView {
    let sender = Sender::from_context();
    let label = display_value(
        crate::i18n::current_lang(),
        &entity.state,
        entity.unit.as_deref(),
    );
    let caps = &entity.capabilities;
    let (arm_required, disarm_required) = (caps.code_arm_required, caps.code_disarm_required);
    let numeric = caps.code_format == CodeFormat::Number;
    let typed = RwSignal::new(String::new());

    // The hub's answer arrives as a state echo, so there is no optimistic guess: the card just
    // shows pending.
    let arm_buttons = caps
        .alarm_modes
        .iter()
        .copied()
        .map(|mode| {
            let id = entity.id.clone();
            view! {
                <button
                    type="button"
                    class="btn btn--compact"
                    on:click=move |_| {
                        let code = code_for(arm_required, &typed.get_untracked());
                        typed.set(String::new());
                        sender.send(&id, Command::Arm { mode, code }, None);
                    }
                >
                    {mode_label(mode)}
                </button>
            }
        })
        .collect_view();

    let disarm = {
        let id = entity.id.clone();
        move |_| {
            let code = code_for(disarm_required, &typed.get_untracked());
            typed.set(String::new());
            sender.send(&id, Command::Disarm { code }, None);
        }
    };

    let code_input = (arm_required || disarm_required).then(|| {
        view! {
            <input
                class="field"
                type=if numeric { "number" } else { "text" }
                inputmode=if numeric { "numeric" } else { "text" }
                autocomplete="off"
                placeholder="Code"
                aria-label="Code"
                prop:value=move || typed.get()
                on:input=move |ev| typed.set(event_target_value(&ev))
            />
        }
    });

    let pending = sender.pending(&entity.id);
    let on_click = open_entity(&entity.id);
    view! {
        <CardFrame entity=entity on_click=on_click pending=pending>
            <span class="card__value">{label}</span>
            <div class="card__controls">{code_input}</div>
            <div class="card__controls card__controls--grid">
                {arm_buttons}
                <button type="button" class="btn btn--compact" on:click=disarm>
                    "Disarm"
                </button>
            </div>
        </CardFrame>
    }
}

#[cfg(test)]
mod tests {
    use super::{code_for, mode_label};
    use oikos_core::model::AlarmMode;

    #[test]
    fn code_is_sent_only_when_required_and_typed() {
        assert_eq!(code_for(true, " 1234 "), Some("1234".into()));
        assert_eq!(code_for(true, "  "), None);
        assert_eq!(code_for(false, "1234"), None);
    }

    #[test]
    fn modes_have_short_labels() {
        assert_eq!(mode_label(AlarmMode::Away), "Away");
        assert_eq!(mode_label(AlarmMode::CustomBypass), "Custom bypass");
    }
}
