//! The reactive house: one signal per slice of [`crate::house_state::HouseState`].

use std::collections::BTreeMap;

use leptos::prelude::*;
use oikos_api::{
    AlertView, AreaView, BridgeView, ConnectionStatus, Dashboard, DashboardView, DeviceView,
    EntityView, FlowView, FlowsConfigView, GroupView, IgnoredView, WsEvent,
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
    /// The dashboards, fetched once at start-up: the hub does not push their structure.
    pub dashboards: RwSignal<Vec<DashboardView>>,
    /// The last rejected command as `(entity id, reason)`.
    pub command_failed: RwSignal<Option<(String, String)>>,
    /// Unix seconds of the user's last visit to the Alerts tab. The nav badge counts alerts
    /// newer than this. Persisted to `localStorage` by [`crate::alerts::install_alerts_badge`].
    pub last_alerts_seen: RwSignal<u64>,
    /// Bumped on every `ClimateChanged` event; the Climate screen and the nav refetch on it.
    pub climate_rev: RwSignal<u64>,
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
            dashboards: RwSignal::new(Vec::new()),
            command_failed: RwSignal::new(None),
            last_alerts_seen: RwSignal::new(0),
            climate_rev: RwSignal::new(0),
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

    /// The dashboard with matching `id`, read without subscribing.
    pub fn dashboard(&self, id: &str) -> Option<DashboardView> {
        self.dashboards
            .with_untracked(|v| v.iter().find(|d| d.dashboard.id == id).cloned())
    }

    /// Writes a dashboard the hub just saved into the list, replacing the copy with the same
    /// id or appending it. The hub does not push dashboard structure, so without this the
    /// screen would rebuild from the copy fetched at start-up.
    pub fn store_dashboard(&self, saved: Dashboard) {
        self.dashboards.update(
            |v| match v.iter_mut().find(|d| d.dashboard.id == saved.id) {
                Some(view) => view.dashboard = saved,
                None => v.push(DashboardView {
                    dashboard: saved,
                    synthesized: false,
                }),
            },
        );
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
            WsEvent::ButtonPressed { .. } | WsEvent::SignalReceived { .. } => {}
            // The Climate screen reads its own view: tell it to refetch.
            WsEvent::ClimateChanged { .. } => self.climate_rev.update(|r| *r += 1),
        }
    }
}

impl Default for House {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dashboard(id: &str, widgets: usize) -> Dashboard {
        let widget = |i: usize| oikos_api::Widget {
            id: format!("w{i}"),
            kind: oikos_api::WidgetKind::Light,
            span: oikos_api::Span::X1x1,
            config: oikos_api::WidgetConfig::Entity {
                entity_id: "light.a".into(),
                title: None,
            },
        };
        Dashboard {
            id: id.into(),
            name: id.into(),
            order: 1,
            widgets: (0..widgets).map(widget).collect(),
        }
    }

    #[test]
    fn store_dashboard_replaces_the_cached_copy() {
        let house = House::new();
        house.dashboards.set(vec![DashboardView {
            dashboard: dashboard("d", 0),
            synthesized: false,
        }]);
        house.store_dashboard(dashboard("d", 2));
        assert_eq!(house.dashboard("d").unwrap().dashboard.widgets.len(), 2);
        assert_eq!(house.dashboards.get_untracked().len(), 1);
    }

    #[test]
    fn store_dashboard_appends_an_unknown_one() {
        let house = House::new();
        house.store_dashboard(dashboard("d", 1));
        let stored = house.dashboard("d").unwrap();
        assert!(!stored.synthesized);
        assert_eq!(stored.dashboard.widgets.len(), 1);
    }
}
