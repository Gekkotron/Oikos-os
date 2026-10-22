//! The read view of the engine, as the hub serves it and the web Climate screen renders it.

use serde::{Deserialize, Serialize};

use crate::doc::{
    Decision, HouseSettings, Mode, Override, OverrideOrigin, Proposal, Setpoint, ZoneSettings,
};

/// Where the learner stands for one zone.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum LearnStatus {
    /// The zone is off, or not learning.
    Off,
    /// No history database is configured or reachable.
    HistoryOff,
    /// Fewer usable days than needed.
    NotEnough { days: u32, needed: u32 },
    /// The latest run found nothing to change.
    UpToDate,
    /// A proposal awaits approval.
    Proposed,
    /// The latest proposal was rejected and is being remembered.
    Rejected,
}

/// One zone of the house: its settings, live reading, decision and pending proposal.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct ZoneView {
    pub zone: String,
    pub name: String,
    /// Settings exist for this zone but no area has climate entities any more.
    pub orphan: bool,
    pub settings: ZoneSettings,
    pub settings_invalid: Option<String>,
    pub entities: Vec<String>,
    pub reading: Option<f64>,
    pub reading_age_s: Option<u64>,
    pub decision: Option<Decision>,
    pub overrides: Vec<Override>,
    pub proposal: Option<Proposal>,
    pub learn: LearnStatus,
}

/// The whole engine as one document.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct ClimateView {
    pub enabled: bool,
    pub house: HouseSettings,
    pub house_invalid: Option<String>,
    /// Whether the history database is configured.
    pub history: bool,
    /// The house time zone as an IANA name, for the clock times the UI shows.
    pub timezone: String,
    pub zones: Vec<ZoneView>,
}

/// One decision as the history database keeps it, one row per device.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct DecisionRow {
    /// Unix seconds.
    pub at: u64,
    pub entity_id: String,
    pub mode: String,
    pub setpoint: Option<f64>,
    pub source: String,
    pub reason: String,
    pub applied: bool,
}

/// The decisions of one zone: the recent ones the hub holds in memory, then older rows from
/// the history database when it is configured.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct DecisionLog {
    /// Newest first.
    pub recent: Vec<Decision>,
    /// Newest first; empty without history.
    pub older: Vec<DecisionRow>,
}

/// A temporary override of a zone as a person asks for it: a target and/or a mode, held for
/// `minutes` from now or until a Unix time. Exactly one of `minutes` and `until`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct OverrideRequest {
    /// A temperature in degrees Celsius, or `"off"`.
    #[serde(default)]
    pub setpoint: Option<Setpoint>,
    /// `heat` or `cool`.
    #[serde(default)]
    pub mode: Option<Mode>,
    /// Hold for this many minutes from now.
    #[serde(default)]
    pub minutes: Option<u32>,
    /// Hold until this Unix time, in seconds.
    #[serde(default)]
    pub until: Option<u64>,
}

impl OverrideRequest {
    /// The zone-wide override this asks for, held by a person (`OverrideOrigin::User`).
    pub fn into_override(self, now: u64) -> Result<Override, String> {
        if self.setpoint.is_none() && self.mode.is_none() {
            return Err("give a setpoint, a mode or both".to_owned());
        }
        let until = match (self.minutes, self.until) {
            (Some(minutes), None) if minutes > 0 => now + u64::from(minutes) * 60,
            (None, Some(until)) if until > now => until,
            (None, Some(_)) => return Err("until is already past".to_owned()),
            (Some(_), None) => return Err("minutes must be above 0".to_owned()),
            _ => return Err("give exactly one of minutes and until".to_owned()),
        };
        Ok(Override {
            entity: None,
            setpoint: self.setpoint,
            mode: self.mode,
            until,
            origin: OverrideOrigin::User,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn override_request_needs_one_duration_and_one_target() {
        let base = OverrideRequest {
            setpoint: Some(Setpoint::Temp(21.0)),
            ..OverrideRequest::default()
        };
        let both = OverrideRequest {
            minutes: Some(30),
            until: Some(99),
            ..base.clone()
        };
        assert!(both.into_override(0).is_err());
        assert!(base.clone().into_override(0).is_err());
        let timed = OverrideRequest {
            minutes: Some(30),
            ..base.clone()
        };
        assert_eq!(timed.into_override(100).expect("ok").until, 1900);
        let past = OverrideRequest {
            until: Some(100),
            ..base.clone()
        };
        assert!(past.clone().into_override(100).is_err());
        assert_eq!(past.into_override(99).expect("ok").until, 100);
        let bare = OverrideRequest {
            minutes: Some(30),
            ..OverrideRequest::default()
        };
        assert!(bare.into_override(0).is_err());
    }
}
