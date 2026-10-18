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
}

impl Default for TopicsConfig {
    fn default() -> Self {
        Self {
            discovery_prefixes: vec!["oikos".into()],
            ui_prefix: "oikos/ui".into(),
            z2m_base: "zigbee2mqtt".into(),
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
            bind: "0.0.0.0:8080".into(),
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
    "http.bind",
    "log.level",
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
