//! The whole house at one instant, sent to a client on connect.

use std::collections::BTreeMap;

use oikos_core::model::EntityState;
use oikos_core::registry::Registry;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::events::ConnectionStatus;
use crate::views::{AreaView, BridgeView, DeviceView, EntityView, IgnoredView};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Snapshot {
    pub connection: ConnectionStatus,
    pub bridge: BridgeView,
    pub areas: Vec<AreaView>,
    pub devices: Vec<DeviceView>,
    pub entities: Vec<EntityView>,
    pub ignored: Vec<IgnoredView>,
    pub generated_at: u64,
}

/// Builds the snapshot of `reg`, stamped `at` (unix seconds).
pub fn snapshot(reg: &Registry, connection: ConnectionStatus, at: u64) -> Snapshot {
    let no_state = EntityState::default();
    let entities: Vec<EntityView> = reg
        .entities()
        .map(|e| EntityView::from(e, reg.state(&e.unique_id).unwrap_or(&no_state)))
        .collect();

    let mut per_area: BTreeMap<&str, usize> = BTreeMap::new();
    for entity in reg.entities() {
        *per_area.entry(entity.area.as_str()).or_default() += 1;
    }
    let areas = reg
        .effective_areas()
        .iter()
        .map(|a| AreaView::new(a, per_area.get(a.id.as_str()).copied().unwrap_or(0)))
        .collect();

    let devices = reg
        .devices()
        .map(|device| {
            let members: Vec<_> = reg
                .entities()
                .filter(|e| e.device_id.as_deref() == Some(device.id.as_str()))
                .map(|e| (e, reg.state(&e.unique_id).unwrap_or(&no_state)))
                .collect();
            DeviceView::new(device, &members)
        })
        .collect();

    Snapshot {
        connection,
        bridge: reg.bridge().into(),
        areas,
        devices,
        entities,
        ignored: reg.ignored().iter().map(IgnoredView::from).collect(),
        generated_at: at,
    }
}
