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
        assert_eq!(cfg.topics.automation_prefix, "oikos/automation");
        assert_eq!(cfg.automation.runs_kept, 20);
        assert_eq!(cfg.http.bind, "0.0.0.0:8081");
        assert_eq!(cfg.commands.confirm_timeout_ms, 5000);
        assert_eq!(cfg.commands.subscription_debounce_ms, 500);
        assert_eq!(cfg.log.level, "info");
        assert!(!cfg.log.json);
        assert_eq!(cfg.push.prefix, "oikos/push");
        assert_eq!(cfg.push.legacy_prefix, "oikos-push");
        assert_eq!(cfg.push.credentials, None);
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
        jail.set_env("OIKOS_PUSH__PREFIX", "1");
        jail.set_env("OIKOS_FRIGATE__PASSWORD", "0042");
        let cfg = Config::load(None)?;
        assert_eq!(cfg.mqtt.ha_status_topic, "42");
        assert_eq!(cfg.topics.ui_prefix, "7");
        assert_eq!(cfg.topics.z2m_base, "0123");
        assert_eq!(cfg.http.bind, "8080");
        assert_eq!(cfg.log.level, "5");
        assert_eq!(cfg.push.prefix, "1");
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

#[test]
fn env_overrides_automation_keys() {
    Jail::expect_with(|jail| {
        jail.set_env("OIKOS_TOPICS__AUTOMATION_PREFIX", "home/flows");
        jail.set_env("OIKOS_AUTOMATION__RUNS_KEPT", "5");
        let cfg = Config::load(None)?;
        assert_eq!(cfg.topics.automation_prefix, "home/flows");
        assert_eq!(cfg.automation.runs_kept, 5);
        Ok(())
    });
}

#[test]
fn history_defaults_off() {
    Jail::expect_with(|_jail| {
        let cfg = Config::load(None)?;
        assert!(!cfg.history.enabled());
        assert_eq!(cfg.history.url, "");
        assert_eq!(cfg.history.buffer_rows, 100_000);
        Ok(())
    });
}

#[test]
fn history_url_from_env_stays_a_string() {
    Jail::expect_with(|jail| {
        jail.set_env(
            "OIKOS_HISTORY__URL",
            "postgres://oikos:s3cret@192.168.1.91:5432/oikos",
        );
        let cfg = Config::load(None)?;
        assert_eq!(
            cfg.history.url,
            "postgres://oikos:s3cret@192.168.1.91:5432/oikos"
        );
        assert!(cfg.history.enabled());
        Ok(())
    });
}

#[test]
fn history_debug_masks_password() {
    Jail::expect_with(|jail| {
        jail.create_file(
            "oikos.toml",
            "[history]\nurl = \"postgres://oikos:s3cret@192.168.1.91:5432/oikos\"\n",
        )?;
        let cfg = Config::load(None)?;
        let text = format!("{:?}", cfg.history);
        assert!(text.contains("***"), "{text}");
        assert!(!text.contains("s3cret"), "{text}");
        Ok(())
    });
}

#[test]
fn flows_timezone_is_read() {
    Jail::expect_with(|jail| {
        assert_eq!(Config::load(None)?.flows.timezone, None);
        jail.create_file("oikos.toml", "[flows]\ntimezone = \"Europe/Paris\"\n")?;
        assert_eq!(
            Config::load(None)?.flows.timezone.as_deref(),
            Some("Europe/Paris")
        );
        jail.set_env("OIKOS_FLOWS__TIMEZONE", "UTC");
        assert_eq!(Config::load(None)?.flows.timezone.as_deref(), Some("UTC"));
        Ok(())
    });
}

#[test]
fn push_defaults_are_new_topics() {
    Jail::expect_with(|_jail| {
        let push = Config::load(None)?.push;
        assert_eq!(push.prefix, "oikos/push");
        assert_eq!(push.legacy_prefix, "oikos-push");
        assert_eq!(push.credentials, None);
        assert_eq!(push.retry_interval, 60);
        assert_eq!(push.max_retries, 10);
        assert_eq!(push.history_size, 100);
        assert_eq!(push.history_days, 7);
        Ok(())
    });
}

#[test]
fn push_keys_are_read() {
    Jail::expect_with(|jail| {
        jail.create_file(
            "oikos.toml",
            "[push]\nprefix = \"home/push\"\nlegacy_prefix = \"\"\n\
             credentials = \"/etc/oikos/fcm.json\"\nretry_interval = 30\n",
        )?;
        let push = Config::load(None)?.push;
        assert_eq!(push.prefix, "home/push");
        assert_eq!(push.legacy_prefix, "");
        assert_eq!(
            push.credentials.as_deref(),
            Some(std::path::Path::new("/etc/oikos/fcm.json"))
        );
        assert_eq!(push.retry_interval, 30);
        jail.set_env("OIKOS_PUSH__CREDENTIALS", "/tmp/key.json");
        assert_eq!(
            Config::load(None)?.push.credentials.as_deref(),
            Some(std::path::Path::new("/tmp/key.json"))
        );
        Ok(())
    });
}

#[test]
fn old_push_keys_are_rejected() {
    Jail::expect_with(|jail| {
        jail.create_file("oikos.toml", "[push]\nhistory_topic = \"x\"\n")?;
        let err = Config::load(None).expect_err("history_topic is no longer a key");
        let text = err.to_string();
        assert!(text.contains("history_topic"), "{text}");
        assert!(
            text.contains("legacy_prefix"),
            "the error names the new keys: {text}"
        );
        jail.create_file("oikos.toml", "[push]\nresponses_topic = \"x\"\n")?;
        let err = Config::load(None).expect_err("responses_topic is no longer a key");
        assert!(err.to_string().contains("responses_topic"), "{err}");
        Ok(())
    });
}

#[test]
fn example_config_loads() {
    Jail::expect_with(|_jail| {
        let path = std::path::Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../oikos.example.toml"
        ));
        let cfg = Config::load(Some(path))?;
        assert_eq!(cfg.push.prefix, "oikos/push");
        assert_eq!(cfg.push.credentials, None);
        Ok(())
    });
}
