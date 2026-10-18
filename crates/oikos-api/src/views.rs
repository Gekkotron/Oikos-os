//! Read-only views of the registry's entities, devices, areas, bridge and ignored topics.

use oikos_core::model::{
    Area, Availability, BridgeInfo, Capabilities, Device, Entity, EntityCategory, EntityState,
    Kind, StateValue, UiOverride,
};
use oikos_core::registry::Ignored;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// An entity as the UI shows it, with its current state.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct EntityView {
    pub id: String,
    pub device_id: Option<String>,
    /// The effective name: the override's when set and not blank, else the discovered one.
    pub name: String,
    pub device_name: Option<String>,
    pub label: Option<String>,
    pub kind: Kind,
    pub protocol_kind: Kind,
    pub device_class: Option<String>,
    pub unit: Option<String>,
    pub icon: Option<String>,
    pub category: EntityCategory,
    pub enabled_by_default: bool,
    pub area: String,
    pub part_of: Option<String>,
    pub capabilities: Capabilities,
    pub ui: UiOverride,
    pub state: EntityState,
}

impl EntityView {
    pub fn from(entity: &Entity, state: &EntityState) -> Self {
        let name = match entity.ui.name.as_deref() {
            Some(name) if !name.trim().is_empty() => name.to_owned(),
            _ => entity.name.clone(),
        };
        Self {
            id: entity.unique_id.clone(),
            device_id: entity.device_id.clone(),
            name,
            device_name: entity.device_name.clone(),
            label: entity.label.clone(),
            kind: entity.kind,
            protocol_kind: entity.protocol_kind,
            device_class: entity.device_class.clone(),
            unit: entity.unit.clone(),
            icon: entity.icon.clone(),
            category: entity.category,
            enabled_by_default: entity.enabled_by_default,
            area: entity.area.clone(),
            part_of: entity.part_of.clone(),
            capabilities: entity.capabilities.clone(),
            ui: entity.ui.clone(),
            state: state.clone(),
        }
    }
}

/// A Zigbee device with what its diagnostic entities report.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DeviceView {
    pub id: String,
    pub ieee: String,
    pub friendly_name: String,
    pub model: Option<String>,
    pub vendor: Option<String>,
    pub description: Option<String>,
    pub supported: bool,
    pub interview_state: String,
    pub power_source: Option<String>,
    pub entity_ids: Vec<String>,
    pub availability: Availability,
    pub battery: Option<u8>,
    pub linkquality: Option<u32>,
}

impl DeviceView {
    /// `members` are the device's entities with their states.
    pub fn new(device: &Device, members: &[(&Entity, &EntityState)]) -> Self {
        let any = |wanted: Availability| members.iter().any(|(_, s)| s.availability == wanted);
        let availability = if any(Availability::Online) {
            Availability::Online
        } else if any(Availability::Offline) {
            Availability::Offline
        } else {
            Availability::Unknown
        };
        let number = |suffix: &str| {
            members
                .iter()
                .find_map(|(entity, state)| match state.value {
                    StateValue::Number(n) if entity.unique_id.ends_with(suffix) => Some(n),
                    _ => None,
                })
        };
        Self {
            id: device.id.clone(),
            ieee: device.ieee.clone(),
            friendly_name: device.friendly_name.clone(),
            model: device.model.clone(),
            vendor: device.vendor.clone(),
            description: device.description.clone(),
            supported: device.supported,
            interview_state: device.interview_state.clone(),
            power_source: device.power_source.clone(),
            entity_ids: members.iter().map(|(e, _)| e.unique_id.clone()).collect(),
            availability,
            // `as` saturates, so the clamp only needs to bound the top.
            battery: number("_battery_zigbee2mqtt").map(|n| n.clamp(0.0, 100.0) as u8),
            linkquality: number("_linkquality_zigbee2mqtt").map(|n| n as u32),
        }
    }
}

/// A room with the number of entities in it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AreaView {
    pub id: String,
    pub name: String,
    pub icon: Option<String>,
    pub order: Option<i32>,
    pub entity_count: usize,
}

impl AreaView {
    pub fn new(area: &Area, entity_count: usize) -> Self {
        Self {
            id: area.id.clone(),
            name: area.name.clone(),
            icon: area.icon.clone(),
            order: area.order,
            entity_count,
        }
    }
}

/// The Zigbee2MQTT bridge.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct BridgeView {
    pub version: Option<String>,
    pub coordinator: Option<String>,
    pub permit_join: bool,
    pub state: Availability,
}

impl From<&BridgeInfo> for BridgeView {
    fn from(bridge: &BridgeInfo) -> Self {
        Self {
            version: bridge.version.clone(),
            coordinator: bridge.coordinator.clone(),
            permit_join: bridge.permit_join,
            state: bridge.state,
        }
    }
}

/// A discovery topic that did not become an entity.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct IgnoredView {
    pub topic: String,
    pub reason: String,
}

impl From<&Ignored> for IgnoredView {
    fn from(ignored: &Ignored) -> Self {
        Self {
            topic: ignored.topic.clone(),
            reason: ignored.reason.clone(),
        }
    }
}
