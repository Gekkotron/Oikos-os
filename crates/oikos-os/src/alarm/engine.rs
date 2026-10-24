//! The alarm engine: turns registry events into state-machine events, runs the machine and
//! publishes the panel state and the persisted runtime. The machine itself is `oikos_alarm`.

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use oikos_alarm::doc::ArmMode;
use oikos_alarm::machine::{Effect, Event, Presence, Source, World, next_deadline, step};
use oikos_api::{CommandResponse, ConnectionStatus, WsEvent};
use oikos_core::flow::Scalar;
use oikos_core::model::{AlarmMode, Command, StateValue};
use tokio::sync::broadcast::{self, error::RecvError};
use tokio::task::JoinHandle;

use super::hub::tracker_presence;
use super::{ALARM_ENTITY_ID, lock};
use crate::hub::{Hub, StateSeen};

/// How long after the broker link comes up the engine waits for the retained states to land.
const ALARM_BOOT_SETTLE: Duration = Duration::from_secs(5);

/// What a raw `<p>/panel/set` payload asks for.
pub(crate) fn parse_panel_command(payload: &[u8]) -> Option<Command> {
    let arm = |mode| Some(Command::Arm { mode, code: None });
    match std::str::from_utf8(payload).ok()?.trim() {
        "ARM_AWAY" => arm(AlarmMode::Away),
        "ARM_HOME" => arm(AlarmMode::Home),
        "ARM_NIGHT" => arm(AlarmMode::Night),
        "DISARM" => Some(Command::Disarm { code: None }),
        _ => None,
    }
}

/// The last value seen of each watched entity, since the link last came up.
#[derive(Default)]
struct Known {
    sensors: HashMap<String, Option<bool>>,
    trackers: HashMap<String, Presence>,
}

impl Hub {
    /// Starts the engine loop. Always spawned: it idles while the alarm is switched off.
    pub fn start_alarm(self: &Arc<Self>) -> JoinHandle<()> {
        // Subscribed now, so nothing that happens during the boot settle is missed.
        let events = self.subscribe();
        let seen = self.subscribe_state_seen();
        let hub = Arc::clone(self);
        tokio::spawn(async move { hub.alarm_loop(events, seen).await })
    }

    /// The engine's clock: Unix seconds, advanced by the monotonic clock from a fixed anchor.
    pub(crate) fn alarm_now(&self) -> u64 {
        let (wall, at) = self.alarm.anchor;
        wall + at.elapsed().as_secs()
    }

    /// Applies `ev` at `now`: runs the machine, performs its effects, publishes the panel and
    /// persists the runtime. A no-op while the alarm is off.
    pub async fn alarm_event(self: &Arc<Self>, ev: Event, now: u64) {
        let _pass = self.alarm.pass.lock().await;
        let settings = self.alarm_settings();
        if !settings.enabled {
            return;
        }
        let on = self.alarm_sensors_on(&settings);
        let (effects, rt, before) = {
            let mut rt = lock(&self.alarm.runtime);
            let before = (rt.state_name(), rt.deadline());
            let effects = step(&mut rt, &settings, &World { on: &on }, ev, now);
            (effects, rt.clone(), before)
        };
        self.run_alarm_effects(effects).await;
        // The runtime goes out before the panel state, so a restart never reads an older one.
        if let Err(err) = self.persist_alarm_runtime(&rt) {
            tracing::warn!(%err, "alarm runtime could not be persisted");
        }
        if let Err(err) = self.publish_alarm_panel(&rt) {
            tracing::warn!(%err, "alarm panel state could not be published");
        }
        if before != (rt.state_name(), rt.deadline()) {
            self.broadcast(WsEvent::AlarmChanged);
        }
    }

    /// What the machine asked for. Sirens and alerts arrive with the effects task; until then
    /// they are only logged.
    pub(crate) async fn run_alarm_effects(self: &Arc<Self>, effects: Vec<Effect>) {
        for effect in effects {
            tracing::debug!(?effect, "alarm effect");
        }
    }

