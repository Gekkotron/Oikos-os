//! Layered configuration: defaults, then `oikos.toml`, then `OIKOS_*` env vars.

use std::fmt;
use std::path::Path;

use figment::Figment;
use figment::providers::{Env, Format, Serialized, Toml};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Config {
    pub mqtt: MqttConfig,
    pub topics: TopicsConfig,
    pub http: HttpConfig,
    pub commands: CommandsConfig,
    pub log: LogConfig,
    pub push: PushConfig,
    pub frigate: FrigateConfig,
    pub automation: AutomationConfig,
    pub flows: FlowsConfig,
    pub groups: GroupsConfig,
    pub history: HistoryConfig,
}

#[derive(Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct MqttConfig {
    pub host: String,
    pub port: u16,
    pub user: Option<String>,
    pub password: Option<String>,
    pub client_id: String,
    pub send_ha_birth: bool,
    pub ha_status_topic: String,
}

impl Default for MqttConfig {
    fn default() -> Self {
        Self {
            host: "localhost".into(),
            port: 1883,
            user: None,
            password: None,
            client_id: "oikos-os".into(),
            send_ha_birth: false,
            ha_status_topic: "homeassistant/status".into(),
        }
    }
}

impl fmt::Debug for MqttConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let password = self.password.as_ref().map(|_| "***");
        f.debug_struct("MqttConfig")
            .field("host", &self.host)
            .field("port", &self.port)
            .field("user", &self.user)
            .field("password", &password)
            .field("client_id", &self.client_id)
            .field("send_ha_birth", &self.send_ha_birth)
            .field("ha_status_topic", &self.ha_status_topic)
            .finish()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct TopicsConfig {
    pub discovery_prefixes: Vec<String>,
    pub ui_prefix: String,
    pub z2m_base: String,
    /// Where flow definitions live: `<prefix>/<flow id>`, retained, one JSON document each.
    pub automation_prefix: String,
}

impl Default for TopicsConfig {
    fn default() -> Self {
        Self {
            discovery_prefixes: vec!["oikos".into()],
            ui_prefix: "oikos/ui".into(),
            z2m_base: "zigbee2mqtt".into(),
            automation_prefix: "oikos/automation".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct HttpConfig {
    pub bind: String,
}

impl Default for HttpConfig {
    fn default() -> Self {
        Self {
            bind: "0.0.0.0:8081".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct CommandsConfig {
    pub confirm_timeout_ms: u64,
    pub subscription_debounce_ms: u64,
}

impl Default for CommandsConfig {
    fn default() -> Self {
        Self {
            confirm_timeout_ms: 5000,
            subscription_debounce_ms: 500,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct LogConfig {
    pub level: String,
    pub json: bool,
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            level: "info".into(),
            json: false,
        }
    }
}

/// Where OikosPush keeps its retained alert history and where the app's replies go. The
/// defaults match OikosPush today; they become `oikos/history` and `oikos/push/responses`
/// once the hub takes over push.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct PushConfig {
    pub history_topic: String,
    pub responses_topic: String,
}

impl Default for PushConfig {
    fn default() -> Self {
        Self {
            history_topic: "oikos-push/history".into(),
            responses_topic: "oikos-push/responses".into(),
        }
    }
}

/// Frigate, for the alert snapshots and clips the browser cannot fetch itself: with `url` set,
/// media on that origin is served through the hub, which logs in with `user` / `password`
/// (Frigate's login port, 8971) or fetches anonymously when they are unset.
#[derive(Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct FrigateConfig {
    pub url: Option<String>,
    pub user: Option<String>,
    pub password: Option<String>,
    /// How long a connection to Frigate may take before the media request fails.
    pub connect_timeout_ms: u64,
}

impl Default for FrigateConfig {
    fn default() -> Self {
        Self {
            url: None,
            user: None,
            password: None,
            connect_timeout_ms: 5000,
        }
    }
}

impl fmt::Debug for FrigateConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let password = self.password.as_ref().map(|_| "***");
        f.debug_struct("FrigateConfig")
            .field("url", &self.url)
            .field("user", &self.user)
            .field("password", &password)
            .field("connect_timeout_ms", &self.connect_timeout_ms)
            .finish()
    }
}

/// The flow engine's knobs.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct AutomationConfig {
    /// How many recent runs the hub keeps per flow (newest first); older ones are dropped.
    pub runs_kept: usize,
}

impl Default for AutomationConfig {
    fn default() -> Self {
        Self { runs_kept: 20 }
    }
}

/// Flow scheduling.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct FlowsConfig {
    /// IANA time zone the schedules run in (`Europe/Paris`). Absent: the system's zone;
    /// unparseable: UTC, with a warning.
    pub timezone: Option<String>,
    /// House latitude in degrees, north positive. Set together with `longitude`; absent
    /// (both): Paris.
    pub latitude: Option<f64>,
    /// House longitude in degrees, east positive. Set together with `latitude`.
    pub longitude: Option<f64>,
}

impl FlowsConfig {
    /// Rejects a lone coordinate and coordinates outside the globe.
    pub fn validate(&self) -> Result<(), String> {
        match (self.latitude, self.longitude) {
            (None, None) => Ok(()),
            (Some(_), None) => Err(
                "[flows] latitude is set but longitude is missing: set both or neither".to_owned(),
            ),
            (None, Some(_)) => Err(
                "[flows] longitude is set but latitude is missing: set both or neither".to_owned(),
            ),
            (Some(lat), Some(lon)) => {
                if !(-90.0..=90.0).contains(&lat) {
                    return Err(format!("[flows] latitude {lat} is outside -90..=90"));
                }
                if !(-180.0..=180.0).contains(&lon) {
                    return Err(format!("[flows] longitude {lon} is outside -180..=180"));
                }
                Ok(())
            }
        }
    }

