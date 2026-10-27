//! The flow engine: turns state changes and manual requests into runs.
//!
//! Boot and reconnect safety is the point of the `known` map. The broker replays every retained
//! state when the hub (re)connects, and those replays must never start a run: a light switching
//! because the hub restarted is worse than a light that misses one trigger. So a trigger only
//! fires on a change between two values the engine itself has seen, and the map is emptied
//! whenever the link drops or the engine falls behind the event stream. Every report of a
//! watched entity (changed or not) missing from the map seeds it silently again, so an
//! identical retained replay arms the entity without firing.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use chrono::Timelike;

use oikos_api::{RunStatus, StepOutcome, StepView, TriggerInfo, WsEvent};
use oikos_core::flow::sun::is_dark_with_offset;
use oikos_core::flow::template::render;
use oikos_core::flow::{
    Flow, Missing, Node, NodeKind, Op, Reading, Scalar, Schedule, compare, format_duration,
    in_time_range, parse_clock, parse_duration, pick_duration, switch_exits,
};
use oikos_core::history::buckets::{Sample, readings_mean};
use serde_json::Value;
use tokio::sync::{broadcast, mpsc};
use tokio::task::{AbortHandle, JoinSet};

use super::EngineRequest;
use super::schedule::{Armed, Now, now_in, rearm, timer_clock};
use super::sun::{ArmedSun, next_sun_fire};
use crate::clock;
use crate::hub::{Hub, StateSeen};
use oikos_api::ConnectionStatus;

/// What a finished run task reports: the flow, the run and how it ended.
type Finished = (String, String, RunStatus);

/// A run in progress; at most one per flow.
struct Active {
    run_id: String,
    /// The definition the run started with; a changed definition cancels the run.
    flow: Flow,
    abort: AbortHandle,
    /// The task's id, to find the run again when the task panics.
    task: tokio::task::Id,
}

/// Runs the engine until `requests` closes. Subscribes to the hub's events itself; the hub
/// starts it through `Hub::start_engine`, which subscribes earlier so no event is missed.
pub async fn run(hub: Arc<Hub>, requests: mpsc::UnboundedReceiver<EngineRequest>) {
    let events = hub.subscribe();
    let seen = hub.subscribe_state_seen();
    run_with(hub, events, seen, requests).await;
}

/// [`run`] with the event subscription already taken.
pub(super) async fn run_with(
    hub: Arc<Hub>,
    mut events: broadcast::Receiver<WsEvent>,
    mut seen: broadcast::Receiver<StateSeen>,
    mut requests: mpsc::UnboundedReceiver<EngineRequest>,
) {
    let mut engine = Engine {
        hub: Arc::clone(&hub),
        flows: Vec::new(),
        known: BTreeMap::new(),
        watched: BTreeSet::new(),
        active: BTreeMap::new(),
        keep: BTreeSet::new(),
        tasks: JoinSet::new(),
        zone: hub.zone(),
        now: timer_clock(),
        schedules: BTreeMap::new(),
        sun_timers: BTreeMap::new(),
    };
    engine.reload();
    loop {
        let deadline = engine.next_deadline();
        // Requests first: a flow saved before the state it watches changes must be armed
        // before that change is looked at. Finished runs before events: a run that is over must
        // not count as active for a trigger in the same iteration.
        tokio::select! {
            biased;
            request = requests.recv() => {
                let Some(request) = request else { return };
                let mut pending = vec![request];
                // Coalesce a burst of edits into one reload.
                while let Ok(next) = requests.try_recv() {
                    if next != EngineRequest::Reload
                        || pending.last() != Some(&EngineRequest::Reload)
                    {
                        pending.push(next);
                    }
                }
                for request in pending {
                    engine.handle_request(request);
                }
            }
            Some(joined) = engine.tasks.join_next_with_id(), if !engine.tasks.is_empty() => {
                engine.run_finished(joined);
            }
            // Before the events: a tick that is due is not held back by a busy event stream.
            () = sleep_until_deadline(deadline), if deadline.is_some() => {
                engine.tick();
                engine.tick_suns();
            }
            event = events.recv() => match event {
                Ok(event) => engine.handle_event(event),
                Err(broadcast::error::RecvError::Lagged(missed)) => {
                    tracing::warn!(missed, "flow engine lagged behind the events, forgetting values");
                    engine.known.clear();
                }
                Err(broadcast::error::RecvError::Closed) => return,
            },
            // After the events, so a change is stored by `StateChanged` first. This one only
            // re-seeds an entity whose report was identical to its last state (no event).
            seen = seen.recv() => match seen {
                Ok(seen) => engine.state_seen(&seen),
                // Nothing to recover: the next report of an entity seeds it again.
                Err(broadcast::error::RecvError::Lagged(_)) => {}
                Err(broadcast::error::RecvError::Closed) => return,
            },
        }
    }
}

/// Sleeps until `deadline`; never completes without one (the select branch is disabled then).
async fn sleep_until_deadline(deadline: Option<tokio::time::Instant>) {
    match deadline {
        Some(deadline) => tokio::time::sleep_until(deadline).await,
        None => std::future::pending().await,
    }
}

struct Engine {
    hub: Arc<Hub>,
    /// The house time zone schedules run in.
    zone: chrono_tz::Tz,
    now: Now,
    /// Armed schedule nodes, keyed `"{flow}/{node}"`.
    schedules: BTreeMap<String, Armed>,
    /// Armed sun nodes, keyed `"{flow}/{node}"`.
    sun_timers: BTreeMap<String, ArmedSun>,
    /// Valid and enabled flows.
    flows: Vec<Flow>,
    /// The last value seen for every entity a trigger watches; `None` inside means the entity
    /// had no value when last seen. Absent means never seen since the last reset.
    known: BTreeMap<String, Option<Scalar>>,
    /// The entities watched as of the last reload. Kept apart from `known` because a
    /// disconnect empties `known` while the watched set stays: those entities must stay
    /// unseeded until the broker's replay shows them, not be re-seeded from stale values.
    watched: BTreeSet<String>,
    active: BTreeMap<String, Active>,
    /// `(flow, run)` pairs whose run disabled its own flow with a `flow.enable` node: that run
    /// survives the reload the disable causes. Forgotten when the run ends.
    keep: BTreeSet<(String, String)>,
    tasks: JoinSet<Finished>,
}

/// Whether two definitions of a flow do the same thing: they may differ only in how the UI
/// presents the flow (`name`, `description`, `area`), so renaming it or moving it to another room
/// never cancels its run.
fn same_behaviour(a: &Flow, b: &Flow) -> bool {
    let behaviour = |flow: &Flow| Flow {
        name: String::new(),
        description: None,
        area: None,
        ..flow.clone()
    };
    behaviour(a) == behaviour(b)
}

