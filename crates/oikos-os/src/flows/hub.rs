//! The flow side of the hub: ingesting retained definitions, the writers behind the REST and
//! MCP flow tools, and the run bookkeeping the engine reports through.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use tokio::task::JoinHandle;

use oikos_api::{ConnectionStatus, FlowView, RunStatus, RunView, StepView, TriggerInfo, WsEvent};
use oikos_core::command::{Publish, map_command};
use oikos_core::flow::{Flow, NodeKind, Scalar, validate};
use oikos_core::model::{Command, Kind};

use super::{EngineRequest, StoreApplied};
use crate::clock;
use crate::hub::{Hub, HubError};

impl Hub {
    /// Applies a retained flow payload. A change (or removal) is broadcast and tells the engine
    /// to reload; an identical payload or the removal of an unknown flow does nothing.
    pub(crate) fn ingest_flow(&self, id: &str, payload: &[u8]) -> bool {
        let applied = self.flow_store().apply(id, payload);
        match applied {
            StoreApplied::Changed | StoreApplied::Removed => {
                self.broadcast(WsEvent::FlowsChanged {
                    flows: self.flows(),
                });
                self.send_engine(EngineRequest::Reload);
                true
            }
            StoreApplied::Unchanged | StoreApplied::NotFound => false,
        }
    }

    /// The `command_topic` of every button some enabled flow triggers on, with its entity id.
    /// A press is a message on this topic; the hub subscribes to it only while watched.
    pub(crate) fn watched_button_topics(&self) -> BTreeMap<String, Vec<String>> {
        let mut watched: BTreeSet<String> = BTreeSet::new();
        for flow in self.flow_definitions().iter().filter(|flow| flow.enabled) {
            for node in flow.triggers() {
                if let NodeKind::TriggerState(params) = &node.kind {
                    watched.insert(params.entity.clone());
                }
            }
        }
        let reg = self.read();
        let mut topics: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for id in watched {
            let Some(entity) = reg.entity(&id).filter(|e| e.kind == Kind::Button) else {
                continue;
            };
            if let Some(topic) = entity
                .bindings
                .config
                .get("command_topic")
                .and_then(serde_json::Value::as_str)
            {
                topics.entry(topic.to_owned()).or_default().push(id);
            }
        }
        topics
    }

    /// Every flow with its runs, valid or not, sorted by id.
    pub fn flows(&self) -> Vec<FlowView> {
        self.flow_store().views()
    }

    /// One flow with its runs.
    pub fn flow(&self, id: &str) -> Option<FlowView> {
        self.flow_store().get(id)
    }

    /// The valid definitions, enabled or not; what the engine arms from.
    pub fn flow_definitions(&self) -> Vec<Flow> {
        self.flow_store().flows()
    }

    /// Validates `flow` against the house, publishes it retained and applies it locally so the
    /// returned view already reflects it. Every entity a node names must exist and every
    /// `command` node must map for that entity, so a flow that could never work is refused now
    /// rather than failing at 3 a.m.
    pub async fn set_flow(&self, flow: Flow) -> Result<FlowView, HubError> {
        validate(&flow).map_err(|err| HubError::InvalidFlow(err.to_string()))?;
        self.check_house(&flow)?;
        self.write_flow(&flow)
    }

    /// Removes a flow, valid or not: publishes an empty retained payload and applies it.
    pub async fn delete_flow(&self, id: &str) -> Result<(), HubError> {
        if !self.flow_store().exists(id) {
            return Err(HubError::UnknownFlow(id.to_owned()));
        }
        let topic = self.flow_topic(id);
        self.publish(Publish {
            topic: topic.clone(),
            payload: String::new(),
            retain: true,
        })?;
        self.ingest_flow(id, b"");
        Ok(())
    }

    /// Flips `enabled`. The house checks are skipped on purpose: an entity may have vanished
    /// since the flow was saved, and switching the flow off must still work.
    pub async fn set_flow_enabled(&self, id: &str, enabled: bool) -> Result<FlowView, HubError> {
        let view = self
            .flow(id)
            .ok_or_else(|| HubError::UnknownFlow(id.to_owned()))?;
        if let Some(reason) = view.invalid {
            return Err(HubError::InvalidFlow(reason));
        }
        let mut flow = view.definition;
        flow.enabled = enabled;
        self.write_flow(&flow)
    }

