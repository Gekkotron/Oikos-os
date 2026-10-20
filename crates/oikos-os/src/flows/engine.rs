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
use oikos_core::flow::{Flow, Node, NodeKind, Scalar, compare, parse_duration};
use serde_json::Value;
use tokio::sync::{broadcast, mpsc};
use tokio::task::{AbortHandle, JoinSet};

use super::EngineRequest;
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
        hub,
        flows: Vec::new(),
        known: BTreeMap::new(),
        watched: BTreeSet::new(),
        active: BTreeMap::new(),
        tasks: JoinSet::new(),
    };
    engine.reload();
    loop {
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

struct Engine {
    hub: Arc<Hub>,
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
    tasks: JoinSet<Finished>,
}

impl Engine {
    fn handle_request(&mut self, request: EngineRequest) {
        match request {
            EngineRequest::Reload => self.reload(),
            EngineRequest::Run { flow_id, run_id } => self.run_manual(&flow_id, run_id),
        }
    }

    /// Re-reads the definitions, seeds the values of newly watched entities from the hub (so a
    /// flow written while the entity already has a value fires on its very next change) and
    /// cancels runs whose flow was removed, disabled or edited.
    fn reload(&mut self) {
        self.flows = self
            .hub
            .flow_definitions()
            .into_iter()
            .filter(|flow| flow.enabled)
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
            .filter(|(id, active)| !self.flows.iter().any(|f| f.id == **id && *f == active.flow))
            .map(|(id, _)| id.clone())
            .collect();
        for id in stale {
            self.cancel(&id);
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
            // The broker is about to replay retained states: forget what we knew so the replay
            // is only recorded, never acted on.
            WsEvent::ConnectionChanged(status) if status != ConnectionStatus::Online => {
                self.known.clear();
            }
            _ => {}
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
            self.fire(flow, node, trigger.clone());
        }
    }

    /// Starts a run for a satisfied trigger, honouring the flow's mode when one is active.
    fn fire(&mut self, flow: Flow, node_id: String, trigger: TriggerInfo) {
        if self.active.contains_key(&flow.id) {
            match flow.mode {
                oikos_core::flow::FlowMode::Single => {
                    self.hub.add_skipped_run(&flow.id, &node_id, Some(trigger));
                    return;
                }
                oikos_core::flow::FlowMode::Restart => self.cancel(&flow.id),
            }
        }
        let run = match self.hub.begin_run(&flow.id, Some(trigger.clone())) {
            Ok(run) => run,
            Err(err) => {
                tracing::warn!(flow = %flow.id, error = %err, "could not begin a run");
                return;
            }
        };
        self.spawn(flow, run.id, node_id, Some(trigger));
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
        let (flow_id, task_flow, task_run) = (flow.id.clone(), flow.clone(), run_id.clone());
        let abort = self.tasks.spawn(async move {
            let status = execute(hub, task_flow, task_run.clone(), start, trigger).await;
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
        let (outcome, detail, port) = run_node(&hub, node, trigger.as_ref()).await;
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
        // Arming schedules is the engine's next step; a schedule step just passes.
        NodeKind::TriggerSchedule(_) => (StepOutcome::Ok, "schedule".to_owned(), None),
        // Task 5 gives flow.enable its run-time behaviour; for now it just passes.
        NodeKind::FlowEnable(_) => (StepOutcome::Ok, "flow.enable".to_owned(), None),
        NodeKind::If(params) => {
            let Some(right) = Scalar::from_json(&params.value) else {
                return failed("value is not a scalar".to_owned());
            };
            let left = match (&params.entity, trigger) {
                (Some(entity), _) => hub.entity_scalar(entity),
                (None, Some(trigger)) => hub.entity_scalar(&trigger.entity),
                (None, None) => return failed("no triggering entity".to_owned()),
            };
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
