//! The reactive house: one signal per slice of [`crate::house_state::HouseState`].

use std::collections::BTreeMap;

use leptos::prelude::*;
use oikos_api::{
    AreaView, BridgeView, ConnectionStatus, DeviceView, EntityView, IgnoredView, WsEvent,
};

#[derive(Clone, Copy)]
pub struct House {
    pub connection: RwSignal<ConnectionStatus>,
    pub bridge: RwSignal<BridgeView>,
    pub areas: RwSignal<Vec<AreaView>>,
    pub devices: RwSignal<Vec<DeviceView>>,
    pub entities: RwSignal<BTreeMap<String, EntityView>>,
    pub ignored: RwSignal<Vec<IgnoredView>>,
    /// The last rejected command as `(entity id, reason)`.
    pub command_failed: RwSignal<Option<(String, String)>>,
}

impl House {
    pub fn new() -> Self {
        Self {
            connection: RwSignal::new(ConnectionStatus::default()),
            bridge: RwSignal::new(BridgeView::default()),
            areas: RwSignal::new(Vec::new()),
            devices: RwSignal::new(Vec::new()),
            entities: RwSignal::new(BTreeMap::new()),
            ignored: RwSignal::new(Vec::new()),
            command_failed: RwSignal::new(None),
        }
    }

    /// The entities folded into `parent_id` through `part_of`, read without subscribing.
    pub fn part_of_children(&self, parent_id: &str) -> Vec<EntityView> {
        self.entities.with_untracked(|m| {
            m.values()
                .filter(|e| e.part_of.as_deref() == Some(parent_id))
                .cloned()
                .collect()
        })
    }

    /// Fans `event` out to the signals the way `HouseState::apply` patches its fields.
    pub fn apply(&self, event: WsEvent) {
        match event {
            WsEvent::Snapshot(s) => {
                self.connection.set(s.connection);
                self.bridge.set(s.bridge);
                self.areas.set(s.areas);
                self.devices.set(s.devices);
                self.entities
                    .set(s.entities.into_iter().map(|e| (e.id.clone(), e)).collect());
                self.ignored.set(s.ignored);
            }
            WsEvent::EntityUpserted(view) => {
                self.entities.update(|m| {
                    m.insert(view.id.clone(), view);
                });
            }
            WsEvent::EntityRemoved { id } => {
                self.entities.update(|m| {
                    m.remove(&id);
                });
            }
            WsEvent::StateChanged { id, state } => {
                self.entities.update(|m| {
                    if let Some(entity) = m.get_mut(&id) {
                        entity.state = state;
                    }
                });
            }
            WsEvent::AreasChanged(areas) => self.areas.set(areas),
            WsEvent::DevicesChanged(devices) => self.devices.set(devices),
            WsEvent::BridgeChanged(bridge) => self.bridge.set(bridge),
            WsEvent::ConnectionChanged(connection) => self.connection.set(connection),
            WsEvent::CommandFailed { id, reason } => self.command_failed.set(Some((id, reason))),
        }
    }
}

impl Default for House {
    fn default() -> Self {
        Self::new()
    }
}
