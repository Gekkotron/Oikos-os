use leptos::prelude::*;
use oikos_core::dashboard::{Widget, WidgetConfig, WidgetKind};

use crate::cards::climate::ClimateCard;
use crate::cards::cover::CoverCard;
use crate::cards::light::LightCard;
use crate::cards::sensor::SensorCard;
use crate::cards::water_heater::WaterHeaterCard;
use crate::house::House;

/// Picks the card for a widget. Only the entity's presence is tracked, so the card mounts once
/// the state snapshot lands (a dashboard opened by its address paints before it) without
/// remounting on every state change; the card subscribes to its own updates.
#[component]
pub fn WidgetView(widget: Widget) -> impl IntoView {
    let house = expect_context::<House>();
    let (WidgetConfig::Entity { entity_id, .. }, kind) = (widget.config, widget.kind) else {
        return view! { <Unavailable label=format!("{:?}", widget.kind) /> }.into_any();
    };
    let present = {
        let id = entity_id.clone();
        Memo::new(move |_| house.entities.with(|m| m.contains_key(&id)))
    };
    (move || {
        let found = present
            .get()
            .then(|| {
                house
                    .entities
                    .with_untracked(|m| m.get(&entity_id).cloned())
            })
            .flatten();
        let Some(entity) = found else {
            return view! { <Unavailable label=entity_id.clone() /> }.into_any();
        };
        match kind {
            WidgetKind::Light => view! { <LightCard entity=entity /> }.into_any(),
            WidgetKind::Cover => view! { <CoverCard entity=entity /> }.into_any(),
            WidgetKind::Climate => view! { <ClimateCard entity=entity /> }.into_any(),
            WidgetKind::Sensor => view! { <SensorCard entity=entity /> }.into_any(),
            WidgetKind::WaterHeater => view! { <WaterHeaterCard entity=entity /> }.into_any(),
            // Kinds without a card yet get a labelled placeholder until their own tasks land.
            _ => view! { <Unavailable label=format!("{:?}: {}", kind, entity_id) /> }.into_any(),
        }
    })
    .into_any()
}

#[component]
pub fn Unavailable(label: String) -> impl IntoView {
    view! {
        <div class="widget-unavailable">
            <span class="widget-unavailable-icon">"⚠"</span>
            <span class="widget-unavailable-label">{label}</span>
        </div>
    }
}
