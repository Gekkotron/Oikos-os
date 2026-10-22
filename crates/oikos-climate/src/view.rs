//! The read view of the engine, as the hub serves it and the web Climate screen renders it.

use serde::{Deserialize, Serialize};

use crate::doc::{Decision, HouseSettings, Override, Proposal, ZoneSettings};

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
    pub zones: Vec<ZoneView>,
}
