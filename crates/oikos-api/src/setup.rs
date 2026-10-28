//! Wire types for the onboarding wizard: where the house is and how far setup got.

use std::fmt;

use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};

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
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
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

/// What a read says about a secret: only whether one is stored, never its value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SecretView {
    pub set: bool,
}

/// A secret in a `PUT` or a test: left out keeps the stored one, `null` clears it, a string sets it.
#[derive(Clone, Default, PartialEq, Eq)]
pub enum Secret {
    #[default]
    Keep,
    Clear,
    Set(String),
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Secret::Keep => f.write_str("Keep"),
            Secret::Clear => f.write_str("Clear"),
            Secret::Set(_) => f.write_str("Set(***)"),
        }
    }
}

/// Absent fields are `Keep` through `#[serde(default)]` on the field; `null` is `Clear`.
impl<'de> Deserialize<'de> for Secret {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Secret, D::Error> {
        Ok(match Option::<String>::deserialize(d)? {
            None => Secret::Clear,
            Some(s) => Secret::Set(s),
        })
    }
}

/// The outcome of a connection test.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TestResult {
    pub ok: bool,
    pub message: String,
    pub detail: Option<serde_json::Value>,
}

/// A geocoding candidate.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GeoPlace {
    pub name: String,
    pub admin: Option<String>,
    pub country: Option<String>,
    pub latitude: f64,
    pub longitude: f64,
    pub timezone: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MqttInput {
    pub host: String,
    pub port: u16,
    #[serde(default)]
    pub user: Option<String>,
    #[serde(default)]
    pub password: Secret,
    #[serde(default)]
    pub client_id: Option<String>,
    #[serde(default)]
    pub z2m_base: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MqttView {
    pub host: String,
    pub port: u16,
    pub user: Option<String>,
    pub password: SecretView,
    pub client_id: Option<String>,
    pub z2m_base: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DatabaseInput {
    pub host: String,
    pub port: u16,
    pub database: String,
    pub user: String,
    #[serde(default)]
    pub password: Secret,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DatabaseView {
    pub host: String,
    pub port: u16,
    pub database: String,
    pub user: String,
    pub password: SecretView,
}

/// Where the setup stands, as `GET /api/v1/setup` reports it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SetupStatus {
    /// `"setup"` or `"running"`.
    pub mode: String,
    pub step: SetupStep,
    pub finished: bool,
    /// Whether `bundled.json` exists (the compose file's own broker and database).
    pub bundled: bool,
    pub location: Option<Location>,
    pub mqtt: Option<MqttView>,
    pub database: Option<DatabaseView>,
    pub restarting: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn secret_in_a_body() {
        let absent: MqttInput = serde_json::from_value(json!({"host":"h","port":1})).unwrap();
        assert_eq!(absent.password, Secret::Keep);
        let null: MqttInput =
            serde_json::from_value(json!({"host":"h","port":1,"password":null})).unwrap();
        assert_eq!(null.password, Secret::Clear);
        let set: MqttInput =
            serde_json::from_value(json!({"host":"h","port":1,"password":"pw-1"})).unwrap();
        assert_eq!(set.password, Secret::Set("pw-1".into()));
        assert!(!format!("{set:?}").contains("pw-1"));
    }
}
