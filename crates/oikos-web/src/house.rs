//! The reactive house: one signal per slice of [`crate::house_state::HouseState`].

use std::collections::BTreeMap;

use leptos::prelude::*;
use oikos_api::{
    AlertView, AreaView, BridgeView, ConnectionStatus, DeviceView, EntityView, FlowView,
    FlowsConfigView, GroupView, IgnoredView, WsEvent,
};

#[derive(Clone, Copy)]
pub struct House {
    pub connection: RwSignal<ConnectionStatus>,
    pub bridge: RwSignal<BridgeView>,
    pub areas: RwSignal<Vec<AreaView>>,
    pub devices: RwSignal<Vec<DeviceView>>,
    pub entities: RwSignal<BTreeMap<String, EntityView>>,
    pub ignored: RwSignal<Vec<IgnoredView>>,
    /// The push alerts, newest first.
    pub alerts: RwSignal<Vec<AlertView>>,
    /// The flows with their recent runs.
    pub flows: RwSignal<Vec<FlowView>>,
    /// The hub's `[flows]` settings; `None` until the first fetch answers.
    pub flows_config: RwSignal<Option<FlowsConfigView>>,
    /// The hand-made groups by id (without the `group_` prefix), for the member counts on Home.
    pub groups: RwSignal<BTreeMap<String, GroupView>>,
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
            alerts: RwSignal::new(Vec::new()),
            flows: RwSignal::new(Vec::new()),
            flows_config: RwSignal::new(None),
            groups: RwSignal::new(BTreeMap::new()),
            command_failed: RwSignal::new(None),
        }
    }

    /// Fetches the groups into [`House::groups`]; a failure leaves the last value (the `×N`
    /// badge simply stays hidden).
    pub fn refresh_groups(&self) {
        let groups = self.groups;
        leptos::task::spawn_local(async move {
            if let Ok(list) = crate::api::list_groups().await {
                groups.set(list.into_iter().map(|g| (g.id.clone(), g)).collect());
            }
        });
    }

    /// Fetches the `[flows]` settings into [`House::flows_config`]; a failure leaves the last
    /// value (the badge simply stays hidden).
    pub fn refresh_flows_config(&self) {
        let flows_config = self.flows_config;
        leptos::task::spawn_local(async move {
            if let Ok(cfg) = crate::api::get_flows_config().await {
                flows_config.set(Some(cfg));
            }
        });
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
                self.alerts.set(s.alerts);
                self.flows.set(s.flows);
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
            WsEvent::AreasChanged { areas } => self.areas.set(areas),
            WsEvent::DevicesChanged { devices } => self.devices.set(devices),
            WsEvent::BridgeChanged(bridge) => self.bridge.set(bridge),
            WsEvent::ConnectionChanged(connection) => self.connection.set(connection),
            WsEvent::AlertsChanged { alerts } => self.alerts.set(alerts),
            WsEvent::FlowsChanged { flows } => self.flows.set(flows),
            WsEvent::FlowRunChanged { flow_id, run } => {
                self.flows
                    .update(|flows| crate::house_state::upsert_run(flows, &flow_id, run));
            }
            WsEvent::CommandFailed { id, reason } => self.command_failed.set(Some((id, reason))),
            // A press is an event for the flow engine; the UI has nothing to show for it.
            WsEvent::ButtonPressed { .. } => {}
        }
    }
}

impl Default for House {
    fn default() -> Self {
        Self::new()
    }
}