impl Engine {
    fn handle_request(&mut self, request: EngineRequest) {
        match request {
            EngineRequest::Reload => self.reload(),
            EngineRequest::Run { flow_id, run_id } => self.run_manual(&flow_id, run_id),
            EngineRequest::KeepRun { flow_id, run_id } => {
                self.keep.insert((flow_id, run_id));
            }
        }
    }

    /// Re-reads the definitions, seeds the values of newly watched entities from the hub (so a
    /// flow written while the entity already has a value fires on its very next change) and
    /// cancels runs whose flow was removed, disabled or changed in what it does.
    fn reload(&mut self) {
        let definitions = self.hub.flow_definitions();
        self.flows = definitions
            .iter()
            .filter(|flow| flow.enabled)
            .cloned()
            .collect();
        let watched = self.watched_entities();
        self.known.retain(|id, _| watched.contains(id));
        // Seed only what became watched now, and only while the link is up: otherwise the
        // registry may hold pre-drop values and the broker's replay would look like a change.
        let online = *self.hub.connection().borrow() == ConnectionStatus::Online;
        if online {
            for id in watched.difference(&self.watched) {
                let scalar = self.hub.entity_scalar(id);
                self.known.insert(id.clone(), scalar);
            }
        }
        self.watched = watched;
        self.hub.set_averaged(averaged_windows(&self.flows));
        let stale: Vec<String> = self
            .active
            .iter()
            .filter(|(id, active)| {
                let unchanged = self
                    .flows
                    .iter()
                    .any(|f| f.id == **id && same_behaviour(f, &active.flow));
                !(unchanged || self.survives_self_disable(id, active, &definitions))
            })
            .map(|(id, _)| id.clone())
            .collect();
        for id in stale {
            self.cancel(&id);
        }
        self.arm_schedules();
        self.arm_suns();
    }

    /// Whether `active` is a run that disabled its own flow (announced by `KeepRun`) and whose
    /// flow behaves like the definition it started with but for `enabled`.
    fn survives_self_disable(&self, id: &str, active: &Active, definitions: &[Flow]) -> bool {
        if !self.keep.contains(&(id.to_owned(), active.run_id.clone())) {
            return false;
        }
        let enabled = |flow: &Flow| Flow {
            enabled: true,
            ..flow.clone()
        };
        definitions
            .iter()
            .any(|f| f.id == id && same_behaviour(&enabled(f), &enabled(&active.flow)))
    }

    /// Arms every schedule node of an enabled flow that is not armed with the same expression
    /// yet, from now (never immediately), and disarms the rest: disabled, removed or edited.
    fn arm_schedules(&mut self) {
        let now = now_in(&self.now, self.zone);
        let mut wanted = BTreeMap::new();
        for flow in &self.flows {
            for node in flow.triggers() {
                let NodeKind::TriggerSchedule(params) = &node.kind else {
                    continue;
                };
                wanted.insert(
                    format!("{}/{}", flow.id, node.id),
                    (node.id.clone(), params.cron.clone()),
                );
            }
        }
        self.schedules.retain(|key, armed| {
            wanted
                .get(key)
                .is_some_and(|(node, cron)| *node == armed.node && *cron == armed.cron)
        });
        for (key, (node, cron)) in wanted {
            if self.schedules.contains_key(&key) {
                continue;
            }
            // The definition was validated when the flow was stored.
            let Ok(schedule) = Schedule::parse(&cron) else {
                continue;
            };
            if let Some(at) = schedule.next_after(&now) {
                self.schedules.insert(
                    key,
                    Armed {
                        node,
                        at,
                        schedule,
                        cron,
                    },
                );
            }
        }
    }

    /// Arms every sun node of an enabled flow that is not armed for the same event and offset
    /// yet, and disarms the rest: disabled, removed or edited. A node whose next event is more
    /// than seven days out (polar night, midnight sun) stays disarmed until the next reload.
    fn arm_suns(&mut self) {
        let now = now_in(&self.now, self.zone);
        let (lat, lon) = self.hub.house_point();
        let mut wanted = BTreeMap::new();
        for flow in &self.flows {
            for node in flow.triggers() {
                let NodeKind::TriggerSun(params) = &node.kind else {
                    continue;
                };
                wanted.insert(
                    format!("{}/{}", flow.id, node.id),
                    (
                        flow.id.clone(),
                        node.id.clone(),
                        params.which,
                        params.offset(),
                    ),
                );
            }
        }
        self.sun_timers.retain(|key, armed| {
            wanted.get(key).is_some_and(|(_, node, which, offset)| {
                *node == armed.node && *which == armed.which && *offset == armed.offset_minutes
            })
        });
        for (key, (flow, node, which, offset)) in wanted {
            if self.sun_timers.contains_key(&key) {
                continue;
            }
            match next_sun_fire(lat, lon, self.zone, which, offset, now) {
                Some(at) => {
                    self.sun_timers.insert(
                        key,
                        ArmedSun {
                            node,
                            at,
                            which,
                            offset_minutes: offset,
                        },
                    );
                }
                None => tracing::warn!(
                    flow = %flow,
                    node = %node,
                    "sun flow disarmed: next event more than 7 days out or does not exist"
                ),
            }
        }
    }

    /// When the timer branch should wake: the earliest armed tick or sun event, as a tokio
    /// instant.
    fn next_deadline(&self) -> Option<tokio::time::Instant> {
        let at = self
            .schedules
            .values()
            .map(|armed| armed.at)
            .chain(self.sun_timers.values().map(|armed| armed.at))
            .min()?;
        let wait = (at.to_utc() - (self.now)())
            .to_std()
            .unwrap_or(std::time::Duration::ZERO);
        Some(tokio::time::Instant::now() + wait)
    }

    /// Fires every schedule whose tick is due and re-arms it for the next one. A tick while the
    /// broker link is not online is recorded as skipped instead: commands could not go out.
    fn tick(&mut self) {
        let now = now_in(&self.now, self.zone);
        let due: Vec<String> = self
            .schedules
            .iter()
            .filter(|(_, armed)| armed.at <= now)
            .map(|(key, _)| key.clone())
            .collect();
        let online = *self.hub.connection().borrow() == ConnectionStatus::Online;
        for key in due {
            let Some(armed) = self.schedules.get_mut(&key) else {
                continue;
            };
            let node = armed.node.clone();
            let next = rearm(&armed.schedule, armed.at, now);
            match next {
                Some(at) => armed.at = at,
                None => {
                    self.schedules.remove(&key);
                }
            }
            let flow_id = key.split('/').next().unwrap_or_default();
            let Some(flow) = self.flows.iter().find(|f| f.id == flow_id).cloned() else {
                continue;
            };
            if online {
                self.fire(flow, node, None);
            } else {
                self.hub
                    .add_skipped_run_because(&flow.id, &node, "the broker link is offline");
            }
        }
    }

