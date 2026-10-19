//! The sensor card: the big value and its unit.

use leptos::prelude::*;
use oikos_api::EntityView;
use oikos_core::model::EntityState;

use super::Sender;
use crate::components::card_frame::{CardFrame, open_entity};
use crate::format::{display_value, value_class};

/// The text a sensor shows: the value with its unit, "—" while unknown or unavailable.
pub fn sensor_text(state: &EntityState, unit: Option<&str>) -> String {
    display_value(state, unit)
}

#[component]
pub fn SensorCard(entity: EntityView) -> impl IntoView {
    let sender = Sender::from_context();
    let text = sensor_text(&entity.state, entity.unit.as_deref());
    let class = match value_class(&text) {
        "card__value" => "card__value card__value--big",
        long => long,
    };
    let pending = sender.pending(&entity.id);
    let on_click = open_entity(&entity.id);
    view! {
        <CardFrame entity=entity on_click=on_click pending=pending>
            <span class=class>{text}</span>
        </CardFrame>
    }
}

#[cfg(test)]
mod tests {
    use super::sensor_text;
    use oikos_core::model::{EntityState, StateValue};
    #[test]
    fn unknown_sensor_text() {
        let s = EntityState {
            value: StateValue::Text("unknown".into()),
            ..Default::default()
        };
        assert_eq!(sensor_text(&s, Some("W")), "—");
    }
    #[test]
    fn numeric_sensor_text() {
        let s = EntityState {
            value: StateValue::Number(42.0),
            ..Default::default()
        };
        assert_eq!(sensor_text(&s, Some("W")), "42 W");
    }
}
