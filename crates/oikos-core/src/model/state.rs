//! The current state of an entity, as the app's `StateValue`.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::command::Hs;

/// Whether the entity (or the bridge) is reachable.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum Availability {
    Online,
    Offline,
    #[default]
    Unknown,
}

/// A light's state; `brightness` is normalised to 0..=255.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct LightState {
    pub on: bool,
    pub brightness: Option<u8>,
    pub hs: Option<Hs>,
    pub color_temp: Option<u32>,
    pub effect: Option<String>,
}

/// A cover's state; `position` is normalised to 0 (closed) ..= 100 (open).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct CoverState {
    pub state: Option<String>,
    pub position: Option<u8>,
}

/// A climate's state.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct ClimateState {
    pub current: Option<f64>,
    pub target: Option<f64>,
    pub mode: Option<String>,
    pub fan_mode: Option<String>,
    pub action: Option<String>,
}

/// A vacuum's state; `battery` is 0..=100.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct VacuumState {
    pub state: Option<String>,
    pub battery: Option<u8>,
    pub fan_speed: Option<String>,
}

/// A water heater's state.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct WaterHeaterState {
    pub mode: Option<String>,
    pub current: Option<f64>,
    pub target: Option<f64>,
}

/// The typed value of an entity, by kind.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum StateValue {
    OnOff(bool),
    Number(f64),
    Text(String),
    Light(LightState),
    Cover(CoverState),
    Climate(ClimateState),
    Alarm(String),
    Vacuum(VacuumState),
    WaterHeater(WaterHeaterState),
    /// Nothing received yet, or the payload could not be read.
    #[default]
    Unknown,
}

/// Everything known about an entity's current state.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct EntityState {
    pub value: StateValue,
    /// Merged from `json_attributes_topic`.
    pub attributes: Map<String, Value>,
    pub availability: Availability,
    /// Unix time in seconds of the last state message, when any.
    pub last_update: Option<u64>,
}