    /// Fires every sun node whose event is due and re-arms it for the next one, skipping while
    /// the broker link is not online like `tick`. Events missed while the hub was busy are
    /// never replayed: the next one is searched from now.
    fn tick_suns(&mut self) {
        let now = now_in(&self.now, self.zone);
        let (lat, lon) = self.hub.house_point();
        let due: Vec<String> = self
            .sun_timers
            .iter()
            .filter(|(_, armed)| armed.at <= now)
            .map(|(key, _)| key.clone())
            .collect();
        let online = *self.hub.connection().borrow() == ConnectionStatus::Online;
        for key in due {
            let Some(armed) = self.sun_timers.get_mut(&key) else {
                continue;
            };
            let node = armed.node.clone();
            let next = next_sun_fire(lat, lon, self.zone, armed.which, armed.offset_minutes, now);
            match next {
                Some(at) => armed.at = at,
                None => {
                    self.sun_timers.remove(&key);
                }
            }
            let flow_id = key.split('/').next().unwrap_or_default();
            let Some(flow) = self.flows.iter().find(|f| f.id == flow_id).cloned() else {
                continue;
            };
            if online {
                self.fire(flow, node, None);
            } else {
                self.hub
                    .add_skipped_run_because(&flow.id, &node, "the broker link is offline");
            }
        }
    }

    /// The entities some enabled flow triggers on.
    fn watched_entities(&self) -> BTreeSet<String> {
        self.flows
            .iter()
            .flat_map(|flow| flow.triggers())
            .filter_map(|node| match &node.kind {
                NodeKind::TriggerState(params) => Some(params.entity.clone()),
                _ => None,
            })
            .collect()
    }

    fn handle_event(&mut self, event: WsEvent) {
        match event {
            WsEvent::StateChanged { id, state } => {
                self.state_changed(&id, Scalar::of_state(&state.value));
            }
            WsEvent::ButtonPressed { id } => self.button_pressed(&id),
            WsEvent::SignalReceived { id, signal } => self.signal_received(&id, &signal),
            // The broker is about to replay retained states: forget what we knew so the replay
            // is only recorded, never acted on.
            WsEvent::ConnectionChanged(status) if status != ConnectionStatus::Online => {
                self.known.clear();
            }
            _ => {}
        }
    }

    /// Fires every trigger on button `id`. A press is an event that is never replayed (the hub
    /// ignores retained messages on a button topic), so the `known` map is bypassed. A button
    /// has no value, so the trigger carries the press itself.
    fn button_pressed(&mut self, id: &str) {
        let mut fires = Vec::new();
        for flow in &self.flows {
            for node in flow.triggers() {
                if matches!(&node.kind, NodeKind::TriggerState(params) if params.entity == id)
                    && !fires.iter().any(|(f, _): &(Flow, String)| f.id == flow.id)
                {
                    fires.push((flow.clone(), node.id.clone()));
                }
            }
        }
        let trigger = TriggerInfo {
            entity: id.to_owned(),
            value: Value::String("PRESS".to_owned()),
            previous: Value::Null,
        };
        for (flow, node) in fires {
            self.fire(flow, node, Some(trigger.clone()));
        }
    }

    /// Fires every `trigger.signal` on `id` that wants this signal. Like a press, a signal is an
    /// event the hub never replays from retained messages, so the `known` map is bypassed. The
    /// trigger value is the signal name.
    fn signal_received(&mut self, id: &str, signal: &str) {
        let mut fires = Vec::new();
        for flow in &self.flows {
            for node in flow.triggers() {
                if matches!(&node.kind, NodeKind::TriggerSignal(params)
                        if params.entity == id
                            && params.signal.as_deref().is_none_or(|s| s == signal))
                    && !fires.iter().any(|(f, _): &(Flow, String)| f.id == flow.id)
                {
                    fires.push((flow.clone(), node.id.clone()));
                }
            }
        }
        let trigger = TriggerInfo {
            entity: id.to_owned(),
            value: Value::String(signal.to_owned()),
            previous: Value::Null,
        };
        for (flow, node) in fires {
            self.fire(flow, node, Some(trigger.clone()));
        }
    }

    /// A state topic of `id` was reported, changed or not. A watched entity missing from the
    /// map is seeded silently from the registry: after a reconnect the retained replay is
    /// usually identical to the last state, so the registry emits no `StateChanged` for it.
    ///
    /// A report from before the latest link drop is stale (the engine may only get to it after
    /// the clear) and is ignored: its registry value is exactly the pre-drop one.
    fn state_seen(&mut self, seen: &StateSeen) {
        let id = seen.id.as_str();
        if seen.epoch == self.hub.link_epoch()
            && self.watched.contains(id)
            && !self.known.contains_key(id)
        {
            let scalar = self.hub.entity_scalar(id);
            self.known.insert(id.to_owned(), scalar);
        }
    }

    /// Fires the triggers an entity's change satisfies. Only a change between two present,
    /// different values counts: a first sighting, an availability-only report (same value) and
    /// a value appearing or vanishing are recorded silently.
    fn state_changed(&mut self, id: &str, next: Option<Scalar>) {
        let Some(slot) = self.known.get(id) else {
            // Either not watched, or never seen since the last reset.
            if self.watched.contains(id) {
                self.known.insert(id.to_owned(), next);
            }
            return;
        };
        let previous = slot.clone();
        self.known.insert(id.to_owned(), next.clone());
        let (Some(previous), Some(next)) = (previous, next) else {
            return;
        };
        if previous == next {
            return;
        }
        let mut fires = Vec::new();
        for flow in &self.flows {
            for node in flow.triggers() {
                let NodeKind::TriggerState(params) = &node.kind else {
                    continue;
                };
                if params.entity == id
                    && !fires.iter().any(|(f, _): &(Flow, String)| f.id == flow.id)
                    && filter_matches(params.to.as_ref(), &next)
                    && filter_matches(params.from.as_ref(), &previous)
                {
                    fires.push((flow.clone(), node.id.clone()));
                }
            }
        }
        let trigger = TriggerInfo {
            entity: id.to_owned(),
            value: next.to_json(),
            previous: previous.to_json(),
        };
        for (flow, node) in fires {
            self.fire(flow, node, Some(trigger.clone()));
        }
    }

    /// Starts a run for a satisfied trigger, honouring the flow's mode when one is active.
    fn fire(&mut self, flow: Flow, node_id: String, trigger: Option<TriggerInfo>) {
        if self.active.contains_key(&flow.id) {
            match flow.mode {
                oikos_core::flow::FlowMode::Single => {
                    self.hub.add_skipped_run(&flow.id, &node_id, trigger);
                    return;
                }
                oikos_core::flow::FlowMode::Restart => self.cancel(&flow.id),
            }
        }
        let run = match self.hub.begin_run(&flow.id, trigger.clone()) {
            Ok(run) => run,
            Err(err) => {
                tracing::warn!(flow = %flow.id, error = %err, "could not begin a run");
                return;
            }
        };
        self.spawn(flow, run.id, node_id, trigger);
    }

