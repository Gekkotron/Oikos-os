//! The binary sensor card: a state word, the icon tinted by the card's on state.

use leptos::prelude::*;
use oikos_api::EntityView;
use oikos_core::model::StateValue;

use super::Sender;
use crate::components::card_frame::{CardFrame, open_entity};

/// The word for a binary sensor: "Open"/"Closed" for doors and windows, "Motion"/"Clear" for
/// motion, "Smoke"/"Clear" for smoke, "On"/"Off" otherwise.
pub fn binary_text(device_class: Option<&str>, on: bool) -> &'static str {
    match (device_class, on) {
        (Some("door" | "window" | "garage_door" | "opening"), true) => "Open",
        (Some("door" | "window" | "garage_door" | "opening"), false) => "Closed",
        (Some("motion" | "occupancy" | "presence"), true) => "Motion",
        (Some("smoke"), true) => "Smoke",
        (Some("motion" | "occupancy" | "presence" | "smoke"), false) => "Clear",
        (_, true) => "On",
        (_, false) => "Off",
    }
}

#[component]
pub fn BinarySensorCard(entity: EntityView) -> impl IntoView {
    let sender = Sender::from_context();
    let text = match &entity.state.value {
        StateValue::OnOff(on) => {
            Signal::stored(binary_text(entity.device_class.as_deref(), *on).to_owned())
        }
        _ => super::live_value(&entity),
    };
    let pending = sender.pending(&entity.id);
    let on_click = open_entity(&entity.id);
    view! {
        <CardFrame entity=entity on_click=on_click pending=pending>
            <span class="card__value">{text}</span>
        </CardFrame>
    }
}

#[cfg(test)]
mod tests {
    use super::binary_text;

    #[test]
    fn words_by_device_class() {
        assert_eq!(binary_text(Some("door"), true), "Open");
        assert_eq!(binary_text(Some("window"), false), "Closed");
        assert_eq!(binary_text(Some("motion"), true), "Motion");
        assert_eq!(binary_text(Some("motion"), false), "Clear");
        assert_eq!(binary_text(Some("smoke"), true), "Smoke");
        assert_eq!(binary_text(Some("smoke"), false), "Clear");
        assert_eq!(binary_text(None, true), "On");
        assert_eq!(binary_text(Some("battery"), false), "Off");
    }
}
