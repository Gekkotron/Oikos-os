//! The registry: the pure state machine behind the hub. It takes broker messages classified
//! by [`Registry::route`], applies them with [`Registry::apply`], and exposes the effective
//! entities, their states, the devices, the areas and the bridge status.
//!
//! No I/O, no clock, no logging: the caller passes `at` with every state message and reports
//! what [`Registry::template_failures`] holds.

mod areas;
mod folding;
mod ignored;
mod routing;

use std::collections::{BTreeMap, BTreeSet};

use crate::discovery::DiscoveryTopic;
use crate::model::{
    Area, AreasDocument, BridgeInfo, Device, Entity, EntityState, Kind, StateValue,
    UNASSIGNED_AREA, UiOverride,
};
use crate::state::{Applied, StateError, apply_message};
use crate::template::Templates;

pub use ignored::Ignored;

/// Where the registry listens.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RegistryConfig {
    /// Home Assistant discovery prefixes (`oikos`, `homeassistant`).
    pub discovery_prefixes: Vec<String>,
    /// The sub-tree holding overrides (`<ui_prefix>/<unique_id>`) and areas (`<ui_prefix>/_areas`).
    pub ui_prefix: String,
    /// The Zigbee2MQTT base topic (`zigbee2mqtt`), for `bridge/{devices,info,state}`.
    pub z2m_base: String,
    /// Sub-trees directly under each discovery prefix that are never discovery.
    pub reserved: Vec<String>,
}

impl RegistryConfig {
    /// The reserved sub-trees under a discovery prefix: `ui` and `automation`.
    pub fn default_reserved() -> Vec<String> {
        vec!["ui".to_owned(), "automation".to_owned()]
    }
}

impl Default for RegistryConfig {
    fn default() -> Self {
        Self {
            discovery_prefixes: vec!["oikos".to_owned()],
            ui_prefix: "oikos/ui".to_owned(),
            z2m_base: "zigbee2mqtt".to_owned(),
            reserved: Self::default_reserved(),
        }
    }
}

/// A broker message, classified for [`Registry::apply`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Inbound {
    Discovery(DiscoveryTopic, Vec<u8>),
    Override {
        unique_id: String,
        payload: Vec<u8>,
    },
    Areas(Vec<u8>),
    Z2mDevices(Vec<u8>),
    Z2mBridgeInfo(Vec<u8>),
    Z2mBridgeState(Vec<u8>),
    State {
        topic: String,
        payload: Vec<u8>,
        at: u64,
    },
    Irrelevant,
}

/// What an [`Registry::apply`] changed, deduplicated and in this order.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Change {
    EntityUpserted(String),
    EntityRemoved(String),
    StateChanged(String),
    AreasChanged,
    DevicesChanged,
    BridgeChanged,
    IgnoredChanged,
    SubscriptionsChanged,
}

/// The house as the registry knows it.
#[derive(Debug)]
pub struct Registry {
    config: RegistryConfig,
    templates: Templates,
    entities: BTreeMap<String, Entity>,
    states: BTreeMap<String, EntityState>,
    /// Overrides by unique id, kept even before the entity is discovered.
    overrides: BTreeMap<String, UiOverride>,
    areas: AreasDocument,
    devices: Vec<Device>,
    bridge: BridgeInfo,
    ignored: ignored::IgnoredSet,
    /// Subscribed topic → entities reading it, sorted.
    topic_index: BTreeMap<String, Vec<String>>,
    subscriptions: BTreeSet<String>,
    /// Last payload per discovery topic and per override id, to skip re-deliveries.
    last_discovery: BTreeMap<String, Vec<u8>>,
    last_override: BTreeMap<String, Vec<u8>>,
    /// Discovery topic → the entities it produced, and the reverse.
    produced: BTreeMap<String, Vec<String>>,
    origin: BTreeMap<String, String>,
    template_failures: BTreeSet<String>,
}

impl Registry {
    pub fn new(config: RegistryConfig) -> Self {
        Self {
            config,
            templates: Templates::new(),
            entities: BTreeMap::new(),
            states: BTreeMap::new(),
            overrides: BTreeMap::new(),
            areas: AreasDocument::default(),
            devices: Vec::new(),
            bridge: BridgeInfo::default(),
            ignored: ignored::IgnoredSet::default(),
            topic_index: BTreeMap::new(),
            subscriptions: BTreeSet::new(),
            last_discovery: BTreeMap::new(),
            last_override: BTreeMap::new(),
            produced: BTreeMap::new(),
            origin: BTreeMap::new(),
            template_failures: BTreeSet::new(),
        }
    }

    pub fn config(&self) -> &RegistryConfig {
        &self.config
    }

    /// The template engine the registry renders with, for command mapping.
    pub fn templates(&self) -> &Templates {
        &self.templates
    }

    /// Classifies a message: `<ui_prefix>/_areas` → [`Inbound::Areas`], `<ui_prefix>/<id>` →
    /// [`Inbound::Override`], `<z2m>/bridge/{devices,info,state}`, a discovery topic, a topic
    /// in [`subscription_topics`](Self::subscription_topics) → [`Inbound::State`] with `at`,
    /// else [`Inbound::Irrelevant`].
    pub fn route(&self, topic: &str, payload: Vec<u8>, at: u64) -> Inbound {
        routing::route(self, topic, payload, at)
    }