    /// Executes a run the hub created for a manual request, from the flow's first trigger.
    fn run_manual(&mut self, flow_id: &str, run_id: String) {
        let flow = self.flows.iter().find(|f| f.id == flow_id).cloned();
        let start = flow
            .as_ref()
            .and_then(|f| f.triggers().first().map(|n| n.id.clone()));
        match (flow, start) {
            (Some(flow), Some(start)) if !self.active.contains_key(flow_id) => {
                self.spawn(flow, run_id, start, None);
            }
            (Some(_), Some(_)) => self.hub.end_run(flow_id, &run_id, RunStatus::Cancelled),
            _ => {
                tracing::warn!(
                    flow = flow_id,
                    "manual run of a flow the engine does not have"
                );
                self.hub.end_run(flow_id, &run_id, RunStatus::Cancelled);
            }
        }
    }

    fn spawn(&mut self, flow: Flow, run_id: String, start: String, trigger: Option<TriggerInfo>) {
        let hub = Arc::clone(&self.hub);
        let now = Arc::clone(&self.now);
        let (flow_id, task_flow, task_run) = (flow.id.clone(), flow.clone(), run_id.clone());
        let abort = self.tasks.spawn(async move {
            let status = execute(hub, now, task_flow, task_run.clone(), start, trigger).await;
            (flow_id, task_run, status)
        });
        self.active.insert(
            flow.id.clone(),
            Active {
                run_id,
                flow,
                task: abort.id(),
                abort,
            },
        );
    }

    /// Aborts a flow's active run and ends it `Cancelled`.
    fn cancel(&mut self, flow_id: &str) {
        if let Some(active) = self.active.remove(flow_id) {
            self.keep.retain(|(flow, _)| flow != flow_id);
            active.abort.abort();
            self.hub
                .end_run(flow_id, &active.run_id, RunStatus::Cancelled);
        }
    }

    /// A run task ended. A cancelled one was already ended by whoever cancelled it, and a run
    /// that finished at the very moment it was cancelled no longer matches `active`.
    fn run_finished(
        &mut self,
        joined: Result<(tokio::task::Id, Finished), tokio::task::JoinError>,
    ) {
        match joined {
            Ok((_, (flow_id, run_id, status))) => {
                if self
                    .active
                    .get(&flow_id)
                    .is_some_and(|active| active.run_id == run_id)
                {
                    self.active.remove(&flow_id);
                    self.keep.retain(|(flow, _)| *flow != flow_id);
                    self.hub.end_run(&flow_id, &run_id, status);
                }
            }
            Err(err) if err.is_cancelled() => {}
            Err(err) => {
                tracing::error!(error = %err, "a flow run panicked");
                // Without this the flow would stay active forever and never run again.
                let flow_id = self
                    .active
                    .iter()
                    .find(|(_, active)| active.task == err.id())
                    .map(|(id, _)| id.clone());
                if let Some(flow_id) = flow_id
                    && let Some(active) = self.active.remove(&flow_id)
                {
                    self.keep.retain(|(flow, _)| *flow != flow_id);
                    let failure = step(
                        "engine",
                        StepOutcome::Failed,
                        "run panicked".to_owned(),
                        None,
                    );
                    self.hub.record_step(&flow_id, &active.run_id, failure);
                    self.hub
                        .end_run(&flow_id, &active.run_id, RunStatus::Failed);
                }
            }
        }
    }
}

/// `eq` semantics of `compare`: the filter must be a scalar equal to the value, type included.
fn filter_matches(filter: Option<&Value>, actual: &Scalar) -> bool {
    filter.is_none_or(|value| Scalar::from_json(value).as_ref() == Some(actual))
}

/// Walks the flow depth-first from `start`, recording a step per node before following its
/// successors in edge order. The first failed step ends the run.
///
/// A cancelled run must not act: `abort()` only lands at the task's next `.await`, so a run can
/// wake from its delay after the flow was deleted, disabled or the run ended. `record_step`
/// reports whether the run is still the flow's active one; the run stops (as `Cancelled`) as
/// soon as it is not, and every delay is followed by a step, so a command never follows a
/// wake-up that was not checked. The window between that check and the publish inside
/// `send_command` (no `.await` in between) is accepted.
pub(super) async fn execute(
    hub: Arc<Hub>,
    now: Now,
    flow: Flow,
    run_id: String,
    start: String,
    trigger: Option<TriggerInfo>,
) -> RunStatus {
    let mut stack = vec![start];
    while let Some(id) = stack.pop() {
        let Some(node) = flow.node(&id) else {
            let step = step(&id, StepOutcome::Failed, "unknown node".to_owned(), None);
            if !hub.record_step(&flow.id, &run_id, step) {
                return RunStatus::Cancelled;
            }
            return RunStatus::Failed;
        };
        let (outcome, detail, ports) =
            run_node(&hub, &now, &flow.id, &run_id, node, trigger.as_ref()).await;
        let branching = matches!(
            node.kind,
            NodeKind::If(_) | NodeKind::Switch(_) | NodeKind::TimeBetween(_)
        );
        let shown = (branching && outcome == StepOutcome::Ok).then(|| ports.clone());
        if !hub.record_step(&flow.id, &run_id, step(&node.id, outcome, detail, shown)) {
            return RunStatus::Cancelled;
        }
        if outcome == StepOutcome::Failed {
            return RunStatus::Failed;
        }
        // A node with no port to follow continues on its port-less edges; a branching node
        // that fired nothing follows nothing (its edges all carry a port).
        let next: Vec<&Node> = if branching {
            ports
                .iter()
                .flat_map(|port| flow.successors(&node.id, Some(port.as_str())))
                .collect()
        } else {
            flow.successors(&node.id, None)
        };
        stack.extend(next.into_iter().rev().map(|next| next.id.clone()));
    }
    RunStatus::Done
}

fn step(node: &str, outcome: StepOutcome, detail: String, ports: Option<Vec<String>>) -> StepView {
    StepView {
        node: node.to_owned(),
        at: clock::now(),
        outcome,
        detail,
        ports,
    }
}

