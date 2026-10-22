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

use oikos_api::{RunStatus, StepOutcome, StepView, TriggerInfo, WsEvent};
use oikos_core::flow::sun::is_dark_at;
use oikos_core::flow::{
    Flow, Missing, Node, NodeKind, Op, Scalar, Schedule, compare, parse_duration,
};
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
    /// cancels runs whose flow was removed, disabled or edited.
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
        let stale: Vec<String> = self
            .active
            .iter()
            .filter(|(id, active)| {
                let unchanged = self.flows.iter().any(|f| f.id == **id && *f == active.flow);
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
    /// flow differs from the definition it started with in nothing but `enabled`.
    fn survives_self_disable(&self, id: &str, active: &Active, definitions: &[Flow]) -> bool {
        if !self.keep.contains(&(id.to_owned(), active.run_id.clone())) {
            return false;
        }
        let normalised = |flow: &Flow| Flow {
            enabled: true,
            ..flow.clone()
        };
        definitions
            .iter()
            .any(|f| f.id == id && normalised(f) == normalised(&active.flow))
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
                    let failure = step("engine", StepOutcome::Failed, "run panicked".to_owned());
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
            let step = step(&id, StepOutcome::Failed, "unknown node".to_owned());
            if !hub.record_step(&flow.id, &run_id, step) {
                return RunStatus::Cancelled;
            }
            return RunStatus::Failed;
        };
        let (outcome, detail, port) =
            run_node(&hub, &now, &flow.id, &run_id, node, trigger.as_ref()).await;
        if !hub.record_step(&flow.id, &run_id, step(&node.id, outcome, detail)) {
            return RunStatus::Cancelled;
        }
        if outcome == StepOutcome::Failed {
            return RunStatus::Failed;
        }
        stack.extend(
            flow.successors(&node.id, port)
                .into_iter()
                .rev()
                .map(|next| next.id.clone()),
        );
    }
    RunStatus::Done
}

fn step(node: &str, outcome: StepOutcome, detail: String) -> StepView {
    StepView {
        node: node.to_owned(),
        at: clock::now(),
        outcome,
        detail,
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
) -> (StepOutcome, String, Option<&'static str>) {
    let failed = |detail: String| (StepOutcome::Failed, detail, None);
    match &node.kind {
        NodeKind::TriggerState(params) => {
            let detail = match trigger {
                Some(trigger) => format!("{} = {}", params.entity, trigger.value),
                None => "manual".to_owned(),
            };
            (StepOutcome::Ok, detail, None)
        }
        NodeKind::TriggerSignal(params) => {
            let detail = match trigger {
                Some(trigger) => format!("{} sent {}", params.entity, trigger.value),
                None => "manual".to_owned(),
            };
            (StepOutcome::Ok, detail, None)
        }
        NodeKind::TriggerSchedule(params) => {
            (StepOutcome::Ok, format!("schedule {}", params.cron), None)
        }
        NodeKind::TriggerSun(params) => (
            StepOutcome::Ok,
            format!(
                "sun {} {:+}m",
                format!("{:?}", params.which).to_lowercase(),
                params.offset()
            ),
            None,
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
                    (StepOutcome::Ok, format!("{verb} {target}"), None)
                }
                Err(err) => failed(err.to_string()),
            }
        }
        NodeKind::Notify(params) => {
            let mut alert = serde_json::json!({
                "v": 1,
                "id": params
                    .id
                    .clone()
                    .unwrap_or_else(|| format!("flow-{flow_id}-{run_id}")),
                "title": params.title,
                "channel": params.channel.as_deref().unwrap_or("misc"),
                "severity": params.severity.as_deref().unwrap_or("info"),
            });
            for (key, value) in [
                ("body", params.body.as_ref()),
                ("entity_id", params.entity_id.as_ref()),
                ("image_url", params.image_url.as_ref()),
            ] {
                if let Some(value) = value {
                    alert[key] = Value::String(value.clone());
                }
            }
            if let Some(silent) = params.silent {
                alert["silent"] = Value::Bool(silent);
            }
            match hub.notify("flow", &alert) {
                Ok(id) => (StepOutcome::Ok, format!("sent {id}"), None),
                Err(rejected) => failed(rejected.reason),
            }
        }
        NodeKind::If(params) => {
            if params.op == Op::IsDark {
                let (lat, lon) = hub.house_point();
                let dark = is_dark_at(lat, lon, now());
                let port = if dark { "true" } else { "false" };
                return (StepOutcome::Ok, format!("is_dark → {dark}"), Some(port));
            }
            let Some(right) = Scalar::from_json(params.value.as_ref().unwrap_or(&Value::Null))
            else {
                return failed("value is not a scalar".to_owned());
            };
            let left = match (&params.entity, trigger) {
                (Some(entity), _) => hub.entity_scalar(entity),
                (None, Some(trigger)) => hub.entity_scalar(&trigger.entity),
                (None, None) => return failed("no triggering entity".to_owned()),
            };
            match (&left, params.missing) {
                (None, Some(Missing::True)) => {
                    return (StepOutcome::Ok, "missing → true".to_owned(), Some("true"));
                }
                (None, Some(Missing::False)) => {
                    return (StepOutcome::Ok, "missing → false".to_owned(), Some("false"));
                }
                _ => {}
            }
            match compare(left.as_ref(), params.op, &right) {
                Ok(true) => (StepOutcome::Ok, "true".to_owned(), Some("true")),
                Ok(false) => (StepOutcome::Ok, "false".to_owned(), Some("false")),
                Err(err) => failed(err.to_string()),
            }
        }
        NodeKind::Delay(params) => match parse_duration(&params.for_) {
            Ok(duration) => {
                tokio::time::sleep(duration).await;
                (StepOutcome::Ok, format!("waited {}", params.for_), None)
            }
            Err(err) => failed(err.to_string()),
        },
        NodeKind::Command(params) => {
            match hub
                .send_command(&params.entity, params.command.clone())
                .await
            {
                Ok(response) if response.accepted => (StepOutcome::Ok, "accepted".to_owned(), None),
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
}
