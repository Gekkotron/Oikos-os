//! Wire types for the onboarding wizard: where the house is and how far setup got.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// The house's town, geocoded once at setup and stored as plain numbers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Location {
    pub city: String,
    pub country: Option<String>,
    pub latitude: f64,
    pub longitude: f64,
    /// IANA timezone name.
    pub timezone: String,
}

/// The wizard step the browser resumes at.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SetupStep {
    #[default]
    Welcome,
    Mqtt,
    Database,
    Devices,
    Services,
    Done,
}
