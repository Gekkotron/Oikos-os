#![allow(clippy::result_large_err)] // figment::Jail closures return figment::Error

use figment::Jail;
use oikos_os::config::Config;

#[test]
fn defaults_match_spec() {
    Jail::expect_with(|_jail| {
        let cfg = Config::load(None)?;
        assert_eq!(cfg.mqtt.port, 1883);
        assert_eq!(cfg.mqtt.client_id, "oikos-os");
        assert!(!cfg.mqtt.send_ha_birth);
        assert_eq!(cfg.mqtt.ha_status_topic, "homeassistant/status");
        assert_eq!(cfg.topics.discovery_prefixes, vec!["oikos".to_string()]);
        assert_eq!(cfg.topics.ui_prefix, "oikos/ui");
        assert_eq!(cfg.topics.z2m_base, "zigbee2mqtt");
        assert_eq!(cfg.http.bind, "0.0.0.0:8080");
        assert_eq!(cfg.commands.confirm_timeout_ms, 5000);
        assert_eq!(cfg.commands.subscription_debounce_ms, 500);
        assert_eq!(cfg.log.level, "info");
        assert!(!cfg.log.json);
        Ok(())
    });
}

#[test]
fn env_overrides_file() {
    Jail::expect_with(|jail| {
        jail.create_file("oikos.toml", "[mqtt]\nhost = \"a\"\nport = 1884\n")?;
        let cfg = Config::load(None)?;
        assert_eq!(cfg.mqtt.host, "a");
        assert_eq!(cfg.mqtt.port, 1884);
        jail.set_env("OIKOS_MQTT__HOST", "b");
        let cfg = Config::load(None)?;
        assert_eq!(cfg.mqtt.host, "b");
        assert_eq!(cfg.mqtt.port, 1884);
        Ok(())
    });
}

#[test]
fn env_list_and_explicit_path() {
    Jail::expect_with(|jail| {
        jail.create_file("custom.toml", "[http]\nbind = \"127.0.0.1:9\"\n")?;
        jail.set_env(
            "OIKOS_TOPICS__DISCOVERY_PREFIXES",
            r#"["oikos","homeassistant"]"#,
        );
        let cfg = Config::load(Some(std::path::Path::new("custom.toml")))?;
        assert_eq!(cfg.http.bind, "127.0.0.1:9");
        assert_eq!(
            cfg.topics.discovery_prefixes,
            vec!["oikos".to_string(), "homeassistant".to_string()]
        );
        Ok(())
    });
}

#[test]
fn debug_redacts_password() {
    let mut cfg = Config::default();
    cfg.mqtt.password = Some("s3cret-pw".into());
    let dbg = format!("{:?}", cfg.mqtt);
    assert!(!dbg.contains("s3cret-pw"));
    assert!(dbg.contains("***"));
    cfg.mqtt.password = None;
    assert!(format!("{:?}", cfg.mqtt).contains("password: None"));
}
