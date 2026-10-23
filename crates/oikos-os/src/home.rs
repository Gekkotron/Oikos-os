//! The home settings document: the few values the home header needs from the hub (the eco
//! setpoint, the house mode select and the indoor temperature sensor). It is retained on
//! `<ui_prefix>/_home`, next to `_areas` and `_types`, and intercepted by the hub before the
//! registry so it never becomes an override.

use oikos_api::{HomeSettings, WsEvent};
use oikos_core::command::Publish;
use oikos_core::model::Kind;
use oikos_core::registry::Registry;
use std::sync::MutexGuard;

use crate::hub::{Hub, HubError};

/// The lowest accepted eco setpoint, in °C.
const ECO_MIN: f64 = 5.0;
/// The highest accepted eco setpoint, in °C.
const ECO_MAX: f64 = 30.0;

/// The topic the document is retained on.
pub fn topic(ui_prefix: &str) -> String {
    format!("{ui_prefix}/_home")
}

/// Checks the document against the house: the setpoint is a sane temperature, the mode is a
/// select and the indoor entity is a temperature sensor. The message names the field.
pub fn validate(settings: &HomeSettings, reg: &Registry) -> Result<(), String> {
    if let Some(eco) = settings.eco_setpoint
        && !(eco.is_finite() && (ECO_MIN..=ECO_MAX).contains(&eco))
    {
        return Err(format!(
            "eco_setpoint must be between {ECO_MIN} and {ECO_MAX} °C"
        ));
    }
    if let Some(id) = &settings.mode_entity {
        match reg.entity(id) {
            Some(entity) if entity.kind == Kind::Select => {}
            Some(_) => return Err(format!("mode_entity `{id}` is not a select")),
            None => return Err(format!("mode_entity `{id}` is not an entity")),
        }
    }
    if let Some(id) = &settings.indoor_entity {
        match reg.entity(id) {
            Some(entity)
                if entity.kind == Kind::Sensor
                    && entity.device_class.as_deref() == Some("temperature") => {}
            Some(_) => return Err(format!("indoor_entity `{id}` is not a temperature sensor")),
            None => return Err(format!("indoor_entity `{id}` is not an entity")),
        }
    }
    Ok(())
}

impl Hub {
    fn home_slot(&self) -> MutexGuard<'_, HomeSettings> {
        self.home_settings
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// The home settings as last applied (defaults until one is retained).
    pub fn home_settings(&self) -> HomeSettings {
        self.home_slot().clone()
    }

    /// Validates, publishes the document retained and applies it. Returns the stored document.
    pub fn set_home_settings(&self, settings: HomeSettings) -> Result<HomeSettings, HubError> {
        validate(&settings, &self.read()).map_err(HubError::Rejected)?;
        let payload = serde_json::to_string(&settings)
            .map_err(|err| HubError::Rejected(format!("home settings: {err}")))?;
        self.publish(Publish {
            topic: topic(&self.ui_prefix),
            payload: payload.clone(),
            retain: true,
        })?;
        self.ingest_home(payload.as_bytes());
        Ok(self.home_settings())
    }

    /// Applies the retained document: empty means defaults, an undecodable payload keeps the
    /// previous document. A change is broadcast.
    pub(crate) fn ingest_home(&self, payload: &[u8]) {
        let next = if payload.is_empty() {
            HomeSettings::default()
        } else {
            match serde_json::from_slice::<HomeSettings>(payload) {
                Ok(settings) => settings,
                Err(err) => {
                    tracing::warn!(%err, "ignoring an undecodable home settings document");
                    return;
                }
            }
        };
        {
            let mut slot = self.home_slot();
            if *slot == next {
                return;
            }
            *slot = next.clone();
        }
        self.broadcast(WsEvent::HomeSettingsChanged { settings: next });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oikos_core::registry::RegistryConfig;

    const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../oikos-core/tests/fixtures/");

    fn registry() -> Registry {
        let mut reg = Registry::new(RegistryConfig {
            discovery_prefixes: vec!["oikos".to_owned()],
            ..RegistryConfig::default()
        });
        for (topic, file) in [
            (
                "oikos/sensor/0x00124b0008fd6568/temperature_1/config",
                "sensor__0x00124b0008fd6568__temperature_1.json",
            ),
            (
                "oikos/sensor/0x00124b0008fd6568/humidity_1/config",
                "sensor__0x00124b0008fd6568__humidity_1.json",
            ),
            (
                "oikos/select/0x000d6ffffe9d44e4/indicator_mode/config",
                "select__0x000d6ffffe9d44e4__indicator_mode.json",
            ),
        ] {
            let payload = std::fs::read(format!("{FIXTURES}discovery/{file}")).expect("fixture");
            let inbound = reg.route(topic, payload, 0);
            reg.apply(inbound);
        }
        reg
    }

    /// The id of the first entity matching `pred`.
    fn id_where(reg: &Registry, pred: impl Fn(&oikos_core::model::Entity) -> bool) -> String {
        reg.entities()
            .find(|e| pred(e))
            .expect("fixture entity")
            .unique_id
            .clone()
    }

    fn eco(value: Option<f64>) -> HomeSettings {
        HomeSettings {
            eco_setpoint: value,
            ..HomeSettings::default()
        }
    }

    #[test]
    fn eco_setpoint_bounds() {
        let reg = registry();
        assert!(validate(&eco(Some(5.0)), &reg).is_ok());
        assert!(validate(&eco(Some(30.0)), &reg).is_ok());
        assert!(validate(&eco(None), &reg).is_ok());
        for bad in [4.9, 30.1, f64::NAN, f64::INFINITY] {
            let err = validate(&eco(Some(bad)), &reg).expect_err("out of range");
            assert!(err.contains("eco_setpoint"), "{err}");
        }
    }

    #[test]
    fn mode_entity_must_be_a_select() {
        let reg = registry();
        let select = id_where(&reg, |e| e.kind == Kind::Select);
        let sensor = id_where(&reg, |e| e.kind == Kind::Sensor);
        let with = |id: &str| HomeSettings {
            mode_entity: Some(id.to_owned()),
            ..HomeSettings::default()
        };
        assert!(validate(&with(&select), &reg).is_ok());
        for bad in [sensor.as_str(), "nope"] {
            let err = validate(&with(bad), &reg).expect_err("not a select");
            assert!(err.contains("mode_entity"), "{err}");
        }
    }

    #[test]
    fn indoor_entity_must_be_a_temperature_sensor() {
        let reg = registry();
        let class = |c: &str| {
            id_where(&reg, |e| {
                e.kind == Kind::Sensor && e.device_class.as_deref() == Some(c)
            })
        };
        let with = |id: &str| HomeSettings {
            indoor_entity: Some(id.to_owned()),
            ..HomeSettings::default()
        };
        assert!(validate(&with(&class("temperature")), &reg).is_ok());
        for bad in [class("humidity"), "nope".to_owned()] {
            let err = validate(&with(&bad), &reg).expect_err("not a temperature sensor");
            assert!(err.contains("indoor_entity"), "{err}");
        }
    }

    #[test]
    fn topic_sits_beside_areas() {
        assert_eq!(topic("oikos/ui"), "oikos/ui/_home");
    }
}
