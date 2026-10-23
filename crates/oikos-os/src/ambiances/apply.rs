//! Applying an ambiance: its actions run in order and each one reports what happened.

use std::sync::Arc;

use super::override_request;
use futures_util::future::join_all;
use oikos_api::{
    ActionReport, AmbianceAction, ApplyReport, CategoryAction, ClimateAction, ConnectionStatus,
    DeviceAction, EntityView, FlowAction, FlowDo, Outcome, Problem, WsEvent, already_there,
    entity_views, plan_category,
};
use oikos_climate::doc::ZoneState;

use crate::clock;
use crate::hub::{Hub, HubError};

const GONE: &str = "no longer exists";

/// Holds an ambiance id in `ambiances_running`; dropping it, on any exit, lets it run again.
struct RunLock {
    hub: Arc<Hub>,
    id: String,
}

impl RunLock {
    fn take(hub: &Arc<Hub>, id: &str) -> Option<Self> {
        let inserted = hub
            .ambiances_running
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(id.to_owned());
        inserted.then(|| Self {
            hub: Arc::clone(hub),
            id: id.to_owned(),
        })
    }
}

impl Drop for RunLock {
    fn drop(&mut self) {
        self.hub
            .ambiances_running
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(&self.id);
    }
}

/// What one action did, before it is folded into an [`ActionReport`].
#[derive(Default)]
struct Tally {
    total: usize,
    /// Targets that were done or already in the wanted state.
    ok: usize,
    failed: Vec<Problem>,
    skipped: Vec<Problem>,
    reason: Option<String>,
}

impl Tally {
    fn skipped_whole(total: usize, reason: &str) -> Self {
        Self {
            total,
            reason: Some(reason.to_owned()),
            ..Self::default()
        }
    }

    fn report(self, index: usize) -> ActionReport {
        let outcome = if !self.failed.is_empty() {
            Outcome::Failed
        } else if self.ok == 0 {
            Outcome::Skipped
        } else {
            Outcome::Ok
        };
        ActionReport {
            index,
            outcome,
            total: self.total,
            failed: self.failed,
            skipped: self.skipped,
            reason: self.reason,
        }
    }
}

fn problem(id: &str, reason: impl Into<String>) -> Problem {
    Problem {
        id: id.to_owned(),
        reason: reason.into(),
    }
}

impl Hub {
    /// Runs the ambiance's actions in order. A failing or skipped action never stops the run;
    /// the report says what each one did. Refused up front while the broker is down, for an
    /// unknown or invalid ambiance, and while the same ambiance is already running.
    pub async fn apply_ambiance(self: &Arc<Self>, id: &str) -> Result<ApplyReport, HubError> {
        if !matches!(*self.connection().borrow(), ConnectionStatus::Online) {
            return Err(HubError::Publish("broker offline".to_owned()));
        }
        let view = self
            .ambiance(id)
            .ok_or_else(|| HubError::UnknownAmbiance(id.to_owned()))?;
        let ambiance = match (view.ambiance, view.invalid) {
            (Some(ambiance), _) => ambiance,
            (None, reason) => {
                return Err(HubError::Rejected(
                    reason.unwrap_or_else(|| "invalid ambiance".to_owned()),
                ));
            }
        };
        let Some(_lock) = RunLock::take(self, id) else {
            return Err(HubError::Rejected("already running".to_owned()));
        };

        let started_at = clock::now();
        let mut actions = Vec::with_capacity(ambiance.actions.len());
        for (index, action) in ambiance.actions.iter().enumerate() {
            let tally = match action {
                AmbianceAction::Category(a) => self.apply_category(a).await,
                AmbianceAction::Device(a) => self.apply_device(a).await,
                AmbianceAction::Climate(a) => self.apply_climate(a).await,
                AmbianceAction::Flow(a) => self.apply_flow(a).await,
            };
            actions.push(tally.report(index));
        }
        let report = ApplyReport {
            ambiance: id.to_owned(),
            started_at,
            actions,
        };
        self.broadcast(WsEvent::AmbianceApplied {
            report: report.clone(),
        });
        Ok(report)
    }

    /// The house as it is right now; never held across an await.
    fn views_now(&self) -> Vec<EntityView> {
        entity_views(&self.read())
    }