    /// The panel entity's command path (`send_command` and the raw MQTT topic).
    pub(crate) async fn alarm_command(self: &Arc<Self>, cmd: Command) -> CommandResponse {
        let refuse = |reason: &str| CommandResponse {
            accepted: false,
            reason: Some(reason.to_owned()),
            state: None,
        };
        if !self.alarm_settings().enabled {
            return refuse("alarm disabled");
        }
        let ev = match cmd {
            Command::Arm { mode, .. } => {
                let mode = match mode {
                    AlarmMode::Away => ArmMode::Away,
                    AlarmMode::Home => ArmMode::Home,
                    AlarmMode::Night => ArmMode::Night,
                    AlarmMode::Vacation | AlarmMode::CustomBypass => {
                        return refuse("this arm mode is not supported");
                    }
                };
                Event::Arm {
                    mode,
                    source: Source::Command,
                }
            }
            Command::Disarm { .. } => Event::Disarm {
                source: Source::Command,
            },
            _ => return refuse("not an alarm command"),
        };
        self.alarm_event(ev, self.alarm_now()).await;
        CommandResponse {
            accepted: true,
            reason: None,
            state: self.entity_view(ALARM_ENTITY_ID).map(|v| v.state),
        }
    }

    /// The watched sensors that are on in the registry right now.
    fn alarm_sensors_on(&self, settings: &oikos_alarm::doc::AlarmSettings) -> BTreeSet<String> {
        let reg = self.read();
        settings
            .sensors
            .iter()
            .filter(|rule| reg.state(&rule.entity).and_then(|s| sensor_on(&s.value)) == Some(true))
            .map(|rule| rule.entity.clone())
            .collect()
    }

    /// The earliest time a `Tick` is due, while the alarm is on.
    fn alarm_deadline(&self) -> Option<u64> {
        let settings = self.alarm_settings();
        settings
            .enabled
            .then(|| next_deadline(&lock(&self.alarm.runtime), &settings))
            .flatten()
    }

    /// After the settle (boot or reconnect): the engine owns the runtime from here on, the
    /// panel entity is republished in case the broker lost it, and the machine catches up.
    async fn alarm_resume(self: &Arc<Self>) {
        self.alarm.engine_started.store(true, Ordering::SeqCst);
        if !self.alarm_settings().enabled {
            return;
        }
        if let Err(err) = self.sync_alarm_entity() {
            tracing::warn!(%err, "alarm entity could not be republished");
        }
        self.alarm_event(Event::Resume, self.alarm_now()).await;
    }

    async fn alarm_loop(
        self: Arc<Self>,
        mut events: broadcast::Receiver<WsEvent>,
        mut seen: broadcast::Receiver<StateSeen>,
    ) {
        let mut link = self.connection();
        let mut known = Known::default();
        let mut resumed_epoch;
        loop {
            while !matches!(*link.borrow_and_update(), ConnectionStatus::Online) {
                if link.changed().await.is_err() {
                    return;
                }
            }
            resumed_epoch = self.link_epoch();
            tokio::time::sleep(ALARM_BOOT_SETTLE).await;
            // What queued up during the settle is dropped: the seed below reflects the final
            // registry, and a queued replay read against it would look like a change.
            drain(&mut events, &mut seen);
            if self.link_epoch() == resumed_epoch
                && matches!(*link.borrow_and_update(), ConnectionStatus::Online)
            {
                break;
            }
        }
        self.alarm_seed_all(&mut known);
        self.alarm_resume().await;
        // Seeding is allowed only while the link is up and settled: during an outage the
        // registry holds the values from before the drop.
        let mut synced = true;
        let mut resume_at: Option<tokio::time::Instant> = None;
        loop {
            let now = self.alarm_now();
            let deadline = self.alarm_deadline();
            tokio::select! {
                biased;
                event = events.recv() => match event {
                    Ok(WsEvent::StateChanged { id, state }) => {
                        self.alarm_state_changed(&mut known, &id, &state.value).await;
                    }
                    Ok(WsEvent::SignalReceived { id, signal }) => {
                        let remote = self.alarm_settings().remotes.iter().any(|r| r.entity == id);
                        if remote {
                            let ev = Event::Signal { entity: id, signal };
                            self.alarm_event(ev, self.alarm_now()).await;
                        }
                    }
                    // The broker is about to replay retained states: forget what we knew, so a
                    // replay is only recorded, never acted on.
                    Ok(WsEvent::ConnectionChanged(status)) if status != ConnectionStatus::Online => {
                        known = Known::default();
                        resume_at = None;
                        synced = false;
                    }
                    Ok(WsEvent::ConnectionChanged(_)) if self.link_epoch() != resumed_epoch => {
                        resumed_epoch = self.link_epoch();
                        resume_at = Some(tokio::time::Instant::now() + ALARM_BOOT_SETTLE);
                    }
                    // A newly watched entity has no known value yet: its first change would be
                    // taken for a first sighting. (Not while a reconnect is settling: the
                    // registry still holds the value from before the drop.)
                    Ok(WsEvent::AlarmChanged) if synced => self.alarm_seed_all(&mut known),
                    // The deadline is recomputed at the top of every pass.
                    Ok(_) | Err(RecvError::Lagged(_)) => {}
                    Err(RecvError::Closed) => break,
                },
                // After the events, so a change is stored by `StateChanged` first. This only
                // re-seeds an entity whose replay was identical (the registry stays silent).
                report = seen.recv() => match report {
                    Ok(report) if synced => self.alarm_seed(&mut known, &report),
                    Ok(_) => {}
                    Err(RecvError::Lagged(_)) => {}
                    Err(RecvError::Closed) => break,
                },
                () = async {
                    match resume_at {
                        Some(at) => tokio::time::sleep_until(at).await,
                        None => std::future::pending().await,
                    }
                } => {
                    resume_at = None;
                    self.alarm_seed_all(&mut known);
                    self.alarm_resume().await;
                    synced = true;
                }
                () = async {
                    match deadline {
                        Some(due) => {
                            tokio::time::sleep(Duration::from_secs(due.saturating_sub(now))).await;
                        }
                        None => std::future::pending().await,
                    }
                } => {
                    self.alarm_event(Event::Tick, self.alarm_now()).await;
                }
            }
        }
    }

