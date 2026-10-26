use leptos::prelude::*;
use oikos_api::EntityView;
use oikos_core::dashboard::{Widget, WidgetConfig, WidgetKind};

use super::camera_widget::CameraWidget;
use super::energy_stat::EnergyStatWidget;
use super::flow_button::FlowButtonWidget;
use super::history_widget::HistoryWidget;
use super::room_group::RoomGroupWidget;
use crate::cards::climate::ClimateCard;
use crate::cards::cover::CoverCard;
use crate::cards::light::LightCard;
use crate::cards::lock::LockCard;
use crate::cards::sensor::SensorCard;
use crate::cards::water_heater::WaterHeaterCard;
use crate::house::House;
use crate::i18n::{use_lang, widget_kind_word, widget_placeholder};

/// Picks the card for a widget. Only the presence of what the widget points at is tracked, so
/// the card mounts once the state snapshot lands (a dashboard opened by its address paints
/// before it) without remounting on every state change; the card subscribes to its own updates.
///
/// `current` is the widget as the dashboard holds it now (the grid keeps a tile mounted across
/// edits); the Camera widget follows its span and camera list, which pick its layout.
#[component]
pub fn WidgetView(
    widget: Widget,
    #[prop(optional, into)] current: Option<Signal<Widget>>,
) -> impl IntoView {
    let house = expect_context::<House>();
    match (widget.kind, widget.config) {
        (WidgetKind::Camera, WidgetConfig::Cameras { cameras, .. }) => {
            let span = widget.span;
            let shape = Memo::new(move |_| match current {
                Some(current) => current.with(|w| match &w.config {
                    WidgetConfig::Cameras { cameras, .. } => (cameras.clone(), w.span),
                    _ => (Vec::new(), w.span),
                }),
                None => (cameras.clone(), span),
            });
            (move || {
                let (cameras, span) = shape.get();
                view! { <CameraWidget cameras=cameras span=span /> }
            })
            .into_any()
        }
        (kind, WidgetConfig::Entity { entity_id, .. }) => {
            entity_widget(house, entity_id.clone(), move |entity| match kind {
                WidgetKind::Light => view! { <LightCard entity=entity /> }.into_any(),
                WidgetKind::Cover => view! { <CoverCard entity=entity /> }.into_any(),
                WidgetKind::Climate => view! { <ClimateCard entity=entity /> }.into_any(),
                WidgetKind::Sensor => view! { <SensorCard entity=entity /> }.into_any(),
                WidgetKind::WaterHeater => view! { <WaterHeaterCard entity=entity /> }.into_any(),
                WidgetKind::Lock => view! { <LockCard entity=entity /> }.into_any(),
                // A kind that takes no entity (an invalid widget): a labelled placeholder.
                _ => {
                    let lang = use_lang();
                    let id = entity_id.clone();
                    let label = Signal::derive(move || widget_placeholder(lang(), kind, &id));
                    view! { <Unavailable label=label /> }.into_any()
                }
            })
        }
        (WidgetKind::HistoryChart, WidgetConfig::History { entity_id, range }) => {
            entity_widget(house, entity_id, move |entity| {
                view! { <HistoryWidget entity=entity range=range /> }.into_any()
            })
        }
        (WidgetKind::RoomGroup, WidgetConfig::Area { area_id, title }) => {
            let room = {
                let id = area_id.clone();
                Memo::new(move |_| {
                    house
                        .areas
                        .with(|a| a.iter().find(|a| a.id == id).map(|a| a.name.clone()))
                })
            };
            (move || match room.get() {
                Some(room) => view! {
                    <RoomGroupWidget area_id=area_id.clone() room=room title=title.clone() />
                }
                .into_any(),
                None => view! { <Unavailable label=area_id.clone() /> }.into_any(),
            })
            .into_any()
        }
        (WidgetKind::FlowButton, WidgetConfig::Flow { flow_id, title }) => {
            let flow = {
                let id = flow_id.clone();
                Memo::new(move |_| {
                    house.flows.with(|flows| {
                        flows
                            .iter()
                            .find(|f| f.definition.id == id)
                            .map(|f| (f.definition.name.clone(), f.invalid.clone()))
                    })
                })
            };
            (move || match flow.get() {
                Some((flow_name, invalid)) => view! {
                    <FlowButtonWidget
                        flow_id=flow_id.clone()
                        flow_name=flow_name
                        title=title.clone()
                        invalid=invalid
                    />
                }
                .into_any(),
                None => view! { <Unavailable label=flow_id.clone() /> }.into_any(),
            })
            .into_any()
        }
        (WidgetKind::EnergyStat, WidgetConfig::Energy { metric, title }) => {
            view! { <EnergyStatWidget metric=metric title=title /> }.into_any()
        }
        (kind, _) => {
            let lang = use_lang();
            let label = Signal::derive(move || widget_kind_word(lang(), kind).to_owned());
            view! { <Unavailable label=label /> }.into_any()
        }
    }
}

/// `render` over the entity `entity_id`, or the placeholder while the house has none.
fn entity_widget(
    house: House,
    entity_id: String,
    render: impl Fn(EntityView) -> AnyView + Send + Sync + 'static,
) -> AnyView {
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
        match found {
            Some(entity) => render(entity),
            None => view! { <Unavailable label=entity_id.clone() /> }.into_any(),
        }
    })
    .into_any()
}

#[component]
pub fn Unavailable(#[prop(into)] label: Signal<String>) -> impl IntoView {
    view! {
        <div class="widget-unavailable">
            <span class="widget-unavailable-icon">"⚠"</span>
            <span class="widget-unavailable-label">{move || label.get()}</span>
        </div>
    }
}
