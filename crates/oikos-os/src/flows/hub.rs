//! The flow side of the hub: ingesting retained definitions, the writers behind the REST and
//! MCP flow tools, and the run bookkeeping the engine reports through.

use std::sync::Arc;

use tokio::task::JoinHandle;

use oikos_api::{FlowView, RunStatus, RunView, StepView, TriggerInfo, WsEvent};
use oikos_core::command::{Publish, map_command};
use oikos_core::flow::{Flow, NodeKind, Scalar, validate};

use super::{EngineRequest, StoreApplied};
use crate::clock;
use crate::hub::{Hub, HubError};

impl Hub {
    /// Applies a retained flow payload. A change (or removal) is broadcast and tells the engine
    /// to reload; an identical payload or the removal of an unknown flow does nothing.
    pub(crate) fn ingest_flow(&self, id: &str, payload: &[u8]) {
        let applied = self.flow_store().apply(id, payload);
        match applied {
            StoreApplied::Changed | StoreApplied::Removed => {
                self.broadcast(WsEvent::FlowsChanged {
                    flows: self.flows(),
                });
                self.send_engine(EngineRequest::Reload);
            }
            StoreApplied::Unchanged | StoreApplied::NotFound => {}
        }
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

    /// Starts a manual run: creates it and asks the engine to execute it.
    pub fn run_flow(&self, id: &str) -> Result<RunView, HubError> {
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

    /// Appends a step to a run.
    pub fn record_step(&self, flow_id: &str, run_id: &str, step: StepView) {
        let run = self.flow_store().record_step(flow_id, run_id, step);
        if let Some(run) = run {
            self.broadcast_run(flow_id, &run);
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
                NodeKind::Delay(_) => None,
            };
            let Some(entity) = entity else { continue };
            let Some(found) = reg.entity(entity) else {
                return Err(HubError::InvalidFlow(format!(
                    "unknown entity `{entity}` in node `{}`",
                    node.id
                )));
            };
            if let NodeKind::Command(params) = &node.kind {
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
        tokio::spawn(super::engine::run_with(Arc::clone(self), events, requests))
    }
}