    /// [`Self::set_flow_enabled`] for a `flow.enable` node of run `run_id`. A run disabling its
    /// own flow is announced to the engine first (before the write's reload) so it finishes
    /// its remaining steps instead of being cancelled with the flow.
    pub(crate) async fn set_flow_enabled_by_run(
        &self,
        id: &str,
        enabled: bool,
        run_id: &str,
    ) -> Result<FlowView, HubError> {
        if !enabled && self.flow_store().active_run(id).as_deref() == Some(run_id) {
            self.send_engine(EngineRequest::KeepRun {
                flow_id: id.to_owned(),
                run_id: run_id.to_owned(),
            });
        }
        self.set_flow_enabled(id, enabled).await
    }

    /// Starts a manual run: creates it and asks the engine to execute it. Refused while the broker
    /// link is down, like the other writers: the run's commands could not be published.
    pub fn run_flow(&self, id: &str) -> Result<RunView, HubError> {
        if !matches!(*self.connection().borrow(), ConnectionStatus::Online) {
            return Err(HubError::Publish("broker offline".to_owned()));
        }
        let run = {
            let mut store = self.flow_store();
            let view = store
                .get(id)
                .ok_or_else(|| HubError::UnknownFlow(id.to_owned()))?;
            if let Some(reason) = view.invalid {
                return Err(HubError::InvalidFlow(reason));
            }
            if !view.definition.enabled {
                return Err(HubError::Rejected("flow is disabled".to_owned()));
            }
            if store.active_run(id).is_some() {
                return Err(HubError::Rejected("a run is active".to_owned()));
            }
            store
                .begin_run(id, None, clock::now())
                .ok_or_else(|| HubError::UnknownFlow(id.to_owned()))?
        };
        self.broadcast_run(id, &run);
        self.send_engine(EngineRequest::Run {
            flow_id: id.to_owned(),
            run_id: run.id.clone(),
        });
        Ok(run)
    }

    /// Starts a `Running` run for the engine (a trigger fired).
    pub fn begin_run(
        &self,
        flow_id: &str,
        trigger: Option<TriggerInfo>,
    ) -> Result<RunView, HubError> {
        let run = self
            .flow_store()
            .begin_run(flow_id, trigger, clock::now())
            .ok_or_else(|| HubError::UnknownFlow(flow_id.to_owned()))?;
        self.broadcast_run(flow_id, &run);
        Ok(run)
    }

    /// Records a trigger that was dropped because a run was active.
    pub fn add_skipped_run(
        &self,
        flow_id: &str,
        trigger_node: &str,
        trigger: Option<TriggerInfo>,
    ) -> Option<RunView> {
        let run =
            self.flow_store()
                .add_skipped_run(flow_id, trigger_node, trigger, clock::now())?;
        self.broadcast_run(flow_id, &run);
        Some(run)
    }

    /// Records a scheduled tick that could not run, with the reason.
    pub fn add_skipped_run_because(
        &self,
        flow_id: &str,
        trigger_node: &str,
        reason: &str,
    ) -> Option<RunView> {
        let run = self.flow_store().add_skipped_run_because(
            flow_id,
            trigger_node,
            None,
            clock::now(),
            reason,
        )?;
        self.broadcast_run(flow_id, &run);
        Some(run)
    }

    /// Appends a step to a run, but only while it is the flow's active run (checked under the
    /// store lock). Returns whether it was: `false` means the run was cancelled, ended or its
    /// flow deleted, and the caller must stop acting.
    pub fn record_step(&self, flow_id: &str, run_id: &str, step: StepView) -> bool {
        let run = {
            let mut store = self.flow_store();
            if store.active_run(flow_id).as_deref() != Some(run_id) {
                return false;
            }
            store.record_step(flow_id, run_id, step)
        };
        match run {
            Some(run) => {
                self.broadcast_run(flow_id, &run);
                true
            }
            None => false,
        }
    }

