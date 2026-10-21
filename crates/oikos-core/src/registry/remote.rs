//! Folding the `device_automation` configs of one device into a single remote entity
//! (`device_automation.<node>`) whose `actions` are the per-config triggers.

use serde_json::{Map, Value, json};

use super::{Change, Registry, as_discovered};
use crate::discovery::{DeviceAutomationFragment, capabilities, naming};
use crate::model::{Bindings, Entity, EntityCategory, Kind, UNASSIGNED_AREA, UiOverride};
use crate::types::GenericCaps;

const COMPONENT: &str = "device_automation";

impl Registry {
    /// Adds (or updates) one action on the remote of its device, creating the remote on the
    /// first action. `origin` is the discovery topic that carried the action.
    pub(super) fn upsert_device_automation(
        &mut self,
        fragment: DeviceAutomationFragment,
        origin: &str,
        changes: &mut Vec<Change>,
    ) {
        let remote_id = format!("{COMPONENT}.{}", fragment.node);
        // The topic now describes another device: take the action off the old remote first.
        if self
            .device_automation_actions
            .get(origin)
            .is_some_and(|(id, _)| *id != remote_id)
        {
            self.remove_device_automation(origin, changes);
        }
        let action = json!({
            "object": fragment.object,
            "topic": fragment.topic,
            "payload": fragment.payload,
            "type": fragment.action_type,
            "subtype": fragment.action_subtype,
        });
        let mut entity = match self.entities.get(&remote_id) {
            Some(existing) => existing.clone(),
            None => new_remote(&remote_id, &fragment, origin),
        };
        let mut actions = actions_of(&entity);
        splice(&mut actions, action);
        set_actions(&mut entity, actions);
        self.device_automation_actions.insert(
            origin.to_owned(),
            (remote_id.clone(), fragment.object.clone()),
        );
        self.store_remote(entity, changes);
    }

    /// Takes the action carried by `origin` off its remote; the last one removes the remote.
    /// Does nothing when `origin` carried no action.
    pub(super) fn remove_device_automation(&mut self, origin: &str, changes: &mut Vec<Change>) {
        let Some((remote_id, object)) = self.device_automation_actions.remove(origin) else {
            return;
        };
        let Some(mut entity) = self.entities.get(&remote_id).cloned() else {
            return;
        };
        let mut actions = actions_of(&entity);
        actions.retain(|a| a.get("object").and_then(Value::as_str) != Some(object.as_str()));
        if actions.is_empty() {
            self.remove_entity(&remote_id, changes);
            return;
        }
        set_actions(&mut entity, actions);
        self.store_remote(entity, changes);
    }

    /// Stores the remote unless it is the same as discovered before.
    fn store_remote(&mut self, entity: Entity, changes: &mut Vec<Change>) {
        if let Some(previous) = self.entities.get(&entity.unique_id)
            && as_discovered(previous) == as_discovered(&entity)
        {
            return;
        }
        self.store(entity, changes);
    }
}

fn actions_of(entity: &Entity) -> Vec<Value> {
    entity
        .bindings
        .config
        .get("actions")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

/// Puts `action` in place of the one with the same object id or payload, else at the end,
/// then drops any other action with the same payload.
fn splice(actions: &mut Vec<Value>, action: Value) {
    let key = |a: &Value, k: &str| a.get(k).and_then(Value::as_str).map(str::to_owned);
    let (object, payload) = (key(&action, "object"), key(&action, "payload"));
    match actions
        .iter()
        .position(|a| key(a, "object") == object)
        .or_else(|| actions.iter().position(|a| key(a, "payload") == payload))
    {
        Some(at) => actions[at] = action,
        None => actions.push(action),
    }
    let mut seen = Vec::new();
    actions.retain(|a| {
        let p = key(a, "payload");
        if seen.contains(&p) {
            return false;
        }
        seen.push(p);
        true
    });
}

/// Writes `actions` into the config and refreshes the signals (the payloads, in order).
fn set_actions(entity: &mut Entity, actions: Vec<Value>) {
    let signals: Vec<String> = actions
        .iter()
        .filter_map(|a| a.get("payload").and_then(Value::as_str).map(str::to_owned))
        .collect();
    if let Some(config) = entity.bindings.config.as_object_mut() {
        config.insert("actions".to_owned(), Value::Array(actions));
    }
    let generic = entity
        .capabilities
        .generic
        .get_or_insert_with(GenericCaps::default);
    generic.signals = signals;
}

fn new_remote(id: &str, fragment: &DeviceAutomationFragment, origin: &str) -> Entity {
    let device = fragment.device.as_object();
    let device_id = device
        .and_then(|d| d.get("identifiers"))
        .and_then(|v| match v {
            Value::String(s) => Some(s.clone()),
            Value::Array(items) => items.iter().find_map(Value::as_str).map(str::to_owned),
            _ => None,
        });
    let raw_name = device
        .and_then(|d| d.get("name"))
        .and_then(Value::as_str)
        .or(device_id.as_deref());
    let (mut name, device_name, label, path_area) = naming::split(raw_name, None);
    if name.is_empty() {
        name = fragment.node.clone();
    }
    let suggested_area = device
        .and_then(|d| d.get("suggested_area"))
        .and_then(Value::as_str)
        .map(str::to_owned);
    let mut config = Map::new();
    config.insert("device".to_owned(), fragment.device.clone());
    config.insert("actions".to_owned(), Value::Array(Vec::new()));
    let value = Value::Object(config);
    let mut capabilities = capabilities::from_config(Kind::Generic, &value);
    capabilities.generic = Some(GenericCaps::default());
    Entity {
        unique_id: id.to_owned(),
        device_id,
        name,
        device_name,
        label,
        kind: Kind::Generic,
        protocol_kind: Kind::Generic,
        component: COMPONENT.to_owned(),
        device_class: None,
        unit: None,
        icon: None,
        category: EntityCategory::Primary,
        enabled_by_default: true,
        capabilities,
        bindings: Bindings { config: value },
        suggested_area,
        path_area,
        area: UNASSIGNED_AREA.to_owned(),
        ui: UiOverride::default(),
        part_of: None,
        discovery_topic: origin.to_owned(),
    }
}
