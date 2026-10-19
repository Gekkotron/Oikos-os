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
        assert_eq!(cfg.push.history_topic, "oikos-push/history");
        assert_eq!(cfg.push.responses_topic, "oikos-push/responses");
        assert_eq!(cfg.frigate.url, None);
        assert_eq!(cfg.frigate.user, None);
        assert_eq!(cfg.frigate.password, None);
        assert_eq!(cfg.frigate.connect_timeout_ms, 5000);
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

#[test]
fn numeric_looking_env_strings_stay_strings() {
    Jail::expect_with(|jail| {
        jail.set_env("OIKOS_MQTT__PASSWORD", "123456");
        jail.set_env("OIKOS_MQTT__CLIENT_ID", "1");
        jail.set_env("OIKOS_MQTT__USER", "007");
        jail.set_env("OIKOS_MQTT__HOST", "192.168.1.91");
        jail.set_env("OIKOS_MQTT__HA_STATUS_TOPIC", "42");
        jail.set_env("OIKOS_TOPICS__UI_PREFIX", "7");
        jail.set_env("OIKOS_TOPICS__Z2M_BASE", "0123");
        jail.set_env("OIKOS_HTTP__BIND", "8080");
        jail.set_env("OIKOS_LOG__LEVEL", "5");
        jail.set_env("OIKOS_PUSH__HISTORY_TOPIC", "1");
        jail.set_env("OIKOS_FRIGATE__PASSWORD", "0042");
        let cfg = Config::load(None)?;
        assert_eq!(cfg.mqtt.ha_status_topic, "42");
        assert_eq!(cfg.topics.ui_prefix, "7");
        assert_eq!(cfg.topics.z2m_base, "0123");
        assert_eq!(cfg.http.bind, "8080");
        assert_eq!(cfg.log.level, "5");
        assert_eq!(cfg.push.history_topic, "1");
        assert_eq!(cfg.frigate.password.as_deref(), Some("0042"));
        assert_eq!(cfg.mqtt.password.as_deref(), Some("123456"));
        assert_eq!(cfg.mqtt.client_id, "1");
        assert_eq!(cfg.mqtt.user.as_deref(), Some("007"));
        assert_eq!(cfg.mqtt.host, "192.168.1.91");
        Ok(())
    });
}

#[test]
fn env_layer_excludes_string_keys() {
    Jail::expect_with(|jail| {
        for key in oikos_os::config::STRING_ENV_KEYS {
            jail.set_env(
                format!("OIKOS_{}", key.replace('.', "__").to_uppercase()),
                "123",
            );
        }
        jail.set_env("OIKOS_MQTT__PORT", "1999");
        let env = figment::Figment::from(Config::env_provider());
        for key in oikos_os::config::STRING_ENV_KEYS {
            assert!(
                env.find_value(key).is_err(),
                "{key} leaked into the Env layer"
            );
        }
        assert!(env.find_value("mqtt.port").is_ok());
        Ok(())
    });
}

#[test]
fn debug_redacts_frigate_password() {
    Jail::expect_with(|jail| {
        jail.create_file(
            "oikos.toml",
            "[frigate]\nurl = \"http://f:8971\"\nuser = \"u\"\npassword = \"hunter2\"\n",
        )?;
        let cfg = Config::load(None)?;
        let text = format!("{cfg:?}");
        assert!(text.contains("http://f:8971"));
        assert!(!text.contains("hunter2"), "{text}");
        Ok(())
    });
}