/// Runs one node: its outcome, the detail to record and the output port to follow.
async fn run_node(
    hub: &Hub,
    now: &Now,
    flow_id: &str,
    run_id: &str,
    node: &Node,
    trigger: Option<&TriggerInfo>,
) -> (StepOutcome, String, Vec<String>) {
    let failed = |detail: String| (StepOutcome::Failed, detail, Vec::new());
    match &node.kind {
        NodeKind::TriggerState(params) => {
            let detail = match trigger {
                Some(trigger) => format!("{} = {}", params.entity, trigger.value),
                None => "manual".to_owned(),
            };
            (StepOutcome::Ok, detail, Vec::new())
        }
        NodeKind::TriggerSignal(params) => {
            let detail = match trigger {
                Some(trigger) => format!("{} sent {}", params.entity, trigger.value),
                None => "manual".to_owned(),
            };
            (StepOutcome::Ok, detail, Vec::new())
        }
        NodeKind::TriggerSchedule(params) => (
            StepOutcome::Ok,
            format!("schedule {}", params.cron),
            Vec::new(),
        ),
        NodeKind::TriggerSun(params) => (
            StepOutcome::Ok,
            format!(
                "sun {} {:+}m",
                format!("{:?}", params.which).to_lowercase(),
                params.offset()
            ),
            Vec::new(),
        ),
        NodeKind::FlowEnable(params) => {
            let target = params.flow.as_deref().unwrap_or(flow_id);
            if hub.flow(target).is_none() {
                return failed(format!("unknown flow `{target}`"));
            }
            let result = if target == flow_id {
                hub.set_flow_enabled_by_run(target, params.enabled, run_id)
                    .await
            } else {
                hub.set_flow_enabled(target, params.enabled).await
            };
            match result {
                Ok(_) => {
                    let verb = if params.enabled {
                        "enabled"
                    } else {
                        "disabled"
                    };
                    (StepOutcome::Ok, format!("{verb} {target}"), Vec::new())
                }
                Err(err) => failed(err.to_string()),
            }
        }
        NodeKind::TimeBetween(params) => {
            let (Ok(from), Ok(to)) = (parse_clock(&params.from), parse_clock(&params.to)) else {
                return failed("time is not HH:MM".to_owned());
            };
            let local = now().with_timezone(&hub.zone());
            let minute = u16::try_from(local.hour() * 60 + local.minute()).unwrap_or(0);
            let port = if in_time_range(from, to, minute) {
                "in"
            } else {
                "out"
            };
            (
                StepOutcome::Ok,
                format!("{:02}:{:02} → {port}", local.hour(), local.minute()),
                vec![port.to_owned()],
            )
        }
        NodeKind::Switch(params) => {
            let left = match node_left(
                hub,
                now,
                params.entity.as_deref(),
                params.average.as_deref(),
                trigger,
            )
            .await
            {
                Ok(left) => left,
                Err(reason) => return failed(reason),
            };
            let exits = match switch_exits(params, left.as_ref()) {
                Ok(exits) => exits,
                Err(err) => return failed(err.to_string()),
            };
            let taken = if exits.is_empty() {
                "none".to_owned()
            } else {
                exits.join(",")
            };
            let detail = match (&left, params.average.as_deref()) {
                (None, _) => format!("unknown → {taken}"),
                (Some(Scalar::Number(mean)), Some(window)) => {
                    format!("average {window} = {mean:.1} → {taken}")
                }
                (Some(value), _) => format!("{} → {taken}", scalar_text(value)),
            };
            (StepOutcome::Ok, detail, exits)
        }
        NodeKind::Notify(params) => {
            let lang = params.lang.unwrap_or_default();
            let lookup = |id: &str| hub.reading(id);
            // The value the trigger carried, with the unit and words of its entity.
            let trigger = trigger.and_then(|t| {
                let value = Scalar::from_json(&t.value)?;
                let mut reading = hub.reading(&t.entity).unwrap_or(Reading {
                    value: value.clone(),
                    unit: None,
                    device_class: None,
                });
                reading.value = value;
                Some(reading)
            });
            // `validate` refused a text that does not parse, so the fallback never shows.
            let fill = |text: &str| {
                render(text, lang, &lookup, trigger.as_ref()).unwrap_or_else(|_| text.to_owned())
            };
            let title = fill(&params.title);
            let mut alert = serde_json::json!({
                "v": 1,
                "id": params
                    .id
                    .clone()
                    .unwrap_or_else(|| format!("flow-{flow_id}-{run_id}")),
                "title": title,
                "channel": params.channel.as_deref().unwrap_or("misc"),
                "severity": params.severity.as_deref().unwrap_or("info"),
            });
            for (key, value) in [
                ("body", params.body.as_deref().map(fill)),
                ("entity_id", params.entity_id.clone()),
                ("image_url", params.image_url.clone()),
            ] {
                if let Some(value) = value {
                    alert[key] = Value::String(value);
                }
            }
            if let Some(silent) = params.silent {
                alert["silent"] = Value::Bool(silent);
            }
            match hub.notify("flow", &alert) {
                Ok(id) => (StepOutcome::Ok, format!("sent {id}: {title}"), Vec::new()),
                Err(rejected) => failed(rejected.reason),
            }
        }
        NodeKind::If(params) => {
            if params.op == Op::IsDark {
                let (lat, lon) = hub.house_point();
                let dark = is_dark_with_offset(lat, lon, now(), params.dark_offset());
                let port = if dark { "true" } else { "false" };
                return (
                    StepOutcome::Ok,
                    format!("is_dark → {dark}"),
                    vec![port.to_owned()],
                );
            }
            let Some(right) = Scalar::from_json(params.value.as_ref().unwrap_or(&Value::Null))
            else {
                return failed("value is not a scalar".to_owned());
            };
            let left = match node_left(
                hub,
                now,
                params.entity.as_deref(),
                params.average.as_deref(),
                trigger,
            )
            .await
            {
                Ok(left) => left,
                Err(reason) => return failed(reason),
            };
            match (&left, params.missing) {
                (None, Some(Missing::True)) => {
                    return (
                        StepOutcome::Ok,
                        "missing → true".to_owned(),
                        vec!["true".to_owned()],
                    );
                }
                (None, Some(Missing::False)) => {
                    return (
                        StepOutcome::Ok,
                        "missing → false".to_owned(),
                        vec!["false".to_owned()],
                    );
                }
                _ => {}
            }
            let shown = |outcome: bool| match (params.average.as_deref(), &left) {
                (Some(window), Some(Scalar::Number(mean))) => {
                    format!("average {window} = {mean:.1} → {outcome}")
                }
                _ => outcome.to_string(),
            };
            match compare(left.as_ref(), params.op, &right) {
                Ok(true) => (StepOutcome::Ok, shown(true), vec!["true".to_owned()]),
                Ok(false) => (StepOutcome::Ok, shown(false), vec!["false".to_owned()]),
                Err(err) => failed(err.to_string()),
            }
        }
        NodeKind::Delay(params) => match params.bounds() {
            Ok((low, None)) => {
                tokio::time::sleep(low).await;
                (
                    StepOutcome::Ok,
                    format!("waited {}", params.for_),
                    Vec::new(),
                )
            }
            Ok((low, Some(high))) => {
                let duration = pick_duration(low, Some(high), clock::random_roll());
                tokio::time::sleep(duration).await;
                let detail = format!(
                    "waited {} ({} to {})",
                    format_duration(duration),
                    params.for_,
                    params.up_to.as_deref().unwrap_or_default()
                );
                (StepOutcome::Ok, detail, Vec::new())
            }
            Err(reason) => failed(reason),
        },
        NodeKind::Command(params) => {
            match hub
                .send_command(&params.entity, params.command.clone())
                .await
            {
                Ok(response) if response.accepted => {
                    (StepOutcome::Ok, "accepted".to_owned(), Vec::new())
                }
                Ok(response) => failed(
                    response
                        .reason
                        .unwrap_or_else(|| "command not accepted".to_owned()),
                ),
                Err(err) => failed(err.to_string()),
            }
        }
    }
}

