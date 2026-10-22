//! The climate side of the hub: the read view and the writers behind the REST and MCP layers.

use std::collections::BTreeMap;
use std::sync::MutexGuard;

use oikos_api::WsEvent;
use oikos_climate::doc::{Decision, HouseSettings, ZoneSettings, ZoneState};
use oikos_climate::topics::{self, ClimateTopic, ZoneDoc};
use oikos_climate::view::{ClimateView, DecisionLog, DecisionRow, LearnStatus, ZoneView};
use oikos_core::command::Publish;
use oikos_core::model::{Kind, UNASSIGNED_AREA};
use serde::Serialize;

use super::inputs;
use super::store::{Applied, ClimateStore, ZoneDocs};
use super::{ClimateRuntime, GROUP_PREFIX};
use crate::clock;
use crate::hub::{Hub, HubError};

impl Hub {
    /// Whether `[climate] enabled` is on.
    pub fn climate_enabled(&self) -> bool {
        self.climate.is_some()
    }

    /// The topic prefix of the climate documents, `None` while the engine is off.
    pub(crate) fn climate_prefix(&self) -> Option<&str> {
        self.climate.as_ref().map(|c| c.prefix.as_str())
    }

    /// The retained documents. Callers have checked [`Hub::climate_enabled`] (or went through
    /// `require_climate`); the store only exists while the engine is on.
    pub(crate) fn climate_store(&self) -> MutexGuard<'_, ClimateStore> {
        self.climate
            .as_ref()
            .expect("climate_store is only used while [climate] is enabled")
            .store
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// The engine's runtime state; same rules as [`Hub::climate_store`], and never held across
    /// an `await`.
    pub(crate) fn climate_runtime(&self) -> MutexGuard<'_, ClimateRuntime> {
        self.climate
            .as_ref()
            .expect("climate_runtime is only used while [climate] is enabled")
            .runtime
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// The whole engine as one document: the settings, and one entry per zone.
    pub fn climate_view(&self) -> Result<ClimateView, HubError> {
        self.require_climate()?;
        let areas = self.climate_areas();
        let history = self.history_configured();
        let now = clock::now();
        let (seen, learn) = {
            let rt = self.climate_runtime();
            (rt.seen_at.clone(), rt.learn_status.clone())
        };
        let mut view = {
            let store = self.climate_store();
            let mut zones: Vec<ZoneView> = areas
                .iter()
                .map(|(zone, (name, devices))| {
                    let docs = store.zones.get(zone);
                    zone_view(
                        zone,
                        name,
                        false,
                        devices.clone(),
                        docs,
                        history,
                        learn.get(zone),
                    )
                })
                .collect();
            for (zone, docs) in &store.zones {
                if !areas.contains_key(zone) {
                    let devices = default_entities(&docs.settings, &[]);
                    zones.push(zone_view(
                        zone,
                        zone,
                        true,
                        devices,
                        Some(docs),
                        history,
                        learn.get(zone),
                    ));
                }
            }
            zones.sort_by(|a, b| a.orphan.cmp(&b.orphan).then_with(|| a.name.cmp(&b.name)));
            ClimateView {
                enabled: true,
                house: store.house.clone(),
                house_invalid: store.house_invalid.clone(),
                history,
                timezone: self.zone().name().to_owned(),
                zones,
            }
        };
        // The reading and its age are what the engine sees now, not what the last decision froze.
        let stale_after_s = u64::from(view.house.stale_after_min) * 60;
        for zone in &mut view.zones {
            let (reading, age) = inputs::view_reading(
                self,
                &zone.settings,
                &zone.entities,
                stale_after_s,
                now,
                &seen,
            );
            zone.reading = reading;
            zone.reading_age_s = age;
        }
        Ok(view)
    }

    /// The latest decisions of `zone` (at most `limit`, newest first) from memory, then older
    /// rows from the history database when it is on (none when it is off or unreachable).
    pub async fn climate_decision_log(
        &self,
        zone: &str,
        limit: usize,
    ) -> Result<DecisionLog, HubError> {
        self.require_climate()?;
        if !self.climate_zone_ids().iter().any(|z| z == zone) {
            return Err(HubError::UnknownArea(zone.to_owned()));
        }
        let recent: Vec<Decision> = self
            .climate_runtime()
            .decisions
            .get(zone)
            .map(|log| log.iter().rev().take(limit).cloned().collect())
            .unwrap_or_default();
        // Rows of `recent` are in the database too: take only what is older than them.
        let before = recent.last().map(|d| {
            i64::try_from(d.at)
                .unwrap_or(i64::MAX)
                .saturating_mul(1_000_000)
        });
        let mut older = Vec::new();
        if let Some(history) = self.history().cloned() {
            match history.db() {
                Ok(db) => match db
                    .climate_decisions(zone, before, i64::try_from(limit).unwrap_or(i64::MAX))
                    .await
                {
                    Ok(rows) => {
                        older = rows
                            .into_iter()
                            .map(|row| DecisionRow {
                                at: u64::try_from(row.at / 1_000_000).unwrap_or(0),
                                entity_id: row.entity_id,
                                mode: row.mode,
                                setpoint: row.setpoint,
                                source: row.source,
                                reason: row.reason,
                                applied: row.applied,
                            })
                            .collect();
                    }
                    Err(err) => tracing::warn!(%zone, error = %err, "climate decisions not read"),
                },
                Err(err) => tracing::warn!(%zone, error = %err, "climate history unavailable"),
            }
        }
        Ok(DecisionLog { recent, older })
    }

    /// Replaces the house settings: validates, publishes retained and applies locally.
    pub fn set_climate_house(&self, house: HouseSettings) -> Result<(), HubError> {
        let prefix = self.require_climate()?.prefix.clone();
        house.validate().map_err(HubError::Rejected)?;
        self.publish_climate(topics::house_topic(&prefix), Some(&house))
    }

