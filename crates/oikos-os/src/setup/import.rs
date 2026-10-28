//! Imports an existing `oikos.toml` into the new shape: a [`HubFile`] and the [`Sections`].
//!
//! Key files: `Config::load` already defaults `secrets/firebase.json` and `secrets/solcast.key`
//! to the TOML's folder; a path named in the TOML is used as written, which is how the running
//! hub reads it (relative to the process working directory).

use std::path::{Path, PathBuf};

use anyhow::Context;
use oikos_api::{Location, SetupStep};

use crate::config::Config;
use crate::setup::hub_file::HubFile;
use crate::setup::sections::{
    AdvancedSection, PushSection, Sections, SolcastSection, WeatherSection,
};

/// The TOML to import: the `--config` file when it exists, else `<data_dir>/oikos.toml` when it
/// exists, else `None`. A missing file never imports.
pub fn toml_candidate(data_dir: &Path, flag: Option<&Path>) -> Option<PathBuf> {
    if let Some(flag) = flag.filter(|p| p.is_file()) {
        return Some(flag.to_path_buf());
    }
    let default = data_dir.join("oikos.toml");
    default.is_file().then_some(default)
}

/// Loads and validates `path` as today's hub does, then maps it into the new files.
pub fn import_toml(path: &Path) -> anyhow::Result<(HubFile, Sections)> {
    let cfg = Config::load(Some(path)).context("cannot load the config")?;
    cfg.validate_all()
        .map_err(|e| anyhow::anyhow!("invalid config: {e}"))?;

    let database_url = (!cfg.history.url.is_empty()).then(|| cfg.history.url.clone());
    let city = cfg.weather.city.clone();
    let country = cfg.weather.country.clone();
    let (location, pending_city) = match (cfg.flows.latitude, cfg.flows.longitude) {
        (Some(latitude), Some(longitude)) => {
            let timezone = cfg
                .flows
                .timezone
                .clone()
                .or_else(|| iana_time_zone::get_timezone().ok())
                .unwrap_or_else(|| "UTC".to_owned());
            let loc = Location {
                city,
                country,
                latitude,
                longitude,
                timezone,
            };
            (Some(loc), None)
        }
        _ if !city.is_empty() => (None, Some((city, country))),
        _ => (None, None),
    };
    let finished = database_url.is_some();
    let file = HubFile {
        mqtt: Some(cfg.mqtt.clone()),
        z2m_base: Some(cfg.topics.z2m_base.clone()),
        database_url,
        location,
        pending_city,
        http: cfg.http.clone(),
        log: cfg.log.clone(),
        step: if finished {
            SetupStep::Done
        } else {
            SetupStep::Database
        },
        finished,
        settings_imported: false,
    };

    let service_account = cfg.push.service_account.clone().or_else(|| {
        let path = cfg.push.credentials.as_deref()?;
        match std::fs::read_to_string(path) {
            Ok(text) => Some(text),
            Err(error) => {
                tracing::warn!(path = %path.display(), %error, "push key file not imported");
                None
            }
        }
    });
    let api_key = cfg.solcast.read_key().ok();
    if api_key.is_none() && cfg.solcast.api_key_file.is_some() {
        tracing::warn!(
            path = ?cfg.solcast.api_key_file,
            "solcast key file not imported (missing, unreadable or empty)"
        );
    }
    let sections = Sections {
        weather: WeatherSection {
            enabled: cfg.weather.enabled,
            refresh_minutes: cfg.weather.refresh_minutes,
        },
        solcast: SolcastSection {
            enabled: cfg.solcast.enabled,
            sites: cfg.solcast.sites.clone(),
            daily_limit: cfg.solcast.daily_limit,
            api_key,
        },
        frigate: cfg.frigate.clone(),
        push: PushSection {
            service_account,
            prefix: cfg.push.prefix.clone(),
            legacy_prefix: cfg.push.legacy_prefix.clone(),
            retry_interval: cfg.push.retry_interval,
            max_retries: cfg.push.max_retries,
            history_size: cfg.push.history_size,
            history_days: cfg.push.history_days,
        },
        climate: cfg.climate.clone(),
        advanced: AdvancedSection {
            topics: cfg.topics.clone(),
            commands: cfg.commands.clone(),
            automation: cfg.automation.clone(),
            watchdog: cfg.watchdog.clone(),
            groups: cfg.groups.clone(),
            alarm: cfg.alarm.clone(),
            history_buffer_rows: cfg.history.buffer_rows,
        },
    };
    Ok((file, sections))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::setup::sections::assemble;

    /// Holds figment's global lock so `OIKOS_*` env tests elsewhere cannot leak in mid-load.
    #[allow(clippy::result_large_err)]
    fn import_locked(p: &Path) -> anyhow::Result<(HubFile, Sections)> {
        let mut out = None;
        figment::Jail::expect_with(|_| {
            out = Some(import_toml(p));
            Ok(())
        });
        out.unwrap()
    }

    #[allow(clippy::result_large_err)]
    fn load_locked(p: &Path) -> Config {
        let mut out = None;
        figment::Jail::expect_with(|_| {
            out = Some(Config::load(Some(p)).unwrap());
            Ok(())
        });
        out.unwrap()
    }

    fn write(dir: &Path, name: &str, body: &str) -> PathBuf {
        let p = dir.join(name);
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&p, body).unwrap();
        p
    }

    const FULL: &str = r#"