/// A scalar as shown in a step detail.
fn scalar_text(value: &Scalar) -> String {
    match value {
        Scalar::Bool(b) => b.to_string(),
        Scalar::Number(n) => n.to_string(),
        Scalar::Text(t) => t.clone(),
    }
}

/// The value an `if` or `switch` node compares: `entity` (else the one that fired the trigger),
/// as its current value or, with `average`, the mean of its readings over that window.
/// `Ok(None)` when the value is unknown; `Err` carries the failure reason.
async fn node_left(
    hub: &Hub,
    now: &Now,
    entity: Option<&str>,
    average: Option<&str>,
    trigger: Option<&TriggerInfo>,
) -> Result<Option<Scalar>, String> {
    let entity = match (entity, trigger) {
        (Some(entity), _) => entity,
        (None, Some(trigger)) => trigger.entity.as_str(),
        (None, None) => return Err("no triggering entity".to_owned()),
    };
    Ok(match average {
        Some(window) => window_average(hub, entity, window, now())
            .await
            .map(Scalar::Number),
        None => hub.entity_scalar(entity),
    })
}

/// The entities the `if` and `switch` nodes of `flows` average, each with its longest window in
/// microseconds. A node without `entity` averages the entities its flow's triggers watch.
fn averaged_windows(flows: &[Flow]) -> BTreeMap<String, i64> {
    let mut windows = BTreeMap::new();
    for flow in flows {
        let triggers: Vec<&String> = flow
            .nodes
            .iter()
            .filter_map(|node| match &node.kind {
                NodeKind::TriggerState(params) => Some(&params.entity),
                _ => None,
            })
            .collect();
        for node in &flow.nodes {
            let (entity, average) = match &node.kind {
                NodeKind::If(params) => (&params.entity, &params.average),
                NodeKind::Switch(params) => (&params.entity, &params.average),
                _ => continue,
            };
            let Some(window) = average.as_deref().and_then(|w| parse_duration(w).ok()) else {
                continue;
            };
            let micros = i64::try_from(window.as_micros()).unwrap_or(i64::MAX);
            let entities = match entity {
                Some(entity) => vec![entity],
                None => triggers.clone(),
            };
            for entity in entities {
                let longest = windows.entry(entity.clone()).or_insert(micros);
                *longest = (*longest).max(micros);
            }
        }
    }
    windows
}

