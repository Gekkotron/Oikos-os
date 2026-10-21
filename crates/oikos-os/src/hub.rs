//! The hub: the registry behind a lock, the event broadcast, the connection status and the
//! three writers (commands, overrides, areas). The MQTT task feeds it through [`Hub::ingest`]
//! and drains its publishes from the channel given to [`Hub::new`]; the HTTP layer reads it.

use std::collections::BTreeSet;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, RwLock, RwLockReadGuard, RwLockWriteGuard};
use std::time::Duration;

use crate::alerts::{AlertStore, Applied};
use crate::flows::FlowStore;
use crate::history::History;
use crate::media::MediaProxy;
use crate::pictures::{MAX_PICTURE_BYTES, Picture, Pictures};
use crate::virtual_entities as virtuals;
use oikos_api::{
    AlertView, AreaView, CommandResponse, ConnectionStatus, EntityView, IgnoredView, OverridePatch,
    Snapshot, VirtualSpec, WsEvent, area_views, device_views, mask_secret, snapshot,
};
use oikos_core::command::{CommandError, Publish, map_command};
use oikos_core::model::{
    AreasDocument, Availability, Command, Entity, GroupKind, Kind, PatchError, StateValue,
    UNASSIGNED_AREA, UiOverride,
};
use oikos_core::registry::{Change, Ignored, Inbound, Registry, RegistryConfig};
use oikos_core::types::command::expects_echo;
use tokio::sync::{broadcast, mpsc, watch};

use crate::clock;
use crate::config::Config;
pub use crate::flows::EngineRequest;

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
    /// The publish was refused (the broker link is down) or could not be handed to the MQTT
    /// task; nothing was applied locally.
    #[error("publish: {0}")]
    Publish(String),
    #[error("unknown alert `{0}`")]
    UnknownAlert(String),
    #[error("unknown action `{0}`")]
    UnknownAction(String),
    #[error("unknown flow `{0}`")]
    UnknownFlow(String),
    #[error("unknown area `{0}`")]
    UnknownArea(String),
    /// The flow cannot be saved or run: it does not validate, or names something the house
    /// does not have.
    #[error("invalid flow: {0}")]
    InvalidFlow(String),
}

/// The `install_id` the web sends in a reply; the app sends its own install id.
const WEB_INSTALL_ID: &str = "oikos-web";

/// The reply the app's `ReplyWorker` publishes, field order included.
#[derive(serde::Serialize)]
struct Reply<'a> {
    id: &'a str,
    action: &'a str,
    install_id: &'a str,
    ts: u64,
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

/// A report on a state topic of `id`, with the link epoch it was ingested in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StateSeen {
    pub id: String,
    /// [`Hub::link_epoch`] when the report arrived; a report from before the latest link drop
    /// is stale and must not seed anything.
    pub epoch: u64,
}

pub struct Hub {
    registry: RwLock<Registry>,
    events: broadcast::Sender<WsEvent>,
    /// The ids whose state was reported, changed or not: Zigbee2MQTT echoes the whole device
    /// state on every `/set`, so a no-op command is confirmed by its echo.
    state_seen: broadcast::Sender<StateSeen>,
    /// Woken when the set of topics to subscribe to may have changed outside a broker message
    /// (a local flow write); the MQTT task diffs its subscriptions on it.
    pub(crate) subscriptions_changed: Arc<tokio::sync::Notify>,
    /// Counts the times the broker link left `Online`.
    link_epoch: AtomicU64,
    connection: watch::Sender<ConnectionStatus>,
    publisher: mpsc::Sender<Publish>,
    ui_prefix: String,
    /// The first discovery prefix: virtual entities are published under it.
    discovery_prefix: String,
    /// OikosPush's retained history topic and its decoded copy.
    history_topic: String,
    responses_topic: String,
    alerts: Mutex<AlertStore>,
    /// The retained flow definitions and their runs.
    flow_store: Mutex<FlowStore>,
    /// The topic prefix flows are retained under, one topic per flow id.
    automation_prefix: String,
    /// The time zone schedules and their preview run in.
    zone: chrono_tz::Tz,
    /// The house (latitude, longitude), Paris when `[flows]` sets none.
    house_point: (f64, f64),
    /// Whether `house_point` is the Paris fallback, latched at startup.
    fallback_active: bool,
    /// The `[flows]` section as configured.
    flows_config: crate::config::FlowsConfig,
    /// Requests for the flow engine; the engine takes the receiving end once.
    engine_tx: mpsc::UnboundedSender<EngineRequest>,
    engine_rx: Mutex<Option<mpsc::UnboundedReceiver<EngineRequest>>>,
    /// Frigate media served through the hub; `None` without a `[frigate] url`.
    media: Option<Arc<MediaProxy>>,
    /// The latest picture of each camera or image entity.
    pictures: Arc<Pictures>,
    confirm_timeout: Duration,
    subscription_debounce: Duration,
    /// Entities whose template failure was already logged, so each failure is logged once.
    logged_failures: Mutex<BTreeSet<String>>,
    /// Ignored discovery topics (with their reason) already logged, so each is logged once.
    logged_ignored: Mutex<BTreeSet<(String, String)>>,
    /// Set once at startup when `[history] url` is configured; receives one [`Incoming`]
    /// per `Change::StateChanged` emitted by [`Self::ingest`].
    history: OnceLock<Arc<History>>,
}

