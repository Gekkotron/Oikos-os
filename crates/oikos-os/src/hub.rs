//! The hub: the registry behind a lock, the event broadcast, the connection status and the
//! three writers (commands, overrides, areas). The MQTT task feeds it through [`Hub::ingest`]
//! and drains its publishes from the channel given to [`Hub::new`]; the HTTP layer reads it.

use std::collections::BTreeSet;
use std::sync::{Arc, Mutex, RwLock, RwLockReadGuard, RwLockWriteGuard};
use std::time::Duration;

use oikos_api::{
    AreaView, CommandResponse, ConnectionStatus, EntityView, IgnoredView, OverridePatch, Snapshot,
    WsEvent, area_views, device_views, snapshot,
};
use oikos_core::command::{CommandError, Publish, map_command};
use oikos_core::model::{AreasDocument, Availability, Command, PatchError, UiOverride};
use oikos_core::registry::{Change, Inbound, Registry, RegistryConfig};
use tokio::sync::{broadcast, mpsc, watch};

use crate::clock;
use crate::config::Config;

/// Events kept for a slow WebSocket client before it starts missing some.
const EVENT_CAPACITY: usize = 1024;

/// Why a request to the hub failed.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum HubError {
    #[error("unknown entity `{0}`")]
    UnknownEntity(String),
    /// The entity cannot take the request right now; nothing was published.
    #[error("rejected: {0}")]
    Rejected(String),
    #[error(transparent)]
    Patch(#[from] PatchError),
    #[error(transparent)]
    Command(#[from] CommandError),
    /// The publish could not be handed to the MQTT task.
    #[error("publish: {0}")]
    Publish(String),
}

/// What [`Hub::entity_detail`] returns: the view plus what the view leaves out.
#[derive(Clone, Debug, PartialEq)]
pub struct EntityDetail {
    pub view: EntityView,
    /// The expanded discovery config the entity was built from.
    pub bindings: serde_json::Value,
    /// The override retained on `<ui_prefix>/<id>`, verbatim.
    pub raw_override: Option<UiOverride>,
}

pub struct Hub {
    registry: RwLock<Registry>,
    events: broadcast::Sender<WsEvent>,
    /// The ids that received a state message, changed or not: Zigbee2MQTT echoes the whole
    /// device state on every `/set`, so a no-op command is confirmed by its echo.
    state_seen: broadcast::Sender<String>,
    connection: watch::Sender<ConnectionStatus>,
    publisher: mpsc::Sender<Publish>,
    ui_prefix: String,
    confirm_timeout: Duration,
    subscription_debounce: Duration,
    /// Entities whose template failure was already logged, so each failure is logged once.
    logged_failures: Mutex<BTreeSet<String>>,
}

impl Hub {
    pub fn new(cfg: &Config, publisher: mpsc::Sender<Publish>) -> Arc<Hub> {
        let registry = Registry::new(RegistryConfig {
            discovery_prefixes: cfg.topics.discovery_prefixes.clone(),
            ui_prefix: cfg.topics.ui_prefix.clone(),
            z2m_base: cfg.topics.z2m_base.clone(),
            reserved: RegistryConfig::default_reserved(),
        });
        let (events, _) = broadcast::channel(EVENT_CAPACITY);
        let (state_seen, _) = broadcast::channel(EVENT_CAPACITY);
        let (connection, _) = watch::channel(ConnectionStatus::Connecting);
        Arc::new(Hub {
            registry: RwLock::new(registry),
            events,
            state_seen,
            connection,
            publisher,
            ui_prefix: cfg.topics.ui_prefix.clone(),
            confirm_timeout: Duration::from_millis(cfg.commands.confirm_timeout_ms),
            subscription_debounce: Duration::from_millis(cfg.commands.subscription_debounce_ms),
            logged_failures: Mutex::new(BTreeSet::new()),
        })
    }

    /// Routes and applies one broker message, broadcasts the resulting events and reports the
    /// changes (the MQTT task watches for [`Change::SubscriptionsChanged`]).
    pub fn ingest(&self, topic: &str, payload: &[u8]) -> Vec<Change> {
        let at = clock::now();
        let (changes, events, failing, seen) = {
            let mut reg = self.write();
            let inbound = reg.route(topic, payload.to_vec(), at);
            let seen = match &inbound {
                Inbound::State { topic, .. } => reg.entities_on_topic(topic).to_vec(),
                _ => Vec::new(),
            };
            let changes = reg.apply(inbound);
            let events = events_for(&reg, &changes);
            (changes, events, reg.template_failures().clone(), seen)
        };
        self.log_new_template_failures(&failing);
        for event in events {
            self.broadcast(event);
        }
        for id in seen {
            // An error only means no command is waiting right now.
            let _ = self.state_seen.send(id);
        }
        changes
    }

    pub fn snapshot(&self) -> Snapshot {
        let connection = self.connection.borrow().clone();
        snapshot(&self.read(), connection, clock::now())
    }

    pub fn subscribe(&self) -> broadcast::Receiver<WsEvent> {
        self.events.subscribe()
    }

    pub fn connection(&self) -> watch::Receiver<ConnectionStatus> {
        self.connection.subscribe()
    }

    /// Records the broker link's status and broadcasts it when it changed.
    pub fn set_connection(&self, status: ConnectionStatus) {
        let changed = self.connection.send_if_modified(|current| {
            if *current == status {
                false
            } else {
                *current = status.clone();
                true
            }
        });
        if changed {
            self.broadcast(WsEvent::ConnectionChanged(status));
        }
    }

    /// Every topic some entity reads.
    pub fn subscription_topics(&self) -> BTreeSet<String> {
        self.read().subscription_topics().clone()
    }

    /// How long the MQTT task waits after a subscription change before diffing.
    pub fn subscription_debounce(&self) -> Duration {
        self.subscription_debounce
    }

    /// Entity and device counts, for logging.
    pub fn counts(&self) -> (usize, usize) {
        let reg = self.read();
        (reg.entities().count(), reg.devices().count())
    }

    /// The topics no entity could be made from, with the reason.
    pub fn ignored(&self) -> Vec<IgnoredView> {
        self.read()
            .ignored()
            .iter()
            .map(IgnoredView::from)
            .collect()
    }

    pub fn entity_view(&self, id: &str) -> Option<EntityView> {
        let reg = self.read();
        let entity = reg.entity(id)?;
        let state = reg.state(id).cloned().unwrap_or_default();
        Some(EntityView::from(entity, &state))
    }

    /// The entity's view with its expanded discovery config (topics, templates, payload
    /// constants) and the override as retained, `None` when none is retained for it.
    pub fn entity_detail(&self, id: &str) -> Option<EntityDetail> {
        let reg = self.read();
        let entity = reg.entity(id)?;
        let state = reg.state(id).cloned().unwrap_or_default();
        Some(EntityDetail {
            view: EntityView::from(entity, &state),
            bindings: entity.bindings.config.clone(),
            raw_override: reg.raw_override(id).cloned(),
        })
    }

    /// Maps `cmd`, publishes it and waits up to `confirm_timeout_ms` for the entity's state to
    /// change or for any state message addressed to it (a no-op command is echoed unchanged).
    /// An unknown or offline entity is rejected before anything is published.
    pub async fn send_command(&self, id: &str, cmd: Command) -> Result<CommandResponse, HubError> {
        // Subscribed before publishing, so a fast echo cannot slip between the two.
        let mut events = self.subscribe();
        let mut seen = self.state_seen.subscribe();
        let publishes = {
            let reg = self.read();
            let entity = reg
                .entity(id)
                .ok_or_else(|| HubError::UnknownEntity(id.to_owned()))?;
            let state = reg.state(id).cloned().unwrap_or_default();
            if state.availability == Availability::Offline {
                return Err(HubError::Rejected("entity is offline".to_owned()));
            }
            map_command(entity, &state, &cmd, reg.templates())?
        };
        for publish in publishes {
            self.publish(publish)?;
        }

        let confirmed = tokio::time::timeout(self.confirm_timeout, async {
            loop {
                tokio::select! {
                    event = events.recv() => match event {
                        Ok(WsEvent::StateChanged { id: changed, state }) if changed == id => {
                            return Some(state);
                        }
                        Ok(_) | Err(broadcast::error::RecvError::Lagged(_)) => {}
                        Err(broadcast::error::RecvError::Closed) => return None,
                    },
                    seen_id = seen.recv() => match seen_id {
                        Ok(seen_id) if seen_id == id => {
                            return Some(self.read().state(id).cloned().unwrap_or_default());
                        }
                        Ok(_) | Err(broadcast::error::RecvError::Lagged(_)) => {}
                        Err(broadcast::error::RecvError::Closed) => return None,
                    },
                }
            }
        })
        .await
        .ok()
        .flatten();

        match confirmed {
            Some(state) => Ok(CommandResponse {
                accepted: true,
                reason: None,
                state: Some(state),
            }),
            None => {
                let reason = format!(
                    "no state change within {} ms",
                    self.confirm_timeout.as_millis()
                );
                self.broadcast(WsEvent::CommandFailed {
                    id: id.to_owned(),
                    reason: reason.clone(),
                });
                Ok(CommandResponse {
                    accepted: false,
                    reason: Some(reason),
                    state: None,
                })
            }
        }
    }

    /// Patches the entity's override, publishes it retained and applies it locally so the
    /// returned view already reflects it (the broker's echo is then a no-op).
    pub async fn set_override(
        &self,
        id: &str,
        patch: &OverridePatch,
    ) -> Result<EntityView, HubError> {
        let current = {
            let reg = self.read();
            if reg.entity(id).is_none() {
                return Err(HubError::UnknownEntity(id.to_owned()));
            }
            reg.raw_override(id).cloned().unwrap_or_default()
        };
        let next = current.apply_patch(patch)?;
        self.write_override(id, &next)
    }

    /// Removes the entity's override (an empty retained payload).
    pub async fn clear_override(&self, id: &str) -> Result<EntityView, HubError> {
        if self.read().entity(id).is_none() {
            return Err(HubError::UnknownEntity(id.to_owned()));
        }
        self.write_override(id, &UiOverride::default())
    }

    /// Replaces the declared areas: publishes the document retained and applies it locally.
    pub async fn set_areas(&self, doc: AreasDocument) -> Result<Vec<AreaView>, HubError> {
        let topic = format!("{}/_areas", self.ui_prefix);
        let payload = doc.encode();
        self.publish(Publish {
            topic: topic.clone(),
            payload: payload.clone(),
            retain: true,
        })?;
        self.ingest(&topic, payload.as_bytes());
        Ok(area_views(&self.read()))
    }

    fn write_override(&self, id: &str, next: &UiOverride) -> Result<EntityView, HubError> {
        let topic = format!("{}/{id}", self.ui_prefix);
        let payload = if next.is_default() {
            String::new()
        } else {
            next.encode()
        };
        self.publish(Publish {
            topic: topic.clone(),
            payload: payload.clone(),
            retain: true,
        })?;
        self.ingest(&topic, payload.as_bytes());
        self.entity_view(id)
            .ok_or_else(|| HubError::UnknownEntity(id.to_owned()))
    }

    /// Hands a publish to the MQTT task without waiting: a full queue (the broker is away and
    /// 64 publishes are already pending) is an error for the caller, not a hang.
    fn publish(&self, publish: Publish) -> Result<(), HubError> {
        self.publisher.try_send(publish).map_err(|err| match err {
            mpsc::error::TrySendError::Full(_) => {
                HubError::Publish("publish queue is full".to_owned())
            }
            mpsc::error::TrySendError::Closed(_) => {
                HubError::Publish("the MQTT task is gone".to_owned())
            }
        })
    }

    fn broadcast(&self, event: WsEvent) {
        // An error only means nobody is listening right now.
        let _ = self.events.send(event);
    }

    /// Warns once per failing entity; an entity that recovers and fails again is warned again.
    fn log_new_template_failures(&self, failing: &BTreeSet<String>) {
        let mut logged = self
            .logged_failures
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        logged.retain(|id| failing.contains(id));
        for id in failing {
            if logged.insert(id.clone()) {
                tracing::warn!(entity = %id, "template failure");
            }
        }
    }

    /// A poisoned lock means a thread panicked while holding it; the registry never panics on
    /// a hostile payload, so the state is taken as it is and the fact is logged.
    fn read(&self) -> RwLockReadGuard<'_, Registry> {
        self.registry.read().unwrap_or_else(|poisoned| {
            tracing::error!("registry lock was poisoned; continuing with its last state");
            poisoned.into_inner()
        })
    }

    fn write(&self) -> RwLockWriteGuard<'_, Registry> {
        self.registry.write().unwrap_or_else(|poisoned| {
            tracing::error!("registry lock was poisoned; continuing with its last state");
            poisoned.into_inner()
        })
    }
}

/// The WebSocket events for `changes`, built from the registry right after they were applied.
/// `IgnoredChanged` and `SubscriptionsChanged` are internal and produce none.
fn events_for(reg: &Registry, changes: &[Change]) -> Vec<WsEvent> {
    changes
        .iter()
        .filter_map(|change| match change {
            Change::EntityUpserted(id) => reg.entity(id).map(|entity| {
                let state = reg.state(id).cloned().unwrap_or_default();
                WsEvent::EntityUpserted(EntityView::from(entity, &state))
            }),
            Change::EntityRemoved(id) => Some(WsEvent::EntityRemoved { id: id.clone() }),
            Change::StateChanged(id) => reg.state(id).map(|state| WsEvent::StateChanged {
                id: id.clone(),
                state: state.clone(),
            }),
            Change::AreasChanged => Some(WsEvent::AreasChanged(area_views(reg))),
            Change::DevicesChanged => Some(WsEvent::DevicesChanged(device_views(reg))),
            Change::BridgeChanged => Some(WsEvent::BridgeChanged(reg.bridge().into())),
            Change::IgnoredChanged | Change::SubscriptionsChanged => None,
        })
        .collect()
}