    /// Finishes a run.
    pub fn end_run(&self, flow_id: &str, run_id: &str, status: RunStatus) {
        let run = self
            .flow_store()
            .end_run(flow_id, run_id, status, clock::now());
        if let Some(run) = run {
            self.broadcast_run(flow_id, &run);
        }
    }

    /// The headline value of an entity, `None` for an unknown entity or an absent value.
    pub fn entity_scalar(&self, id: &str) -> Option<Scalar> {
        let reg = self.read();
        reg.entity(id)?;
        Scalar::of_state(&reg.state(id)?.value)
    }

    fn broadcast_run(&self, flow_id: &str, run: &RunView) {
        self.broadcast(WsEvent::FlowRunChanged {
            flow_id: flow_id.to_owned(),
            run: run.clone(),
        });
    }

    /// Checks every entity a node names exists and every command maps; publishes nothing.
    fn check_house(&self, flow: &Flow) -> Result<(), HubError> {
        let reg = self.read();
        for node in &flow.nodes {
            let entity = match &node.kind {
                NodeKind::TriggerState(params) => Some(&params.entity),
                NodeKind::If(params) => params.entity.as_ref(),
                NodeKind::Command(params) => Some(&params.entity),
                NodeKind::FlowEnable(params) => {
                    if let Some(target) = &params.flow
                        && *target != flow.id
                        && !self.flow_store().exists(target)
                    {
                        return Err(HubError::InvalidFlow(format!(
                            "unknown flow `{target}` in node `{}`",
                            node.id
                        )));
                    }
                    None
                }
                NodeKind::Delay(_) | NodeKind::TriggerSchedule(_) => None,
            };
            let Some(entity) = entity else { continue };
            let Some(found) = reg.entity(entity) else {
                return Err(HubError::InvalidFlow(format!(
                    "unknown entity `{entity}` in node `{}`",
                    node.id
                )));
            };
            if let NodeKind::TriggerState(params) = &node.kind
                && found.kind == Kind::Button
                && [&params.to, &params.from]
                    .into_iter()
                    .any(|filter| filter.as_ref().is_some_and(|v| !v.is_null()))
            {
                return Err(HubError::InvalidFlow(format!(
                    "node `{}`: a button trigger takes no to/from",
                    node.id
                )));
            }
            if let NodeKind::Command(params) = &node.kind {
                if matches!(
                    params.command,
                    Command::Arm { code: Some(_), .. } | Command::Disarm { code: Some(_) }
                ) {
                    return Err(HubError::InvalidFlow(format!(
                        "node `{}`: alarm codes are not allowed in flows (they would be stored in clear on the broker)",
                        node.id
                    )));
                }
                let state = reg.state(entity).cloned().unwrap_or_default();
                map_command(found, &state, &params.command, reg.templates())
                    .map_err(|err| HubError::InvalidFlow(format!("node `{}`: {err}", node.id)))?;
            }
        }
        Ok(())
    }

    /// Publishes the definition retained and applies it locally (the broker's echo is then a
    /// no-op); the registry lock is not held while publishing.
    fn write_flow(&self, flow: &Flow) -> Result<FlowView, HubError> {
        let payload = flow.encode();
        self.publish(Publish {
            topic: self.flow_topic(&flow.id),
            payload: payload.clone(),
            retain: true,
        })?;
        self.ingest_flow(&flow.id, payload.as_bytes());
        self.flow(&flow.id)
            .ok_or_else(|| HubError::UnknownFlow(flow.id.clone()))
    }
}

impl Hub {
    /// Starts the flow engine on the runtime. The event subscription is taken before the task
    /// spawns so nothing ingested right after this call is missed. A second call logs a warning
    /// and returns an already finished handle: the request receiver can be taken only once.
    pub fn start_engine(self: &Arc<Self>) -> JoinHandle<()> {
        let Some(requests) = self.take_engine_requests() else {
            tracing::warn!("the flow engine was already started");
            return tokio::spawn(async {});
        };
        let events = self.subscribe();
        let seen = self.subscribe_state_seen();
        tokio::spawn(super::engine::run_with(
            Arc::clone(self),
            events,
            seen,
            requests,
        ))
    }
}