    /// Applies one message and reports what changed. Nothing here panics on a hostile payload.
    pub fn apply(&mut self, inbound: Inbound) -> Vec<Change> {
        let mut changes = Vec::new();
        match inbound {
            Inbound::Discovery(topic, payload) => {
                let before = self.subscriptions.clone();
                routing::apply_discovery(self, &topic, payload, &mut changes);
                self.recompute(&mut changes);
                if self.subscriptions != before {
                    changes.push(Change::SubscriptionsChanged);
                }
            }
            Inbound::Override { unique_id, payload } => {
                self.apply_override(unique_id, payload, &mut changes);
            }
            Inbound::Areas(payload) => self.apply_areas(&payload, &mut changes),
            Inbound::Z2mDevices(payload) => {
                if let Ok(devices) = Device::parse_list(&payload)
                    && devices != self.devices
                {
                    self.devices = devices;
                    changes.push(Change::DevicesChanged);
                }
            }
            Inbound::Z2mBridgeInfo(payload) => {
                let mut bridge = self.bridge.clone();
                bridge.apply_info(&payload);
                self.replace_bridge(bridge, &mut changes);
            }
            Inbound::Z2mBridgeState(payload) => self.apply_bridge_state(&payload, &mut changes),
            Inbound::State { topic, payload, at } => {
                if topic == format!("{}/bridge/state", self.config.z2m_base) {
                    self.apply_bridge_state(&payload, &mut changes);
                }
                self.apply_state(&topic, &payload, at, &mut changes);
            }
            Inbound::Irrelevant => {}
        }
        changes.sort();
        changes.dedup();
        changes
    }

    pub fn entity(&self, unique_id: &str) -> Option<&Entity> {
        self.entities.get(unique_id)
    }

    /// Every entity, by unique id.
    pub fn entities(&self) -> impl Iterator<Item = &Entity> {
        self.entities.values()
    }

    pub fn state(&self, unique_id: &str) -> Option<&EntityState> {
        self.states.get(unique_id)
    }

    pub fn devices(&self) -> impl Iterator<Item = &Device> {
        self.devices.iter()
    }

    pub fn device(&self, id: &str) -> Option<&Device> {
        self.devices.iter().find(|device| device.id == id)
    }

    /// The declared areas, as retained on `<ui_prefix>/_areas`.
    pub fn areas_document(&self) -> &AreasDocument {
        &self.areas
    }

    /// Declared areas by `order` then name, then the implicit ones carried by entities (named
    /// by their id), then `Unassigned` when any entity has it.
    pub fn effective_areas(&self) -> Vec<Area> {
        areas::effective_list(&self.areas.areas, self.entities.values())
    }

    pub fn bridge(&self) -> &BridgeInfo {
        &self.bridge
    }

    /// Discovery topics that did not become entities, by topic.
    pub fn ignored(&self) -> &[Ignored] {
        self.ignored.as_slice()
    }

    /// Every topic some entity reads: the set to subscribe to.
    pub fn subscription_topics(&self) -> &BTreeSet<String> {
        &self.subscriptions
    }

    /// The override retained for `unique_id`, whether or not the entity exists.
    pub fn raw_override(&self, unique_id: &str) -> Option<&UiOverride> {
        self.overrides.get(unique_id)
    }

    /// The entities reading `topic`, sorted by unique id.
    pub fn entities_on_topic(&self, topic: &str) -> &[String] {
        self.topic_index.get(topic).map_or(&[], Vec::as_slice)
    }

    /// Entities whose template failed on their last message; their value is `Unknown` until a
    /// message maps again. The caller logs each id once.
    pub fn template_failures(&self) -> &BTreeSet<String> {
        &self.template_failures
    }

    fn apply_override(&mut self, unique_id: String, payload: Vec<u8>, changes: &mut Vec<Change>) {
        if self.last_override.get(&unique_id) == Some(&payload) {
            return;
        }
        if payload.is_empty() {
            self.overrides.remove(&unique_id);
            self.last_override.remove(&unique_id);
        } else {
            let Some(ui) = UiOverride::decode(&payload) else {
                return;
            };
            self.overrides.insert(unique_id.clone(), ui);
            self.last_override.insert(unique_id, payload);
        }
        self.recompute(changes);
    }

    fn apply_areas(&mut self, payload: &[u8], changes: &mut Vec<Change>) {
        let document = if payload.is_empty() {
            AreasDocument::default()
        } else {
            let Some(document) = AreasDocument::decode(payload) else {
                return;
            };
            document
        };
        if document == self.areas {
            return;
        }
        self.areas = document;
        changes.push(Change::AreasChanged);
        self.recompute(changes);
    }

    fn apply_bridge_state(&mut self, payload: &[u8], changes: &mut Vec<Change>) {
        let mut bridge = self.bridge.clone();
        bridge.apply_state(payload);
        self.replace_bridge(bridge, changes);
    }