    /// The house (latitude, longitude), Paris when neither is configured.
    pub fn house_point(&self) -> (f64, f64) {
        match (self.latitude, self.longitude) {
            (Some(lat), Some(lon)) => (lat, lon),
            _ => (48.8566, 2.3522),
        }
    }

    /// True when the house point is the Paris fallback.
    pub fn fallback_active(&self) -> bool {
        !(self.latitude.is_some() && self.longitude.is_some())
    }
}

/// Hub-level kill switch for group publishing and migration. Off by default.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct GroupsConfig {
    pub enabled: bool,
}

/// TimescaleDB connection and the writer's queue size. An empty `url` turns history off
/// (the house keeps running, the `/api/v1/history/*` routes answer 503).
#[derive(Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct HistoryConfig {
    pub url: String,
    pub buffer_rows: usize,
}

impl Default for HistoryConfig {
    fn default() -> Self {
        Self {
            url: String::new(),
            buffer_rows: 100_000,
        }
    }
}

impl HistoryConfig {
    pub fn enabled(&self) -> bool {
        !self.url.is_empty()
    }
}

impl fmt::Debug for HistoryConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let masked = if self.url.is_empty() {
            String::new()
        } else {
            match url::Url::parse(&self.url) {
                Ok(mut u) => {
                    if u.password().is_some() {
                        let _ = u.set_password(Some("***"));
                    }
                    u.to_string()
                }
                Err(_) => "<unparsable>".to_string(),
            }
        };
        f.debug_struct("HistoryConfig")
            .field("url", &masked)
            .field("buffer_rows", &self.buffer_rows)
            .finish()
    }
}

/// Dotted, lowercase keys of string-typed fields. Single source of truth for
/// both the Env filter (figment's `ignore` sees the key after `split`, i.e.
/// `mqtt.password`) and the raw-string merge.
pub const STRING_ENV_KEYS: &[&str] = &[
    "mqtt.host",
    "mqtt.user",
    "mqtt.password",
    "mqtt.client_id",
    "mqtt.ha_status_topic",
    "topics.ui_prefix",
    "topics.z2m_base",
    "topics.automation_prefix",
    "http.bind",
    "log.level",
    "push.history_topic",
    "push.responses_topic",
    "flows.timezone",
    "frigate.url",
    "frigate.user",
    "frigate.password",
    "history.url",
];

impl Config {
    /// The `OIKOS_*` env layer for non-string fields; string fields are
    /// excluded because figment would guess `123456` to be a number.
    pub fn env_provider() -> Env {
        Env::prefixed("OIKOS_").split("__").ignore(STRING_ENV_KEYS)
    }

    /// Loads defaults, merged with the TOML file (`oikos.toml` when `path` is
    /// `None`; a missing file is fine), merged with `OIKOS_` env vars
    /// (`__` separates nesting levels).
    #[allow(clippy::result_large_err)] // signature fixed by the spec
    pub fn load(path: Option<&Path>) -> Result<Config, figment::Error> {
        let file = path.unwrap_or_else(|| Path::new("oikos.toml"));
        let mut figment = Figment::from(Serialized::defaults(Config::default()))
            .merge(Toml::file(file))
            .merge(Self::env_provider());
        // String fields keep the raw env text: figment would otherwise guess
        // `123456` or `007` to be numbers.
        for key in STRING_ENV_KEYS {
            let var = format!("OIKOS_{}", key.replace('.', "__").to_uppercase());
            if let Ok(value) = std::env::var(&var) {
                figment = figment.merge(Serialized::default(key, value));
            }
        }
        figment.extract()
    }
}

#[cfg(test)]
mod groups_config_tests {
    use super::*;

    #[test]
    fn groups_default_is_disabled() {
        assert!(!Config::default().groups.enabled);
    }

    #[test]
    fn groups_enabled_true_from_toml() {
        let cfg: Config = Figment::from(Serialized::defaults(Config::default()))
            .merge(Toml::string("[groups]\nenabled = true\n"))
            .extract()
            .unwrap();
        assert!(cfg.groups.enabled);
    }
}

#[cfg(test)]
mod flows_config_tests {
    use super::*;

    #[test]
    fn flows_config_both_coords_required_together() {
        let only_lat: FlowsConfig = toml::from_str("latitude = 45.75").unwrap();
        assert!(only_lat.validate().is_err());
        let only_lon: FlowsConfig = toml::from_str("longitude = 4.85").unwrap();
        assert!(only_lon.validate().is_err());
        let both: FlowsConfig = toml::from_str("latitude = 45.75\nlongitude = 4.85").unwrap();
        assert_eq!(both.validate(), Ok(()));
        let neither = FlowsConfig::default();
        assert_eq!(neither.validate(), Ok(()));
    }

    #[test]
    fn flows_config_coord_ranges_enforced() {
        let bad_lat: FlowsConfig = toml::from_str("latitude = 91.0\nlongitude = 4.85").unwrap();
        assert!(bad_lat.validate().is_err());
        let bad_lon: FlowsConfig = toml::from_str("latitude = 45.75\nlongitude = 181.0").unwrap();
        assert!(bad_lon.validate().is_err());
    }

    #[test]
    fn flows_config_house_point_falls_back_to_paris() {
        let cfg = FlowsConfig::default();
        assert_eq!(cfg.house_point(), (48.8566, 2.3522));
        assert!(cfg.fallback_active());
        let configured: FlowsConfig = toml::from_str("latitude = 45.75\nlongitude = 4.85").unwrap();
        assert_eq!(configured.house_point(), (45.75, 4.85));
        assert!(!configured.fallback_active());
    }
}
