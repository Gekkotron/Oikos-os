//! WebSocket events and REST bodies.

use oikos_core::model::EntityState;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::alerts::AlertView;
use crate::snapshot::Snapshot;
use crate::views::{AreaView, BridgeView, DeviceView, EntityView};

/// The hub's link to the MQTT broker.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ConnectionStatus {
    #[default]
    Connecting,
    Online,
    Offline {
        reason: String,
    },
}

/// One message pushed to a WebSocket client.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
#[allow(
    clippy::large_enum_variant,
    reason = "the variants are the wire shape; an event is built and serialized once"
)]
pub enum WsEvent {
    Snapshot(Snapshot),
    EntityUpserted(EntityView),
    EntityRemoved { id: String },
    StateChanged { id: String, state: EntityState },
    AreasChanged { areas: Vec<AreaView> },
    DevicesChanged { devices: Vec<DeviceView> },
    AlertsChanged { alerts: Vec<AlertView> },
    BridgeChanged(BridgeView),
    ConnectionChanged(ConnectionStatus),
    CommandFailed { id: String, reason: String },
}

/// A partial override: the keys to set (`null` clears one).
pub type OverridePatch = serde_json::Map<String, serde_json::Value>;

/// The reply to a command request.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CommandResponse {
    pub accepted: bool,
    pub reason: Option<String>,
    pub state: Option<EntityState>,
}

/// The body of an error reply.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ApiError {
    pub error: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn every_list_event_serializes() {
        let cases = [
            (
                WsEvent::AreasChanged { areas: vec![] },
                json!({"type": "areas_changed", "areas": []}),
            ),
            (
                WsEvent::DevicesChanged { devices: vec![] },
                json!({"type": "devices_changed", "devices": []}),
            ),
            (
                WsEvent::AlertsChanged { alerts: vec![] },
                json!({"type": "alerts_changed", "alerts": []}),
            ),
        ];
        for (event, expected) in cases {
            let value = serde_json::to_value(&event).expect("serializes");
            assert_eq!(value, expected);
            let back: WsEvent = serde_json::from_value(value).expect("round-trips");
            assert_eq!(back, event);
        }
    }
}
