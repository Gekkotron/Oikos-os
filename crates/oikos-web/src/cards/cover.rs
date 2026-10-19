//! The cover card: open, stop and close buttons, a position slider and a "Moving" indicator.

use leptos::prelude::*;
use oikos_api::EntityView;
use oikos_core::model::{Command, CoverState, EntityState, StateValue};

use super::Sender;
use crate::components::card_frame::{CardFrame, open_entity};
use crate::format::display_value;
use crate::house::House;

/// True when a folded child named "moving" says the cover is moving: an on/off state that is
/// on, or a text state of "moving", "opening" or "closing".
pub fn is_moving(children: &[EntityView]) -> bool {
    children
        .iter()
        .filter(|c| c.name.to_lowercase().contains("moving"))
        .any(|c| match &c.state.value {
            StateValue::OnOff(on) => *on,
            StateValue::Text(s) => ["moving", "opening", "closing"]
                .iter()
                .any(|w| s.eq_ignore_ascii_case(w)),
            _ => false,
        })
}

/// `state` with the cover at `position`, the guess shown while a slider move is in flight.
fn with_position(state: &EntityState, position: u8) -> Option<EntityState> {
    let StateValue::Cover(cover) = &state.value else {
        return None;
    };
    Some(EntityState {
        value: StateValue::Cover(CoverState {
            position: Some(position),
            ..cover.clone()
        }),
        ..state.clone()
    })
}

#[component]
pub fn CoverCard(entity: EntityView) -> impl IntoView {
    let sender = Sender::from_context();
    let label = display_value(&entity.state, entity.unit.as_deref());
    // Read once: the parent is rebuilt when its own state changes, not when a child's does.
    let moving = is_moving(&expect_context::<House>().part_of_children(&entity.id));
    let position = match &entity.state.value {
        StateValue::Cover(c) => c.position.unwrap_or(0),
        _ => 0,
    };

    let button = |text: &'static str, cmd: Command| {
        let id = entity.id.clone();
        view! {
            <button
                type="button"
                class="btn"
                on:click=move |_| sender.send(&id, cmd.clone(), None)
            >
                {text}
            </button>
        }
    };
    let buttons = view! {
        <div class="card__controls">
            {button("Open", Command::Open)}
            {button("Stop", Command::Stop)}
            {button("Close", Command::Close)}
        </div>
    };

    let slider = entity.capabilities.position.then(|| {
        let id = entity.id.clone();
        let state = entity.state.clone();
        let on_change = move |ev: leptos::ev::Event| {
            let Ok(value) = event_target_value(&ev).parse::<u8>() else {
                return;
            };
            sender.send(
                &id,
                Command::SetPosition { position: value },
                with_position(&state, value),
            );
        };
        view! {
            <div class="card__controls">
                <input
                    class="slider"
                    type="range"
                    min="0"
                    max="100"
                    step="1"
                    aria-label="Position"
                    prop:value=position.to_string()
                    on:change=on_change
                />
            </div>
        }
    });

    let pending = sender.pending(&entity.id);
    let on_click = open_entity(&entity.id);
    view! {
        <CardFrame entity=entity on_click=on_click pending=pending>
            <span class="card__value">{label}</span>
            {moving.then(|| view! { <span class="badge">"Moving"</span> })}
            {slider}
            {buttons}
        </CardFrame>
    }
}

#[cfg(test)]
mod tests {
    use super::{is_moving, with_position};
    use oikos_api::EntityView;
    use oikos_core::model::{
        Capabilities, CoverState, EntityCategory, EntityState, Kind, StateValue, UiOverride,
    };

    fn child(name: &str, value: StateValue) -> EntityView {
        EntityView {
            id: "x".into(),
            device_id: None,
            name: name.into(),
            device_name: None,
            label: None,
            kind: Kind::BinarySensor,
            protocol_kind: Kind::BinarySensor,
            device_class: None,
            unit: None,
            icon: None,
            category: EntityCategory::Primary,
            enabled_by_default: true,
            area: "salon".into(),
            part_of: Some("cover.hall".into()),
            capabilities: Capabilities::default(),
            ui: UiOverride::default(),
            state: EntityState {
                value,
                ..Default::default()
            },
        }
    }

    #[test]
    fn moving_binary_sensor_on() {
        assert!(is_moving(&[child("Hall Moving", StateValue::OnOff(true))]));
        assert!(!is_moving(&[child(
            "Hall Moving",
            StateValue::OnOff(false)
        )]));
        assert!(!is_moving(&[child("Hall Tilt", StateValue::OnOff(true))]));
    }

    #[test]
    fn moving_text_words() {
        assert!(is_moving(&[child(
            "Moving",
            StateValue::Text("Opening".into())
        )]));
        assert!(!is_moving(&[child(
            "Moving",
            StateValue::Text("idle".into())
        )]));
        assert!(!is_moving(&[]));
    }

    #[test]
    fn position_guess_keeps_the_state_word() {
        let s = EntityState {
            value: StateValue::Cover(CoverState {
                state: Some("open".into()),
                position: Some(100),
            }),
            ..Default::default()
        };
        let guess = with_position(&s, 30).expect("cover");
        assert_eq!(
            guess.value,
            StateValue::Cover(CoverState {
                state: Some("open".into()),
                position: Some(30)
            })
        );
    }
}
