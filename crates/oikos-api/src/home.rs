//! The home settings document: `GET`/`PUT /api/v1/home/settings`.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// What the home header needs from the user. Every field is optional.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct HomeSettings {
    /// The threshold in °C (5 to 30) at or below which a thermostat, or an off one, counts as eco.
    pub eco_setpoint: Option<f64>,
    /// The entity id of the select that holds the house mode.
    pub mode_entity: Option<String>,
    /// The entity id of the sensor shown as the indoor temperature.
    pub indoor_entity: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn home_settings_default_to_null_and_accept_partial_json() {
        let s: HomeSettings = serde_json::from_value(json!({"eco_setpoint": 15.0})).unwrap();
        assert_eq!(
            s,
            HomeSettings {
                eco_setpoint: Some(15.0),
                ..HomeSettings::default()
            }
        );
        assert_eq!(
            serde_json::to_value(HomeSettings::default()).unwrap(),
            json!({"eco_setpoint": null, "mode_entity": null, "indoor_entity": null})
        );
    }
}