impl Hub {
    /// The house time zone, `[flows] timezone`.
    pub fn zone(&self) -> chrono_tz::Tz {
        self.zone
    }

    /// The house (latitude, longitude), `[flows] latitude`/`longitude` or Paris.
    pub fn house_point(&self) -> (f64, f64) {
        self.house_point
    }

    /// The `[flows]` settings as the UI reads them.
    pub fn flows_config_view(&self) -> oikos_api::FlowsConfigView {
        oikos_api::FlowsConfigView {
            latitude: self.flows_config.latitude,
            longitude: self.flows_config.longitude,
            timezone: self.zone.name().to_owned(),
            fallback_active: self.fallback_active,
        }
    }

    /// True when `house_point` is the Paris fallback.
    pub fn fallback_active(&self) -> bool {
        self.fallback_active
    }

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
        let media = MediaProxy::new(cfg.frigate.clone());
        let alerts = AlertStore::with_media_origin(media.as_ref().map(|m| m.origin().to_owned()));
        let (engine_tx, engine_rx) = mpsc::unbounded_channel();
        Arc::new(Hub {
            registry: RwLock::new(registry),
            events,
            state_seen,
            subscriptions_changed: Arc::new(tokio::sync::Notify::new()),
            link_epoch: AtomicU64::new(0),
            connection,
            publisher,
            ui_prefix: cfg.topics.ui_prefix.clone(),
            discovery_prefix: cfg
                .topics
                .discovery_prefixes
                .first()
                .cloned()
                .unwrap_or_else(|| "oikos".to_owned()),
            history_topic: cfg.push.history_topic.clone(),
            responses_topic: cfg.push.responses_topic.clone(),
            alerts: Mutex::new(alerts),
            flow_store: Mutex::new(FlowStore::new(cfg.automation.runs_kept)),
            automation_prefix: cfg.topics.automation_prefix.clone(),
            zone: crate::flows::house_zone(&cfg.flows),
            house_point: cfg.flows.house_point(),
            fallback_active: cfg.flows.fallback_active(),
            flows_config: cfg.flows.clone(),
            engine_tx,
            engine_rx: Mutex::new(Some(engine_rx)),
            media,
            pictures: Arc::new(Pictures::new()),
            confirm_timeout: Duration::from_millis(cfg.commands.confirm_timeout_ms),
            subscription_debounce: Duration::from_millis(cfg.commands.subscription_debounce_ms),
            logged_failures: Mutex::new(BTreeSet::new()),
            logged_ignored: Mutex::new(BTreeSet::new()),
            history: OnceLock::new(),
        })
    }

    /// Hands the recorder to the hub; the second call is a no-op.
    pub fn set_history(&self, history: Arc<History>) {
        let _ = self.history.set(history);
    }

    /// The recorder, when one is configured.
    pub fn history(&self) -> Option<&Arc<History>> {
        self.history.get()
    }

    /// Routes and applies one broker message, broadcasts the resulting events and reports the
    /// changes (the MQTT task watches for [`Change::SubscriptionsChanged`]).
    pub fn ingest(&self, topic: &str, payload: &[u8]) -> Vec<Change> {
        if topic == self.history_topic {
            self.ingest_history(topic, payload);
            return Vec::new();
        }
        if let Some(id) = self.flow_id_of(topic) {
            // A flow may start or stop watching a button, so its command topic may change.
            return if self.ingest_flow(id, payload) {
                vec![Change::SubscriptionsChanged]
            } else {
                Vec::new()
            };
        }
        let at = clock::now();
        let at_micros = clock::now_micros();
        let history_feed_enabled = self.history.get().is_some();
        let (changes, events, failing, seen, ignored, history_feed) = {
            let mut reg = self.write();
            let inbound = reg.route(topic, payload.to_vec(), at);
            let is_state = matches!(inbound, Inbound::State { .. });
            let is_override = matches!(inbound, Inbound::Override { .. });
            let changes = reg.apply(inbound);
            let events = events_for(&reg, &changes);
            let ignored = changes
                .contains(&Change::IgnoredChanged)
                .then(|| reg.ignored().to_vec());
            // The registry drops an override it cannot decode without a trace; say so here.
            if is_override && !payload.is_empty() && UiOverride::decode(payload).is_none() {
                tracing::warn!(%topic, "invalid override payload ignored");
            }
            // A message on a topic that only carries an entity's availability says nothing
            // about its state, so it does not count as the entity speaking.
            let seen: Vec<String> = if is_state {
                changes
                    .iter()
                    .filter_map(|change| match change {
                        Change::StateChanged(id) | Change::StateReported(id) => Some(id),
                        _ => None,
                    })
                    .filter(|id| {
                        reg.entity(id)
                            .is_some_and(|entity| !entity.bindings.is_availability_only(topic))
                    })
                    .cloned()
                    .collect()
            } else {
                Vec::new()
            };
            let history_feed: Vec<(String, oikos_core::model::EntityState, Option<f64>)> =
                if history_feed_enabled {
                    changes
                        .iter()
                        .filter_map(|change| match change {
                            Change::StateChanged(id) => {
                                let entity = reg.entity(id)?;
                                // A secret is never offered to history.
                                if entity
                                    .capabilities
                                    .generic
                                    .as_ref()
                                    .is_some_and(|g| g.secret)
                                {
                                    return None;
                                }
                                let state = reg.state(id)?;
                                let num = history_num_of(entity, &state.value);
                                Some((id.clone(), state.clone(), num))
                            }
                            _ => None,
                        })
                        .collect()
                } else {
                    Vec::new()
                };
            (
                changes,
                events,
                reg.template_failures().clone(),
                seen,
                ignored,
                history_feed,
            )
        };
        if let Some(history) = self.history.get() {
            for (id, state, num) in history_feed {
                history.offer(oikos_core::history::Incoming {
                    at: at_micros,
                    id,
                    state,
                    num,
                });
            }
        }
        self.log_new_template_failures(&failing);
        if let Some(ignored) = ignored {
            self.log_new_ignored(&ignored);
        }
        for event in events {
            self.broadcast(event);
        }
        for id in seen {
            // An error only means no command is waiting right now.
            let epoch = self.link_epoch();
            let _ = self.state_seen.send(StateSeen { id, epoch });
        }
        self.with_button_subscriptions(changes)
    }

    /// [`Self::ingest`] for a message that carries the broker's retain flag. A press is an
    /// event: a non-retained message on the command topic of a button some flow watches
    /// broadcasts [`WsEvent::ButtonPressed`]. A retained one is a replay, never a press.
    pub fn ingest_message(&self, topic: &str, payload: &[u8], retain: bool) -> Vec<Change> {
        self.capture_pictures(topic, payload);
        if !retain {
            let pressed = self.watched_button_topics().remove(topic);
            if let Some(ids) = pressed {
                for id in ids {
                    self.broadcast(WsEvent::ButtonPressed { id });
                }
                return Vec::new();
            }
            for (id, signal) in self.signals_of(topic, payload) {
                self.record_signal(&id, &signal);
                self.broadcast(WsEvent::SignalReceived { id, signal });
            }
        }
        self.ingest(topic, payload)
    }

    /// The in-memory pictures the route serves.
    pub fn pictures(&self) -> &Arc<Pictures> {
        &self.pictures
    }

    /// Keeps what a message says about the picture of the camera and image entities reading
    /// `topic`: the bytes (decoded from base64 when `image_encoding` says so) or a URL.
    fn capture_pictures(&self, topic: &str, payload: &[u8]) {
        use base64::Engine as _;
        use oikos_core::types::{Shape, type_def};
        let mut found = Vec::new();
        {
            let reg = self.read();
            for id in reg.entities_on_topic(topic) {
                let Some(entity) = reg.entity(id) else {
                    continue;
                };
                let Some(field) = type_def(&entity.component)
                    .filter(|def| def.primary.as_deref() == Some("picture"))
                    .and_then(|def| def.fields.get("picture"))
                    .filter(|field| field.shape == Shape::Picture)
                else {
                    continue;
                };
                let cfg = &entity.bindings.config;
                let is = |key: &str| cfg.get(key).and_then(|v| v.as_str()) == Some(topic);
                if is(&field.topic) {
                    let body = if cfg.get("image_encoding").and_then(|v| v.as_str()) == Some("b64")
                    {
                        let text = String::from_utf8_lossy(payload);
                        match base64::engine::general_purpose::STANDARD.decode(text.trim()) {
                            Ok(body) => body,
                            Err(err) => {
                                tracing::warn!(%id, %err, "picture is not valid base64");
                                continue;
                            }
                        }
                    } else {
                        payload.to_vec()
                    };
                    if body.len() > MAX_PICTURE_BYTES {
                        tracing::warn!(%id, size = body.len(), "picture over 5 MiB dropped");
                        continue;
                    }
                    let content_type = cfg
                        .get("content_type")
                        .and_then(|v| v.as_str())
                        .unwrap_or("image/jpeg")
                        .to_owned();
                    found.push((
                        id.clone(),
                        Picture::Bytes {
                            content_type,
                            body: body.into(),
                        },
                    ));
                } else if is("url_topic") {
                    let text = String::from_utf8_lossy(payload);
                    let url = match cfg.get("url_template").and_then(|v| v.as_str()) {
                        Some(template) => match reg.templates().render(template, &text) {
                            Ok(url) => url,
                            Err(err) => {
                                tracing::warn!(%id, %err, "picture url template failed");
                                continue;
                            }
                        },
                        None => text.into_owned(),
                    };
                    let url = url.trim().to_owned();
                    if !url.is_empty() {
                        found.push((id.clone(), Picture::Url(url)));
                    }
                }
            }
        }
        for (id, picture) in found {
            self.pictures.put(&id, picture);
        }
    }

    /// Stores the signal as the entity's `last` and `at` (ms) and announces the state change,
    /// so the sheet shows it and a repeated signal still counts.
    fn record_signal(&self, id: &str, signal: &str) {
        let at_ms = clock::now_micros().max(0) as u64 / 1000;
        let events = {
            let mut reg = self.write();
            let changes = reg.record_signal(id, signal, at_ms);
            events_for(&reg, &changes)
        };
        for event in events {
            self.broadcast(event);
        }
    }

    /// The signals a message carries: one per signal-placed entity (remote, event, tag) it
    /// belongs to.
    fn signals_of(&self, topic: &str, payload: &[u8]) -> Vec<(String, String)> {
        use oikos_core::types::{Placement, signals::signal_of, type_def};
        let reg = self.read();
        reg.entities_on_topic(topic)
            .iter()
            .filter_map(|id| reg.entity(id))
            .filter_map(|entity| {
                let def =
                    type_def(&entity.component).filter(|d| d.placement == Placement::Signal)?;
                let signal = signal_of(entity, def, topic, payload, reg.templates())?;
                Some((entity.unique_id.clone(), signal))
            })
            .collect()
    }

    /// A button appearing or any entity vanishing may change the button topics to subscribe to.
    fn with_button_subscriptions(&self, mut changes: Vec<Change>) -> Vec<Change> {
        if changes.contains(&Change::SubscriptionsChanged) {
            return changes;
        }
        let reg = self.read();
        let touches_button = changes.iter().any(|change| match change {
            Change::EntityUpserted(id) => reg
                .entity(id)
                .is_some_and(|entity| entity.kind == oikos_core::model::Kind::Button),
            Change::EntityRemoved(_) => true,
            _ => false,
        });
        drop(reg);
        if touches_button {
            changes.push(Change::SubscriptionsChanged);
        }
        changes
    }

    pub fn snapshot(&self) -> Snapshot {
        let connection = self.connection.borrow().clone();
        let mut snapshot = snapshot(&self.read(), connection, clock::now());
        snapshot.alerts = self.alerts();
        snapshot.flows = self.flows();
        snapshot
    }

    /// The Frigate media proxy, when `[frigate] url` is configured.
    pub fn media(&self) -> Option<Arc<MediaProxy>> {
        self.media.clone()
    }

    /// The push alerts, newest first as OikosPush published them.
    pub fn alerts(&self) -> Vec<AlertView> {
        self.alerts
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .list()
            .to_vec()
    }

    /// Publishes the reply the app would publish when the user taps `action` on alert `id`.
    pub fn reply(&self, id: &str, action: &str) -> Result<(), HubError> {
        {
            let alerts = self
                .alerts
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let alert = alerts
                .find(id)
                .ok_or_else(|| HubError::UnknownAlert(id.to_owned()))?;
            if !alert.actions.iter().any(|a| a.id == action) {
                return Err(HubError::UnknownAction(action.to_owned()));
            }
        }
        let payload = serde_json::to_string(&Reply {
            id,
            action,
            install_id: WEB_INSTALL_ID,
            ts: clock::now(),
        })
        .map_err(|err| HubError::Publish(err.to_string()))?;
        self.publish(Publish {
            topic: self.responses_topic.clone(),
            payload,
            retain: false,
        })
    }

    /// A history payload never touches the registry: it is decoded into the alert store and,
    /// when the list changed, broadcast. A payload that is not an array is logged once.
    fn ingest_history(&self, topic: &str, payload: &[u8]) {
        let applied = self
            .alerts
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .apply(payload);
        match applied {
            Applied::Changed(alerts) => self.broadcast(WsEvent::AlertsChanged { alerts }),
            Applied::Unchanged => {}
            Applied::Invalid { first_time: true } => {
                tracing::warn!(%topic, "invalid history payload ignored");
            }
            Applied::Invalid { first_time: false } => {}
        }
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
            if status != ConnectionStatus::Online {
                self.link_epoch.fetch_add(1, Ordering::SeqCst);
            }
            self.broadcast(WsEvent::ConnectionChanged(status));
        }
    }

    /// Every topic some entity reads.
    pub fn subscription_topics(&self) -> BTreeSet<String> {
        let mut topics = self.read().subscription_topics().clone();
        topics.extend(self.watched_button_topics().into_keys());
        topics
    }

    /// The wake-up the MQTT task waits on to re-diff its subscriptions.
    pub fn subscriptions_wakeup(&self) -> Arc<tokio::sync::Notify> {
        Arc::clone(&self.subscriptions_changed)
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

    /// How many ignored discovery topics are currently logged as such (invalid payloads, not
    /// ignored components); for tests and diagnostics.
    pub fn logged_ignored_len(&self) -> usize {
        self.logged_ignored
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .len()
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

    /// How many times the broker link left `Online`.
    pub fn link_epoch(&self) -> u64 {
        self.link_epoch.load(Ordering::SeqCst)
    }

    /// A receiver of every id whose state topic was reported, changed or not. The flow engine
    /// uses it to re-seed an entity whose retained replay was identical to its last state,
    /// which the registry (it only emits real changes) does not announce.
    pub fn subscribe_state_seen(&self) -> broadcast::Receiver<StateSeen> {
        self.state_seen.subscribe()
    }

    /// Maps `cmd`, publishes it and waits up to `confirm_timeout_ms` for the entity's value to
    /// change or for a report on one of its state topics (a no-op command is echoed
    /// unchanged). An availability message alone confirms nothing. An unknown or offline
    /// entity is rejected before anything is published, and a broker link that is down is
    /// [`HubError::Publish`] right away rather than a wait for an echo that cannot come.
    pub async fn send_command(&self, id: &str, cmd: Command) -> Result<CommandResponse, HubError> {
        // Subscribed before publishing, so a fast echo cannot slip between the two.
        let mut events = self.subscribe();
        let mut seen = self.state_seen.subscribe();
        let (publishes, before, no_echo_expected) = {
            let reg = self.read();
            let entity = reg
                .entity(id)
                .ok_or_else(|| HubError::UnknownEntity(id.to_owned()))?;
            let state = reg.state(id).cloned().unwrap_or_default();
            if state.availability == Availability::Offline {
                return Err(HubError::Rejected("entity is offline".to_owned()));
            }
            // A generic control with no state topic to echo on (a press, a scene) is done
            // once published.
            let no_echo_expected = match &cmd {
                Command::Control { control, .. } if entity.kind == Kind::Generic => {
                    oikos_core::types::type_def(&entity.component)
                        .is_some_and(|def| !expects_echo(def, entity, control))
                }
                _ => false,
            };
            (
                map_command(entity, &state, &cmd, reg.templates())?,
                state.value,
                no_echo_expected,
            )
        };
        for publish in publishes {
            self.publish(publish)?;
        }
        if no_echo_expected {
            return Ok(CommandResponse {
                accepted: true,
                reason: None,
                state: None,
            });
        }

        let confirmed = tokio::time::timeout(self.confirm_timeout, async {
            loop {
                tokio::select! {
                    event = events.recv() => match event {
                        // Only a value change counts here: an availability flip is a
                        // `StateChanged` too, and an echo with the same value comes through
                        // `state_seen` (which excludes availability-only topics).
                        Ok(WsEvent::StateChanged { id: changed, state })
                            if changed == id && state.value != before =>
                        {
                            return Some(state);
                        }
                        Ok(_) | Err(broadcast::error::RecvError::Lagged(_)) => {}
                        Err(broadcast::error::RecvError::Closed) => return None,
                    },
                    seen_id = seen.recv() => match seen_id {
                        Ok(seen_id) if seen_id.id == id => {
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
            Some(mut state) => {
                if let Some(entity) = self.read().entity(id) {
                    mask_secret(entity, &mut state);
                }
                Ok(CommandResponse {
                    accepted: true,
                    reason: None,
                    state: Some(state),
                })
            }
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
    /// returned view already reflects it (the broker's echo is then a no-op). The `area` is
    /// stored canonically: a declared name becomes its id, a blank string clears the field.
    pub async fn set_override(
        &self,
        id: &str,
        patch: &OverridePatch,
    ) -> Result<EntityView, HubError> {
        let (current, announced) = {
            let reg = self.read();
            let Some(entity) = reg.entity(id) else {
                return Err(HubError::UnknownEntity(id.to_owned()));
            };
            let announced = entity
                .capabilities
                .generic
                .as_ref()
                .map(|g| g.controls.clone())
                .unwrap_or_default();
            (reg.raw_override(id).cloned().unwrap_or_default(), announced)
        };
        let mut next = current.apply_patch(patch)?;
        if let Some(main) = next.main_control.as_deref()
            && main != "none"
            && !announced.iter().any(|c| c == main)
        {
            return Err(HubError::Patch(PatchError::InvalidValue {
                field: "mainControl".to_owned(),
                expected: "one of the entity's announced controls or \"none\"".to_owned(),
            }));
        }
        next.area = next
            .area
            .as_deref()
            .map(str::trim)
            .filter(|area| !area.is_empty())
            .map(|area| self.read().resolve_area(area));
        self.write_override(id, &next)
    }

    /// Removes the entity's override (an empty retained payload).
    pub async fn clear_override(&self, id: &str) -> Result<EntityView, HubError> {
        if self.read().entity(id).is_none() {
            return Err(HubError::UnknownEntity(id.to_owned()));
        }
        self.write_override(id, &UiOverride::default())
    }

    /// The declared areas document as last retained.
    pub fn areas_document(&self) -> AreasDocument {
        self.read().areas_document().clone()
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

    /// Publishes the entity type table retained on `<ui_prefix>/_types`, for the app.
    pub async fn publish_types(&self) -> Result<(), HubError> {
        self.publish(Publish {
            topic: format!("{}/_types", self.ui_prefix),
            payload: oikos_core::types::table_json().to_owned(),
            retain: true,
        })
    }

    /// Renames area `id` in the declared document, the app's other fields kept; an implicit
    /// area (carried only by entities) becomes declared. `Unassigned` cannot be renamed.
    pub async fn rename_area(&self, id: &str, name: &str) -> Result<Vec<AreaView>, HubError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(HubError::Rejected(
                "an area name cannot be blank".to_owned(),
            ));
        }
        if id == UNASSIGNED_AREA {
            return Err(HubError::Rejected(
                "`Unassigned` cannot be renamed".to_owned(),
            ));
        }
        let mut doc = {
            let reg = self.read();
            if !reg.effective_areas().iter().any(|area| area.id == id) {
                return Err(HubError::UnknownArea(id.to_owned()));
            }
            reg.areas_document().clone()
        };
        doc.rename(id, name);
        self.set_areas(doc).await
    }

    /// Edits the `kind` room group card of area `id` in the declared document: `name` (`None`
    /// leaves it, `Some(None)` or a blank clears it) and `favorite`, the rest kept.
    pub async fn set_area_group(
        &self,
        id: &str,
        kind: &str,
        name: Option<Option<String>>,
        favorite: Option<bool>,
    ) -> Result<Vec<AreaView>, HubError> {
        let kind = GroupKind::from_key(kind)
            .ok_or_else(|| HubError::Rejected(format!("`{kind}` is not a group kind")))?;
        if id == UNASSIGNED_AREA {
            return Err(HubError::Rejected(
                "`Unassigned` has no room groups".to_owned(),
            ));
        }
        if name.is_none() && favorite.is_none() {
            return Err(HubError::Rejected(
                "nothing to change: give a `name` or a `favorite`".to_owned(),
            ));
        }
        let mut doc = {
            let reg = self.read();
            if !reg.effective_areas().iter().any(|area| area.id == id) {
                return Err(HubError::UnknownArea(id.to_owned()));
            }
            reg.areas_document().clone()
        };
        doc.set_group(id, kind, name.as_ref().map(Option::as_deref), favorite);
        self.set_areas(doc).await
    }

    /// Creates or replaces virtual entity `id`: publishes its discovery config retained and
    /// applies it locally. The initial state (`OFF`, the number's `min`, or `initial`) is
    /// published retained only when the entity did not exist, so redefining one keeps its
    /// state; a button has none. A changed kind clears the old config topic.
    pub async fn set_virtual(&self, id: &str, spec: VirtualSpec) -> Result<EntityView, HubError> {
        virtuals::validate_id(id).map_err(HubError::Rejected)?;
        let initial = virtuals::initial_state(&spec).map_err(HubError::Rejected)?;
        let unique_id = virtuals::unique_id(id);
        let previous = self
            .read()
            .entity(&unique_id)
            .map(|entity| entity.discovery_topic.clone());

        let config_topic = virtuals::config_topic(&self.discovery_prefix, spec.kind, id);
        let config = virtuals::build_config(&self.discovery_prefix, id, &spec).to_string();
        self.publish(Publish {
            topic: config_topic.clone(),
            payload: config.clone(),
            retain: true,
        })?;
        if let Some(old) = previous.as_deref().filter(|old| {
            *old != config_topic && virtuals::is_config_topic(&self.discovery_prefix, id, old)
        }) {
            self.publish(Publish {
                topic: old.to_owned(),
                payload: String::new(),
                retain: true,
            })?;
        }
        self.ingest(&config_topic, config.as_bytes());

        if let (None, Some(state)) = (previous, initial) {
            let topic = virtuals::state_topic(&self.discovery_prefix, id);
            self.publish(Publish {
                topic: topic.clone(),
                payload: state.clone(),
                retain: true,
            })?;
            self.ingest(&topic, state.as_bytes());
        }
        self.entity_view(&unique_id)
            .ok_or(HubError::UnknownEntity(unique_id))
    }

    /// Removes virtual entity `id`: an empty retained payload on its config topic and on its
    /// state topic, so the broker forgets both. Anything that is not a virtual entity is unknown.
    pub async fn delete_virtual(&self, id: &str) -> Result<(), HubError> {
        let unique_id = virtuals::unique_id(id);
        let config_topic = self
            .read()
            .entity(&unique_id)
            .map(|entity| entity.discovery_topic.clone())
            .filter(|topic| virtuals::is_config_topic(&self.discovery_prefix, id, topic))
            .ok_or(HubError::UnknownEntity(unique_id))?;
        self.publish(Publish {
            topic: config_topic.clone(),
            payload: String::new(),
            retain: true,
        })?;
        self.publish(Publish {
            topic: virtuals::state_topic(&self.discovery_prefix, id),
            payload: String::new(),
            retain: true,
        })?;
        self.ingest(&config_topic, b"");
        Ok(())
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

    /// Hands a publish to the MQTT task without waiting. While the broker link is not
    /// `Online` the publish is refused: a write accepted during an outage would show in the
    /// hub, never reach the broker and vanish at the next restart. A full queue (64 publishes
    /// already pending) is an error for the caller too, not a hang.
    pub(crate) fn publish(&self, publish: Publish) -> Result<(), HubError> {
        if !matches!(*self.connection.borrow(), ConnectionStatus::Online) {
            return Err(HubError::Publish("broker offline".to_owned()));
        }
        self.publisher.try_send(publish).map_err(|err| match err {
            mpsc::error::TrySendError::Full(_) => {
                HubError::Publish("publish queue is full".to_owned())
            }
            mpsc::error::TrySendError::Closed(_) => {
                HubError::Publish("the MQTT task is gone".to_owned())
            }
        })
    }

    pub(crate) fn broadcast(&self, event: WsEvent) {
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

    /// Warns once per ignored discovery topic and reason, leaving out the components the hub
    /// ignores on purpose; a topic that recovers and breaks again is warned again.
    fn log_new_ignored(&self, ignored: &[Ignored]) {
        let mut logged = self
            .logged_ignored
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        logged.retain(|(topic, reason)| {
            ignored
                .iter()
                .any(|i| i.topic == *topic && i.reason == *reason)
        });
        for entry in ignored {
            if entry.reason.starts_with("ignored component") {
                continue;
            }
            if logged.insert((entry.topic.clone(), entry.reason.clone())) {
                tracing::warn!(topic = %entry.topic, reason = %entry.reason, "discovery payload ignored");
            }
        }
    }

    /// The flow store, taken as it is when a thread panicked while holding it.
    pub(crate) fn flow_store(&self) -> MutexGuard<'_, FlowStore> {
        self.flow_store
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Asks the flow engine to do something; nobody listening is fine (no engine started).
    pub(crate) fn send_engine(&self, request: EngineRequest) {
        let _ = self.engine_tx.send(request);
    }

    /// The topic of flow `id`.
    pub(crate) fn flow_topic(&self, id: &str) -> String {
        format!("{}/{id}", self.automation_prefix)
    }

    /// The flow id when `topic` is exactly `<automation_prefix>/<id>`; deeper topics are not flows.
    fn flow_id_of<'a>(&self, topic: &'a str) -> Option<&'a str> {
        let id = topic
            .strip_prefix(self.automation_prefix.as_str())?
            .strip_prefix('/')?;
        (!id.is_empty() && !id.contains('/')).then_some(id)
    }

    /// The receiving end of the engine requests, handed out once.
    pub fn take_engine_requests(&self) -> Option<mpsc::UnboundedReceiver<EngineRequest>> {
        self.engine_rx
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take()
    }

    /// A poisoned lock means a thread panicked while holding it; the registry never panics on
    /// a hostile payload, so the state is taken as it is and the fact is logged.
    pub(crate) fn read(&self) -> RwLockReadGuard<'_, Registry> {
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

/// The number history plots for a state: the type table's mapping for a generic entity,
/// the kind's own for the others.
fn history_num_of(entity: &Entity, value: &StateValue) -> Option<f64> {
    match value {
        StateValue::Generic(gs) => oikos_core::types::type_def(&entity.component)
            .and_then(|def| oikos_core::types::history_num(def, gs)),
        other => oikos_core::history::num_of(other),
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
            Change::StateChanged(id) => reg.state(id).map(|state| {
                let mut state = state.clone();
                if let Some(entity) = reg.entity(id) {
                    mask_secret(entity, &mut state);
                }
                WsEvent::StateChanged {
                    id: id.clone(),
                    state,
                }
            }),
            Change::AreasChanged => Some(WsEvent::AreasChanged {
                areas: area_views(reg),
            }),
            Change::DevicesChanged => Some(WsEvent::DevicesChanged {
                devices: device_views(reg),
            }),
            Change::BridgeChanged => Some(WsEvent::BridgeChanged(reg.bridge().into())),
            // A report that changed nothing is not pushed to clients; it only confirms commands.
            Change::StateReported(_) | Change::IgnoredChanged | Change::SubscriptionsChanged => {
                None
            }
        })
        .collect()
}