/// The mean of the readings of `entity` over the `window` ending at `now`: the ones the hub
/// kept, newest included, with history filling the part of the window before them (after a
/// restart, or a flow saved since). With no reading in the window the value held throughout,
/// the current one, is the mean. `None` when the entity is unknown, unavailable or not a number.
async fn window_average(
    hub: &Hub,
    entity: &str,
    window: &str,
    now: chrono::DateTime<chrono::Utc>,
) -> Option<f64> {
    let Scalar::Number(current) = hub.entity_scalar(entity)? else {
        return None;
    };
    let window = parse_duration(window).ok()?;
    let end = now.timestamp_micros();
    let start = end - i64::try_from(window.as_micros()).ok()?;
    let recent = hub.readings_since(entity, start);
    let recorded = match hub.history().map(|history| history.db()) {
        Some(Ok(db)) => match db.samples(start, end + 1, Some(entity)).await {
            Ok(rows) => rows.into_iter().map(|(_, sample)| sample).collect(),
            Err(err) => {
                tracing::warn!(%entity, %err, "average: history read failed");
                Vec::new()
            }
        },
        _ => Vec::<Sample>::new(),
    };
    Some(readings_mean(&recorded, &recent, start).unwrap_or(current))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use oikos_core::command::Publish;

    fn sun_flow(id: &str, enabled: bool) -> String {
        serde_json::json!({
            "id": id, "name": id, "enabled": enabled, "mode": "single",
            "nodes": [{ "id": "sun", "type": "trigger.sun", "params": { "which": "sunrise" } }],
            "edges": []
        })
        .to_string()
    }

    /// An engine whose clock reads 2026-01-01 00:00 UTC, on a hub at (`lat`, `lon`).
    fn engine_at(lat: f64, lon: f64, zone: &str) -> Engine {
        let mut cfg = crate::config::Config::default();
        cfg.flows.timezone = Some(zone.to_owned());
        cfg.flows.latitude = Some(lat);
        cfg.flows.longitude = Some(lon);
        let (tx, _rx) = mpsc::channel::<Publish>(8);
        let hub = Hub::new(&cfg, tx);
        let fixed = chrono::Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        Engine {
            zone: hub.zone(),
            hub,
            flows: Vec::new(),
            known: BTreeMap::new(),
            watched: BTreeSet::new(),
            active: BTreeMap::new(),
            keep: BTreeSet::new(),
            tasks: JoinSet::new(),
            now: Arc::new(move || fixed),
            schedules: BTreeMap::new(),
            sun_timers: BTreeMap::new(),
        }
    }

    #[tokio::test]
    async fn polar_night_flow_stays_disarmed_across_a_toggle() {
        let mut engine = engine_at(78.22, 15.65, "Europe/Oslo");
        for enabled in [true, false, true] {
            engine.hub.ingest(
                "oikos/automation/polar",
                sun_flow("polar", enabled).as_bytes(),
            );
            engine.reload();
            assert!(engine.sun_timers.is_empty(), "enabled={enabled}");
        }
    }

    #[tokio::test]
    async fn temperate_flow_arms_on_the_same_date_and_disarms_when_disabled() {
        let mut engine = engine_at(48.8566, 2.3522, "Europe/Paris");
        engine
            .hub
            .ingest("oikos/automation/day", sun_flow("day", true).as_bytes());
        engine.reload();
        assert_eq!(engine.sun_timers.keys().collect::<Vec<_>>(), ["day/sun"]);
        engine
            .hub
            .ingest("oikos/automation/day", sun_flow("day", false).as_bytes());
        engine.reload();
        assert!(engine.sun_timers.is_empty());
    }

    /// Runs one `if` node with `average` on an engine hub that has no history.
    async fn run_if_average(extra: serde_json::Value) -> (StepOutcome, String, Vec<String>) {
        let engine = engine_at(48.8566, 2.3522, "Europe/Paris");
        let mut params = serde_json::json!({
            "op": "lte", "value": 19, "average": "30m", "entity": "sensor_temperature"
        });
        params
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        let node: Node = serde_json::from_value(serde_json::json!({
            "id": "check", "type": "if", "params": params
        }))
        .unwrap();
        run_node(&engine.hub, &engine.now, "f", "r", &node, None).await
    }

    #[tokio::test]
    async fn if_average_without_history_takes_the_missing_port() {
        let (outcome, detail, port) =
            run_if_average(serde_json::json!({ "missing": "false" })).await;
        assert_eq!(outcome, StepOutcome::Ok);
        assert_eq!(detail, "missing → false");
        assert_eq!(port, ["false"]);
        let (_, detail, port) = run_if_average(serde_json::json!({ "missing": "true" })).await;
        assert_eq!(detail, "missing → true");
        assert_eq!(port, ["true"]);
    }

    #[tokio::test]
    async fn if_average_without_history_and_default_missing_compares_nothing() {
        let (outcome, _, port) = run_if_average(serde_json::json!({})).await;
        assert_eq!(outcome, StepOutcome::Ok);
        assert_eq!(port, ["false"]);
    }

    #[tokio::test]
    async fn if_average_counts_every_reading_including_the_newest() {
        let mut engine = engine_at(48.8566, 2.3522, "Europe/Paris");
        engine.hub.ingest(
            "oikos/sensor/room/temperature/config",
            br#"{"unique_id":"room_temperature","state_topic":"test/room","value_template":"{{ value_json.temperature }}","unit_of_measurement":"\u00b0C"}"#,
        );
        let entity = engine.hub.snapshot().entities[0].id.clone();
        let flow = serde_json::json!({
            "id": "avg", "name": "avg", "enabled": true, "mode": "single",
            "nodes": [
                { "id": "t", "type": "trigger.state", "params": { "entity": entity } },
                { "id": "check", "type": "if", "params": { "op": "lte", "value": 19, "average": "30m" } }
            ],
            "edges": [{ "from": "t", "to": "check" }]
        });
        engine
            .hub
            .ingest("oikos/automation/avg", flow.to_string().as_bytes());
        engine.reload();
        // A repeated reading counts again, and the newest one counts.
        for t in [20, 20, 17] {
            engine
                .hub
                .ingest("test/room", format!(r#"{{"temperature":{t}}}"#).as_bytes());
        }
        let node = engine.flows[0].nodes[1].clone();
        let trigger = TriggerInfo {
            entity: entity.clone(),
            value: serde_json::json!(17),
            previous: serde_json::json!(20),
        };
        let (outcome, detail, port) =
            run_node(&engine.hub, &engine.now, "avg", "r", &node, Some(&trigger)).await;
        assert_eq!(outcome, StepOutcome::Ok);
        assert_eq!(detail, "average 30m = 19.0 → true");
        assert_eq!(port, ["true"]);
    }

    #[tokio::test]
    async fn switch_average_counts_every_reading_including_the_newest() {
        let mut engine = engine_at(48.8566, 2.3522, "Europe/Paris");
        let entity = add_sensor(&engine, None);
        let flow = serde_json::json!({
            "id": "swavg", "name": "swavg", "enabled": true, "mode": "single",
            "nodes": [
                { "id": "t", "type": "trigger.state", "params": { "entity": entity } },
                { "id": "s", "type": "switch", "params": { "average": "30m",
                    "rules": [{ "op": "lte", "value": 18 }, { "op": "lte", "value": 19 }] } }
            ],
            "edges": [{ "from": "t", "to": "s" }]
        });
        engine
            .hub
            .ingest("oikos/automation/swavg", flow.to_string().as_bytes());
        engine.reload();
        // Only a switch registers the entity: its readings must be buffered.
        for t in [20, 20, 17] {
            engine
                .hub
                .ingest("test/room", format!(r#"{{"temperature":{t}}}"#).as_bytes());
        }
        let node = engine.flows[0].nodes[1].clone();
        let trigger = TriggerInfo {
            entity: entity.clone(),
            value: serde_json::json!(17),
            previous: serde_json::json!(20),
        };
        let (outcome, detail, ports) = run_node(
            &engine.hub,
            &engine.now,
            "swavg",
            "r",
            &node,
            Some(&trigger),
        )
        .await;
        assert_eq!(outcome, StepOutcome::Ok);
        // The mean is 19, not the current 17 (which would take rule 1).
        assert_eq!(detail, "average 30m = 19.0 → 2");
        assert_eq!(ports, ["2"]);
    }

    /// Reports a numeric sensor and returns its entity id.
    fn add_sensor(engine: &Engine, value: Option<f64>) -> String {
        engine.hub.ingest(
            "oikos/sensor/room/temperature/config",
            br#"{"unique_id":"room_temperature","state_topic":"test/room","value_template":"{{ value_json.temperature }}","unit_of_measurement":"\u00b0C"}"#,
        );
        if let Some(value) = value {
            engine.hub.ingest(
                "test/room",
                format!(r#"{{"temperature":{value}}}"#).as_bytes(),
            );
        }
        engine.hub.snapshot().entities[0].id.clone()
    }

    fn node(value: serde_json::Value) -> Node {
        serde_json::from_value(value).unwrap()
    }

    #[tokio::test]
    async fn switch_takes_first_matching_exit() {
        let engine = engine_at(48.8566, 2.3522, "Europe/Paris");
        let entity = add_sensor(&engine, Some(20.0));
        let node = node(serde_json::json!({
            "id": "s", "type": "switch", "params": { "entity": entity, "rules": [
                { "op": "lt", "value": 18 },
                { "op": "between", "value": 18, "to": 22 },
                { "op": "gt", "value": 22 } ] }
        }));
        let (outcome, detail, ports) =
            run_node(&engine.hub, &engine.now, "f", "r", &node, None).await;
        assert_eq!(outcome, StepOutcome::Ok);
        assert_eq!(ports, ["2"]);
        assert!(detail.ends_with("→ 2"), "{detail}");
    }

    #[tokio::test]
    async fn switch_unknown_value_takes_otherwise() {
        let engine = engine_at(48.8566, 2.3522, "Europe/Paris");
        let node = node(serde_json::json!({
            "id": "s", "type": "switch", "params": {
                "entity": "nobody", "otherwise": true,
                "rules": [{ "op": "gt", "value": 1 }] }
        }));
        let (outcome, detail, ports) =
            run_node(&engine.hub, &engine.now, "f", "r", &node, None).await;
        assert_eq!(outcome, StepOutcome::Ok);
        assert_eq!(ports, ["otherwise"]);
        assert_eq!(detail, "unknown → otherwise");
    }

    #[tokio::test]
    async fn switch_nothing_fired_reads_none() {
        let engine = engine_at(48.8566, 2.3522, "Europe/Paris");
        let entity = add_sensor(&engine, Some(20.0));
        let node = node(serde_json::json!({
            "id": "s", "type": "switch", "params": {
                "entity": entity, "rules": [{ "op": "gt", "value": 99 }] }
        }));
        let (_, detail, ports) = run_node(&engine.hub, &engine.now, "f", "r", &node, None).await;
        assert!(ports.is_empty());
        assert_eq!(detail, "20 → none");
    }

    /// The ids of the steps a run recorded, in order.
    fn recorded(hub: &Hub, flow: &str) -> Vec<String> {
        hub.flow(flow).unwrap().runs[0]
            .steps
            .iter()
            .map(|s| s.node.clone())
            .collect()
    }

    #[tokio::test]
    async fn switch_all_runs_exits_in_rule_order() {
        let mut engine = engine_at(48.8566, 2.3522, "Europe/Paris");
        let entity = add_sensor(&engine, Some(20.0));
        let flow = serde_json::json!({
            "id": "sw", "name": "sw", "enabled": true, "mode": "single",
            "nodes": [
                { "id": "t", "type": "trigger.state", "params": { "entity": entity } },
                { "id": "s", "type": "switch", "params": { "entity": entity, "all": true,
                    "rules": [{ "op": "gte", "value": 10 }, { "op": "lte", "value": 30 }] } },
                { "id": "a", "type": "notify", "params": { "title": "x" } },
                { "id": "a2", "type": "notify", "params": { "title": "x" } },
                { "id": "b", "type": "notify", "params": { "title": "x" } }
            ],
            "edges": [
                { "from": "t", "to": "s" },
                { "from": "s", "to": "b", "port": "2" },
                { "from": "s", "to": "a", "port": "1" },
                { "from": "a", "to": "a2" }
            ]
        });
        engine
            .hub
            .ingest("oikos/automation/sw", flow.to_string().as_bytes());
        engine.reload();
        let flow = engine.flows[0].clone();
        let run = engine.hub.begin_run("sw", None).unwrap();
        execute(
            engine.hub.clone(),
            engine.now.clone(),
            flow,
            run.id,
            "t".to_owned(),
            None,
        )
        .await;
        // Exit 1 runs before exit 2 whatever the edge order, depth first.
        assert_eq!(recorded(&engine.hub, "sw"), ["t", "s", "a", "a2", "b"]);
    }

    /// Runs a `time.between` node with the clock at `utc`.
    async fn run_time(utc: (i32, u32, u32, u32, u32), from: &str, to: &str) -> Vec<String> {
        let mut engine = engine_at(48.8566, 2.3522, "Europe/Paris");
        let at = chrono::Utc
            .with_ymd_and_hms(utc.0, utc.1, utc.2, utc.3, utc.4, 0)
            .unwrap();
        engine.now = Arc::new(move || at);
        let node = node(serde_json::json!({
            "id": "tm", "type": "time.between", "params": { "from": from, "to": to }
        }));
        run_node(&engine.hub, &engine.now, "f", "r", &node, None)
            .await
            .2
    }

    #[tokio::test]
    async fn time_between_uses_house_zone() {
        // 21:40 UTC is 22:40 in Paris (CET is UTC+1 only in winter; this is CET after 25 Oct).
        let now = (2026, 10, 27, 21, 40);
        assert_eq!(run_time(now, "07:00", "22:30").await, ["out"]);
        assert_eq!(run_time(now, "22:00", "06:00").await, ["in"]);
    }

    #[tokio::test]
    async fn time_between_wall_clock_on_dst_day() {
        // 06:30 UTC on 2026-10-25 is 07:30 in Paris, after the clocks went back.
        assert_eq!(
            run_time((2026, 10, 25, 6, 30), "07:00", "08:00").await,
            ["in"]
        );
    }

    #[tokio::test]
    async fn step_view_records_ports_for_branching_nodes_only() {
        let mut engine = engine_at(48.8566, 2.3522, "Europe/Paris");
        let entity = add_sensor(&engine, Some(20.0));
        let flow = serde_json::json!({
            "id": "pv", "name": "pv", "enabled": true, "mode": "single",
            "nodes": [
                { "id": "t", "type": "trigger.state", "params": { "entity": entity } },
                { "id": "tm", "type": "time.between", "params": { "from": "00:00", "to": "23:59" } },
                { "id": "c", "type": "notify", "params": { "title": "x" } }
            ],
            "edges": [{ "from": "t", "to": "tm" }, { "from": "tm", "to": "c", "port": "in" }]
        });
        engine
            .hub
            .ingest("oikos/automation/pv", flow.to_string().as_bytes());
        engine.reload();
        let flow = engine.flows[0].clone();
        let run = engine.hub.begin_run("pv", None).unwrap();
        execute(
            engine.hub.clone(),
            engine.now.clone(),
            flow,
            run.id,
            "t".to_owned(),
            None,
        )
        .await;
        let steps = engine.hub.flow("pv").unwrap().runs[0].steps.clone();
        assert_eq!(steps[0].ports, None);
        assert_eq!(steps[1].ports, Some(vec!["in".to_owned()]));
        assert_eq!(steps[2].node, "c");
        assert_eq!(steps[2].ports, None);
    }

    #[tokio::test]
    async fn step_view_records_no_ports_for_a_failed_branching_step() {
        let mut engine = engine_at(48.8566, 2.3522, "Europe/Paris");
        let entity = add_sensor(&engine, Some(20.0));
        let flow = serde_json::json!({
            "id": "pf", "name": "pf", "enabled": true, "mode": "single",
            "nodes": [
                { "id": "t", "type": "trigger.state", "params": { "entity": entity } },
                { "id": "tm", "type": "switch", "params": { "rules": [{ "op": "gt", "value": 1 }] } }
            ],
            "edges": [{ "from": "t", "to": "tm" }]
        });
        engine
            .hub
            .ingest("oikos/automation/pf", flow.to_string().as_bytes());
        engine.reload();
        let flow = engine.flows[0].clone();
        let run = engine.hub.begin_run("pf", None).unwrap();
        execute(
            engine.hub.clone(),
            engine.now.clone(),
            flow,
            run.id,
            "t".to_owned(),
            None,
        )
        .await;
        let steps = engine.hub.flow("pf").unwrap().runs[0].steps.clone();
        assert_eq!(steps[1].node, "tm");
        assert_eq!(steps[1].outcome, StepOutcome::Failed);
        assert_eq!(steps[1].ports, None);
    }
}