    /// A watched sensor or tracker changed state. Only a change between two known values is an
    /// event: a first sighting, or a value appearing or vanishing, is recorded silently.
    async fn alarm_state_changed(
        self: &Arc<Self>,
        known: &mut Known,
        id: &str,
        value: &StateValue,
    ) {
        let settings = self.alarm_settings();
        if settings.sensors.iter().any(|r| r.entity == id) {
            let next = sensor_on(value);
            let previous = known.sensors.insert(id.to_owned(), next);
            if let Some(Some(previous)) = previous
                && let Some(next) = next
                && previous != next
            {
                let ev = Event::Sensor {
                    entity: id.to_owned(),
                    on: next,
                };
                self.alarm_event(ev, self.alarm_now()).await;
            }
        } else if settings.presence.trackers.iter().any(|t| t == id) {
            let next = tracker_presence(value);
            let previous = known.trackers.insert(id.to_owned(), next);
            if previous.is_some_and(|p| p != next) {
                let ev = Event::Tracker {
                    entity: id.to_owned(),
                    seen: next,
                };
                self.alarm_event(ev, self.alarm_now()).await;
            }
        }
    }

    /// Seeds every watched entity that has no known value yet from the registry.
    fn alarm_seed_all(&self, known: &mut Known) {
        let settings = self.alarm_settings();
        let reg = self.read();
        for rule in &settings.sensors {
            if let Some(state) = reg.state(&rule.entity) {
                known
                    .sensors
                    .entry(rule.entity.clone())
                    .or_insert_with(|| sensor_on(&state.value));
            }
        }
        for id in &settings.presence.trackers {
            if let Some(state) = reg.state(id) {
                known
                    .trackers
                    .entry(id.clone())
                    .or_insert_with(|| tracker_presence(&state.value));
            }
        }
    }

    /// Seeds a watched entity that has no known value yet from the registry, ignoring a report
    /// from before the latest link drop.
    fn alarm_seed(&self, known: &mut Known, report: &StateSeen) {
        if report.epoch != self.link_epoch() {
            return;
        }
        let id = report.id.as_str();
        let settings = self.alarm_settings();
        let reg = self.read();
        let Some(state) = reg.state(id) else {
            return;
        };
        if settings.sensors.iter().any(|r| r.entity == id) {
            known
                .sensors
                .entry(id.to_owned())
                .or_insert_with(|| sensor_on(&state.value));
        } else if settings.presence.trackers.iter().any(|t| t == id) {
            known
                .trackers
                .entry(id.to_owned())
                .or_insert_with(|| tracker_presence(&state.value));
        }
    }
}

/// Throws away everything queued on the two receivers.
fn drain(events: &mut broadcast::Receiver<WsEvent>, seen: &mut broadcast::Receiver<StateSeen>) {
    while !matches!(
        events.try_recv(),
        Err(broadcast::error::TryRecvError::Empty | broadcast::error::TryRecvError::Closed)
    ) {}
    while !matches!(
        seen.try_recv(),
        Err(broadcast::error::TryRecvError::Empty | broadcast::error::TryRecvError::Closed)
    ) {}
}

/// Whether a binary sensor reads on; `None` while its state is unknown.
fn sensor_on(value: &StateValue) -> Option<bool> {
    match Scalar::of_state(value) {
        Some(Scalar::Bool(on)) => Some(on),
        _ => None,
    }
}