    async fn apply_category(&self, action: &CategoryAction) -> Tally {
        let plan = plan_category(action, &self.views_now());
        let mut tally = Tally {
            total: plan.send.len() + plan.already.len() + plan.offline.len(),
            ok: plan.already.len(),
            skipped: plan
                .offline
                .iter()
                .map(|id| problem(id, "offline"))
                .collect(),
            ..Tally::default()
        };
        if tally.total == 0 {
            tally.reason = Some("no matching entities".to_owned());
        }
        let results = join_all(plan.send.into_iter().map(|(id, cmd)| async move {
            let result = self.send_command(&id, cmd).await;
            (id, result)
        }))
        .await;
        for (id, result) in results {
            match result {
                Ok(response) if response.accepted => tally.ok += 1,
                Ok(response) => tally.failed.push(problem(
                    &id,
                    response.reason.unwrap_or_else(|| "not accepted".to_owned()),
                )),
                Err(err) => tally.failed.push(problem(&id, err.to_string())),
            }
        }
        tally
    }

    async fn apply_device(&self, action: &DeviceAction) -> Tally {
        let views = self.views_now();
        let Some(view) = views.iter().find(|v| v.id == action.entity) else {
            return Tally::skipped_whole(1, GONE);
        };
        let mut tally = Tally {
            total: 1,
            ..Tally::default()
        };
        if view.state.availability == oikos_core::model::Availability::Offline {
            tally.skipped.push(problem(&action.entity, "offline"));
        } else if already_there(view, &action.command) {
            tally.ok = 1;
        } else {
            match self
                .send_command(&action.entity, action.command.clone())
                .await
            {
                Ok(response) if response.accepted => tally.ok = 1,
                Ok(response) => tally.failed.push(problem(
                    &action.entity,
                    response.reason.unwrap_or_else(|| "not accepted".to_owned()),
                )),
                Err(err) => tally.failed.push(problem(&action.entity, err.to_string())),
            }
        }
        tally
    }

    async fn apply_climate(self: &Arc<Self>, action: &ClimateAction) -> Tally {
        if !self.climate_enabled() {
            return Tally::skipped_whole(0, "the climate engine is off");
        }
        let known = self.climate_zone_ids();
        let zones: Vec<String> = match &action.zone {
            Some(zone) if known.contains(zone) => vec![zone.clone()],
            Some(_) => return Tally::skipped_whole(1, GONE),
            None => known
                .into_iter()
                .filter(|zone| self.zone_settings(zone).state == ZoneState::Active)
                .collect(),
        };
        let mut tally = Tally {
            total: zones.len(),
            ..Tally::default()
        };
        if zones.is_empty() {
            tally.reason = Some("no active zone".to_owned());
            return tally;
        }
        // The override is the same for every zone; only its end is fixed per call.
        let request = (!action.clear).then(|| override_request(action));
        for zone in &zones {
            let result = match &request {
                None => self.clear_climate_override(zone).await,
                Some(Err(err)) => Err(HubError::Rejected(err.to_string())),
                Some(Ok(request)) => match request.clone().into_override(clock::now()) {
                    Ok(ov) => self.set_climate_override(zone, ov).await,
                    Err(why) => Err(HubError::Rejected(why)),
                },
            };
            match result {
                Ok(()) => tally.ok += 1,
                Err(err) => tally.failed.push(problem(zone, err.to_string())),
            }
        }
        tally
    }

    async fn apply_flow(&self, action: &FlowAction) -> Tally {
        if !self.flow_store().exists(&action.flow) {
            return Tally::skipped_whole(1, GONE);
        }
        let result = match action.action {
            FlowDo::Enable => self.set_flow_enabled(&action.flow, true).await.map(drop),
            FlowDo::Disable => self.set_flow_enabled(&action.flow, false).await.map(drop),
            FlowDo::Run => self.run_flow(&action.flow).map(drop),
        };
        let mut tally = Tally {
            total: 1,
            ..Tally::default()
        };
        match result {
            Ok(()) => tally.ok = 1,
            Err(err) => tally.failed.push(problem(&action.flow, err.to_string())),
        }
        tally
    }
}