    fn replace_bridge(&mut self, bridge: BridgeInfo, changes: &mut Vec<Change>) {
        if bridge != self.bridge {
            self.bridge = bridge;
            changes.push(Change::BridgeChanged);
        }
    }

    /// Hands the message to every entity reading `topic`. A template failure leaves the value
    /// `Unknown` and lists the entity in `template_failures`; a later successful message clears it.
    fn apply_state(&mut self, topic: &str, payload: &[u8], at: u64, changes: &mut Vec<Change>) {
        let Ok(text) = std::str::from_utf8(payload) else {
            return;
        };
        let Some(ids) = self.topic_index.get(topic) else {
            return;
        };
        for id in ids {
            let (Some(entity), Some(state)) = (self.entities.get(id), self.states.get_mut(id))
            else {
                continue;
            };
            match apply_message(entity, state, topic, text, at, &self.templates) {
                Ok(Applied::Changed) => {
                    self.template_failures.remove(id);
                    changes.push(Change::StateChanged(id.clone()));
                }
                Ok(Applied::Unchanged | Applied::NotForThisEntity) => {}
                Err(StateError::Template(_)) => {
                    let had_value = state.value != StateValue::Unknown;
                    state.value = StateValue::Unknown;
                    self.template_failures.insert(id.clone());
                    if had_value {
                        changes.push(Change::StateChanged(id.clone()));
                    }
                }
            }
        }
    }

    /// Stores a freshly parsed entity produced by the discovery topic `origin`, replacing any
    /// previous one with the same unique id. Its state, if any, is kept; a re-parse that
    /// yields the same entity is not a change.
    fn upsert(&mut self, entity: Entity, origin: &str, changes: &mut Vec<Change>) {
        let id = entity.unique_id.clone();
        if let Some(previous_origin) = self.origin.insert(id.clone(), origin.to_owned())
            && previous_origin != origin
            && let Some(ids) = self.produced.get_mut(&previous_origin)
        {
            ids.retain(|other| *other != id);
        }
        if let Some(previous) = self.entities.get(&id) {
            if as_discovered(previous) == entity {
                return;
            }
            let previous = previous.clone();
            self.unindex(&id, &previous);
        }
        self.index(&id, &entity);
        self.states.entry(id.clone()).or_default();
        self.entities.insert(id.clone(), entity);
        changes.push(Change::EntityUpserted(id));
    }

    fn remove_entity(&mut self, id: &str, changes: &mut Vec<Change>) {
        let Some(entity) = self.entities.remove(id) else {
            return;
        };
        self.unindex(id, &entity);
        self.states.remove(id);
        self.origin.remove(id);
        self.template_failures.remove(id);
        changes.push(Change::EntityRemoved(id.to_owned()));
    }

    fn index(&mut self, id: &str, entity: &Entity) {
        for topic in entity.bindings.topics() {
            let ids = self.topic_index.entry(topic.clone()).or_default();
            if let Err(at) = ids.binary_search_by(|other| other.as_str().cmp(id)) {
                ids.insert(at, id.to_owned());
            }
            self.subscriptions.insert(topic);
        }
    }

    fn unindex(&mut self, id: &str, entity: &Entity) {
        for topic in entity.bindings.topics() {
            let Some(ids) = self.topic_index.get_mut(&topic) else {
                continue;
            };
            ids.retain(|other| other != id);
            if ids.is_empty() {
                self.topic_index.remove(&topic);
                self.subscriptions.remove(&topic);
            }
        }
    }

    /// Recomputes every entity's override, area, kind and `part_of`, reporting those that
    /// changed. Cheap at the house's scale (hundreds of entities) and always correct.
    fn recompute(&mut self, changes: &mut Vec<Change>) {
        let declared = &self.areas.areas;
        for (id, entity) in &mut self.entities {
            let ui = self.overrides.get(id).cloned().unwrap_or_default();
            let area = areas::effective(entity, ui.area.as_deref(), declared);
            let kind = if entity.protocol_kind == Kind::Switch && ui.as_light {
                Kind::Light
            } else {
                entity.protocol_kind
            };
            if entity.ui != ui || entity.area != area || entity.kind != kind {
                entity.ui = ui;
                entity.area = area;
                entity.kind = kind;
                changes.push(Change::EntityUpserted(id.clone()));
            }
        }
        let part_of = folding::compute(&self.entities);
        for (id, entity) in &mut self.entities {
            let owner = part_of.get(id).cloned();
            if entity.part_of != owner {
                entity.part_of = owner;
                changes.push(Change::EntityUpserted(id.clone()));
            }
        }
    }
}

/// `entity` with the fields the registry computes reset to what discovery produces, so it
/// compares equal to a fresh parse of the same config.
fn as_discovered(entity: &Entity) -> Entity {
    Entity {
        ui: UiOverride::default(),
        area: UNASSIGNED_AREA.to_owned(),
        kind: entity.protocol_kind,
        part_of: None,
        ..entity.clone()
    }
}