[mqtt]
host = "192.0.2.1"
[history]
url = "postgres://u:pw@db.example.com/oikos"
[flows]
latitude = 45.76
longitude = 4.84
timezone = "Europe/Paris"
[weather]
enabled = true
city = "Lyon"
country = "FR"
"#;

    #[test]
    fn imports_mqtt_history_and_location() {
        let dir = tempfile::tempdir().unwrap();
        let p = write(dir.path(), "oikos.toml", FULL);
        let (file, sections) = import_locked(&p).unwrap();
        assert_eq!(file.mqtt.as_ref().unwrap().host, "192.0.2.1");
        assert_eq!(
            file.database_url.as_deref(),
            Some("postgres://u:pw@db.example.com/oikos")
        );
        let loc = file.location.unwrap();
        assert_eq!(
            (
                loc.city.as_str(),
                loc.country.as_deref(),
                loc.timezone.as_str()
            ),
            ("Lyon", Some("FR"), "Europe/Paris")
        );
        assert_eq!((loc.latitude, loc.longitude), (45.76, 4.84));
        assert!(file.pending_city.is_none());
        assert!(file.finished);
        assert_eq!(file.step, SetupStep::Done);
        assert!(!file.settings_imported);
        assert_eq!(file.z2m_base.as_deref(), Some("zigbee2mqtt"));
        assert!(sections.weather.enabled);
    }

    #[test]
    fn no_history_url_means_database_step() {
        let dir = tempfile::tempdir().unwrap();
        let p = write(dir.path(), "oikos.toml", "[mqtt]\nhost = \"192.0.2.1\"\n");
        let (file, _) = import_locked(&p).unwrap();
        assert_eq!(file.database_url, None);
        assert!(!file.finished);
        assert_eq!(file.step, SetupStep::Database);
    }

    #[test]
    fn no_coordinates_means_pending_city() {
        let dir = tempfile::tempdir().unwrap();
        let p = write(
            dir.path(),
            "oikos.toml",
            "[weather]\nenabled = true\ncity = \"Lyon\"\ncountry = \"FR\"\n",
        );
        let (file, _) = import_locked(&p).unwrap();
        assert!(file.location.is_none());
        assert_eq!(file.pending_city, Some(("Lyon".into(), Some("FR".into()))));
    }

    #[test]
    fn empty_city_with_coordinates_gives_empty_name() {
        let dir = tempfile::tempdir().unwrap();
        let p = write(
            dir.path(),
            "oikos.toml",
            "[flows]\nlatitude = 1.0\nlongitude = 2.0\ntimezone = \"UTC\"\n",
        );
        let (file, _) = import_locked(&p).unwrap();
        let loc = file.location.unwrap();
        assert_eq!(loc.city, "");
        assert_eq!(loc.country, None);
    }

    #[test]
    fn reads_the_key_files() {
        let dir = tempfile::tempdir().unwrap();
        let p = write(dir.path(), "oikos.toml", FULL);
        write(dir.path(), "secrets/firebase.json", "{\"k\":1}");
        write(dir.path(), "secrets/solcast.key", "sk-test\n");
        let (_, sections) = import_locked(&p).unwrap();
        assert_eq!(sections.push.service_account.as_deref(), Some("{\"k\":1}"));
        assert_eq!(sections.solcast.api_key.as_deref(), Some("sk-test"));
    }

    #[test]
    fn missing_key_files_leave_the_fields_unset() {
        let dir = tempfile::tempdir().unwrap();
        let p = write(dir.path(), "oikos.toml", FULL);
        let (_, sections) = import_locked(&p).unwrap();
        assert_eq!(sections.push.service_account, None);
        assert_eq!(sections.solcast.api_key, None);
    }

    #[test]
    fn invalid_toml_is_refused_with_todays_message() {
        let dir = tempfile::tempdir().unwrap();
        let p = write(
            dir.path(),
            "oikos.toml",
            "[climate]\nenabled = true\nprefix = \"#\"\n",
        );
        let err = import_locked(&p).unwrap_err().to_string();
        assert!(err.contains("[climate] prefix"), "{err}");
    }

    #[test]
    fn candidate_ignores_missing_files() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            toml_candidate(dir.path(), Some(&dir.path().join("nope.toml"))),
            None
        );
        assert_eq!(toml_candidate(dir.path(), None), None);
    }

    #[test]
    fn candidate_prefers_the_flag_then_the_data_dir() {
        let dir = tempfile::tempdir().unwrap();
        let data = write(dir.path(), "oikos.toml", "");
        let flag = write(dir.path(), "other.toml", "");
        assert_eq!(toml_candidate(dir.path(), Some(&flag)), Some(flag));
        assert_eq!(toml_candidate(dir.path(), None), Some(data));
    }

    #[test]
    fn assemble_matches_load_on_the_key_sections() {
        let dir = tempfile::tempdir().unwrap();
        let p = write(dir.path(), "oikos.toml", FULL);
        let loaded = load_locked(&p);
        let (file, sections) = import_locked(&p).unwrap();
        let got = assemble(&file, &sections);
        assert_eq!(got.mqtt, loaded.mqtt);
        assert_eq!(got.topics, loaded.topics);
        assert_eq!(got.history, loaded.history);
        assert_eq!(got.flows, loaded.flows);
        assert_eq!(got.weather.enabled, loaded.weather.enabled);
        assert_eq!(got.weather.city, loaded.weather.city);
        assert_eq!(got.weather.country, loaded.weather.country);
        assert_eq!(got.weather.refresh_minutes, loaded.weather.refresh_minutes);
        assert_eq!(got.solcast.enabled, loaded.solcast.enabled);
        assert_eq!(got.frigate, loaded.frigate);
        assert_eq!(got.climate, loaded.climate);
        assert_eq!(got.push.prefix, loaded.push.prefix);
    }
}