    /// Replaces the settings of `zone`, an area with climate entities: validates, publishes
    /// retained and applies locally.
    pub fn set_climate_zone(&self, zone: &str, settings: ZoneSettings) -> Result<(), HubError> {
        let prefix = self.require_climate()?.prefix.clone();
        if zone.is_empty() || zone.contains(['/', '+', '#']) {
            return Err(HubError::Rejected(format!("invalid zone id `{zone}`")));
        }
        settings.validate().map_err(HubError::Rejected)?;
        let known = zone != UNASSIGNED_AREA
            && self
                .read()
                .effective_areas()
                .iter()
                .any(|area| area.id == zone);
        if !known {
            return Err(HubError::UnknownArea(zone.to_owned()));
        }
        self.publish_climate(
            topics::zone_topic(&prefix, zone, ZoneDoc::Settings),
            Some(&settings),
        )
    }

    /// Publishes one climate document retained (`None` publishes the empty payload that deletes
    /// it), applies it locally so the next read sees it before the broker echo, then tells the
    /// UI.
    pub(crate) fn publish_climate(
        &self,
        topic: String,
        value: Option<&impl Serialize>,
    ) -> Result<(), HubError> {
        let payload = match value {
            Some(value) => serde_json::to_string(value)
                .map_err(|err| HubError::Rejected(format!("serialize: {err}")))?,
            None => String::new(),
        };
        self.publish(Publish {
            topic: topic.clone(),
            payload: payload.clone(),
            retain: true,
        })?;
        self.ingest_climate(&topic, payload.as_bytes());
        Ok(())
    }

    /// Feeds a message to the climate store when `topic` is one of its documents; false when it
    /// is not, so the message goes on to the registry.
    pub(crate) fn ingest_climate(&self, topic: &str, payload: &[u8]) -> bool {
        let Some(prefix) = self.climate_prefix() else {
            return false;
        };
        let Some(parsed) = topics::parse(prefix, topic) else {
            return false;
        };
        let was_off = matches!(
            &parsed,
            ClimateTopic::Zone { zone, doc: ZoneDoc::Settings }
                if self.climate_store().zones.get(zone)
                    .is_some_and(|docs| docs.settings.state == ZoneState::Off)
        );
        let applied = self.climate_store().apply(&parsed, payload);
        if was_off
            && let ClimateTopic::Zone { zone, .. } = &parsed
            && self.zone_settings(zone).state != ZoneState::Off
            && let Some(hub) = self.me.upgrade()
        {
            // "On enable": a zone that just left `off` is learned at once.
            super::learning::spawn_zone_learning(&hub, zone);
        }
        match &applied {
            Applied::Changed => {}
            Applied::Invalid(reason) => {
                tracing::warn!(%topic, %reason, "climate document refused, keeping the previous one");
            }
            Applied::Unchanged => return true,
        }
        let zone = match parsed {
            ClimateTopic::House => None,
            ClimateTopic::Zone { zone, .. } => Some(zone),
        };
        self.broadcast(WsEvent::ClimateChanged { zone });
        true
    }

    /// The areas that can be zones, by id: each with its name and the climate entities it holds.
    pub(crate) fn climate_areas(&self) -> BTreeMap<String, (String, Vec<String>)> {
        let reg = self.read();
        let mut out: BTreeMap<String, (String, Vec<String>)> = BTreeMap::new();
        for entity in reg.entities() {
            if entity.kind != Kind::Climate
                || entity.unique_id.starts_with(GROUP_PREFIX)
                || entity.area == UNASSIGNED_AREA
            {
                continue;
            }
            let area = &entity.area;
            let name = reg
                .effective_areas()
                .into_iter()
                .find(|a| a.id == *area)
                .map_or_else(|| area.clone(), |a| a.name);
            out.entry(area.clone())
                .or_insert_with(|| (name, Vec::new()))
                .1
                .push(entity.unique_id.clone());
        }
        for (_, devices) in out.values_mut() {
            devices.sort();
        }
        out
    }

    fn history_configured(&self) -> bool {
        self.history().is_some()
    }
}

/// `settings.entities`, or the zone's climate entities.
fn default_entities(settings: &ZoneSettings, area_entities: &[String]) -> Vec<String> {
    settings
        .entities
        .clone()
        .unwrap_or_else(|| area_entities.to_vec())
}

fn zone_view(
    zone: &str,
    name: &str,
    orphan: bool,
    area_entities: Vec<String>,
    docs: Option<&ZoneDocs>,
    history: bool,
    ran: Option<&LearnStatus>,
) -> ZoneView {
    let empty = ZoneDocs::default();
    let docs = docs.unwrap_or(&empty);
    let learn = if docs.settings.state == ZoneState::Off {
        LearnStatus::Off
    } else if docs.proposal.is_some() {
        LearnStatus::Proposed
    } else if !history {
        LearnStatus::HistoryOff
    } else {
        match ran {
            Some(
                status @ (LearnStatus::NotEnough { .. }
                | LearnStatus::Rejected
                | LearnStatus::HistoryOff),
            ) => status.clone(),
            _ => LearnStatus::UpToDate,
        }
    };
    ZoneView {
        zone: zone.to_owned(),
        name: name.to_owned(),
        orphan,
        entities: default_entities(&docs.settings, &area_entities),
        settings: docs.settings.clone(),
        settings_invalid: docs.settings_invalid.clone(),
        reading: None,
        reading_age_s: None,
        decision: docs.decision.clone(),
        overrides: docs.overrides.clone(),
        proposal: docs.proposal.clone(),
        learn,
    }
}
