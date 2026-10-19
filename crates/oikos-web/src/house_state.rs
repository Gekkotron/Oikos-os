//! The pure client-side model of the house: what the hub last told us.

use std::collections::BTreeMap;

use oikos_api::{
    AlertView, AreaView, BridgeView, ConnectionStatus, DeviceView, EntityView, IgnoredView,
    Snapshot, WsEvent,
};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct HouseState {
    pub connection: ConnectionStatus,
    pub bridge: BridgeView,
    pub areas: Vec<AreaView>,
    pub devices: Vec<DeviceView>,
    pub entities: BTreeMap<String, EntityView>,
    pub ignored: Vec<IgnoredView>,
    /// The push alerts, newest first, as the hub sent them.
    pub alerts: Vec<AlertView>,
    pub generated_at: u64,
}

impl HouseState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_snapshot(s: Snapshot) -> Self {
        Self {
            connection: s.connection,
            bridge: s.bridge,
            areas: s.areas,
            devices: s.devices,
            entities: s.entities.into_iter().map(|e| (e.id.clone(), e)).collect(),
            ignored: s.ignored,
            alerts: s.alerts,
            generated_at: s.generated_at,
        }
    }

    /// The entities folded into `parent_id` through `part_of`.
    pub fn part_of_children(&self, parent_id: &str) -> Vec<EntityView> {
        self.entities
            .values()
            .filter(|e| e.part_of.as_deref() == Some(parent_id))
            .cloned()
            .collect()
    }

    /// A snapshot replaces everything; every other event patches one field.
    pub fn apply(&mut self, event: WsEvent) {
        match event {
            WsEvent::Snapshot(s) => *self = Self::from_snapshot(s),
            WsEvent::EntityUpserted(view) => {
                self.entities.insert(view.id.clone(), view);
            }
            WsEvent::EntityRemoved { id } => {
                self.entities.remove(&id);
            }
            WsEvent::StateChanged { id, state } => {
                if let Some(entity) = self.entities.get_mut(&id) {
                    entity.state = state;
                }
            }
            WsEvent::AreasChanged { areas } => self.areas = areas,
            WsEvent::DevicesChanged { devices } => self.devices = devices,
            WsEvent::BridgeChanged(bridge) => self.bridge = bridge,
            WsEvent::ConnectionChanged(connection) => self.connection = connection,
            WsEvent::AlertsChanged { alerts } => self.alerts = alerts,
            WsEvent::CommandFailed { .. } => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oikos_api::{AlertChannel, AlertSeverity, AlertView, AreaView, DeviceView};
    use oikos_core::model::{
        Availability, Capabilities, EntityCategory, EntityState, Kind, StateValue, UiOverride,
    };

    fn entity(id: &str) -> EntityView {
        EntityView {
            id: id.into(),
            device_id: None,
            name: id.into(),
            device_name: None,
            label: None,
            kind: Kind::Light,
            protocol_kind: Kind::Light,
            device_class: None,
            unit: None,
            icon: None,
            category: EntityCategory::Primary,
            enabled_by_default: true,
            area: "salon".into(),
            part_of: None,
            capabilities: Capabilities::default(),
            ui: UiOverride::default(),
            state: EntityState::default(),
        }
    }

    fn area(id: &str) -> AreaView {
        AreaView {
            id: id.into(),
            name: id.into(),
            icon: None,
            order: None,
            entity_count: 0,
            primary_count: 0,
        }
    }

    fn device(id: &str) -> DeviceView {
        DeviceView {
            id: id.into(),
            ieee: "0x1".into(),
            friendly_name: id.into(),
            model: None,
            vendor: None,
            description: None,
            supported: true,
            interview_state: "SUCCESSFUL".into(),
            power_source: None,
            entity_ids: vec![],
            availability: Availability::Online,
            battery: None,
            linkquality: None,
        }
    }

    fn snapshot(ids: &[&str]) -> Snapshot {
        Snapshot {
            connection: ConnectionStatus::Online,
            bridge: BridgeView {
                version: Some("2.0".into()),
                coordinator: None,
                permit_join: true,
                state: Availability::Online,
            },
            areas: vec![area("salon")],
            devices: vec![device("d1")],
            entities: ids.iter().map(|id| entity(id)).collect(),
            ignored: vec![IgnoredView {
                topic: "t".into(),
                reason: "r".into(),
            }],
            alerts: vec![],
            generated_at: 42,
        }
    }

    fn state_with(ids: &[&str]) -> HouseState {
        HouseState::from_snapshot(snapshot(ids))
    }

    #[test]
    fn snapshot_replaces_the_cache() {
        let mut s = state_with(&["old"]);
        s.apply(WsEvent::Snapshot(snapshot(&["new"])));
        assert!(!s.entities.contains_key("old"));
        assert!(s.entities.contains_key("new"));
    }

    #[test]
    fn entity_upserted_patches_one_entity() {
        let mut s = state_with(&["a"]);
        s.apply(WsEvent::EntityUpserted(entity("b")));
        assert_eq!(s.entities.len(), 2);
        assert_eq!(s.entities["b"], entity("b"));
    }

    #[test]
    fn state_changed_updates_only_that_entity_state() {
        let mut s = state_with(&["a", "b"]);
        let mut attributes = serde_json::Map::new();
        attributes.insert("brightness".into(), serde_json::json!(10));
        let next = EntityState {
            value: StateValue::Number(3.0),
            attributes,
            availability: Availability::Offline,
            last_update: Some(99),
        };
        s.apply(WsEvent::StateChanged {
            id: "a".into(),
            state: next.clone(),
        });
        assert_eq!(s.entities["a"].state, next);
        assert_eq!(s.entities["b"].state, EntityState::default());
        assert_eq!(s.entities["a"].name, "a");
    }

    #[test]
    fn areas_changed_replaces_the_chip_list() {
        let mut s = state_with(&[]);
        s.apply(WsEvent::AreasChanged {
            areas: vec![area("b"), area("a")],
        });
        let ids: Vec<_> = s.areas.iter().map(|a| a.id.as_str()).collect();
        assert_eq!(ids, ["b", "a"]);
    }

    #[test]
    fn devices_changed_replaces_the_device_list() {
        let mut s = state_with(&[]);
        s.apply(WsEvent::DevicesChanged {
            devices: vec![device("x")],
        });
        assert_eq!(s.devices, vec![device("x")]);
    }

    #[test]
    fn bridge_changed_updates_bridge_only() {
        let mut s = state_with(&["a"]);
        let before = s.clone();
        let bridge = BridgeView {
            version: None,
            coordinator: Some("c".into()),
            permit_join: false,
            state: Availability::Offline,
        };
        s.apply(WsEvent::BridgeChanged(bridge.clone()));
        assert_eq!(s.bridge, bridge);
        assert_eq!(
            HouseState {
                bridge: before.bridge.clone(),
                ..s
            },
            before
        );
    }

    #[test]
    fn connection_changed_updates_connection_only() {
        let mut s = state_with(&["a"]);
        let before = s.clone();
        let offline = ConnectionStatus::Offline {
            reason: "down".into(),
        };
        s.apply(WsEvent::ConnectionChanged(offline.clone()));
        assert_eq!(s.connection, offline);
        assert_eq!(
            HouseState {
                connection: before.connection.clone(),
                ..s
            },
            before
        );
    }

    #[test]
    fn command_failed_is_a_noop_on_state() {
        let mut s = state_with(&["a"]);
        let before = s.clone();
        s.apply(WsEvent::CommandFailed {
            id: "a".into(),
            reason: "nope".into(),
        });
        assert_eq!(s, before);
    }

    #[test]
    fn alerts_changed_replaces_the_list() {
        let alert = |id: &str| AlertView {
            id: id.into(),
            channel: AlertChannel::Camera,
            severity: AlertSeverity::Info,
            title: id.into(),
            body: String::new(),
            image_url: None,
            video_url: None,
            entity_id: None,
            actions: vec![],
            source: None,
            silent: false,
            resolved: false,
            sent_at: 1,
        };
        let mut s = state_with(&[]);
        s.apply(WsEvent::AlertsChanged {
            alerts: vec![alert("b"), alert("a")],
        });
        let ids: Vec<_> = s.alerts.iter().map(|a| a.id.as_str()).collect();
        assert_eq!(ids, ["b", "a"]);
        s.apply(WsEvent::Snapshot(snapshot(&[])));
        assert!(s.alerts.is_empty());
    }

    #[test]
    fn entity_removed_drops_the_entry() {
        let mut s = state_with(&["a", "b"]);
        s.apply(WsEvent::EntityRemoved { id: "a".into() });
        assert!(!s.entities.contains_key("a"));
        assert!(s.entities.contains_key("b"));
    }

    #[test]
    fn from_snapshot_populates_every_field() {
        let s = state_with(&["a"]);
        assert_eq!(s.connection, ConnectionStatus::Online);
        assert_eq!(s.bridge.version.as_deref(), Some("2.0"));
        assert_eq!(s.areas.len(), 1);
        assert_eq!(s.devices.len(), 1);
        assert_eq!(s.entities.len(), 1);
        assert_eq!(s.ignored.len(), 1);
        assert_eq!(s.generated_at, 42);
    }

    #[test]
    fn snapshot_after_lagging_replaces_stale_entries() {
        let mut s = state_with(&["a", "stale"]);
        s.apply(WsEvent::Snapshot(snapshot(&["a"])));
        assert!(!s.entities.contains_key("stale"));
        assert_eq!(s.entities.len(), 1);
    }

    #[test]
    fn part_of_children_returns_folded_entities() {
        let mut state = HouseState::new();
        state.apply(WsEvent::EntityUpserted(entity("cover.hall")));
        let mut child = entity("cover.hall_moving");
        child.part_of = Some("cover.hall".into());
        state.apply(WsEvent::EntityUpserted(child));
        assert_eq!(state.part_of_children("cover.hall").len(), 1);
        assert_eq!(
            state.part_of_children("cover.hall")[0].id,
            "cover.hall_moving"
        );
        assert!(state.part_of_children("cover.hall_moving").is_empty());
    }
}
