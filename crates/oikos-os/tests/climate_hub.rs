//! The climate switch, its retained documents and the read view, driven without a broker.

use std::path::PathBuf;
use std::sync::Arc;

use oikos_api::{ConnectionStatus, WsEvent};
use oikos_climate::doc::{Block, Day, HhMm, ModeRules, Rules, Setpoint, ZoneSettings, ZoneState};
use oikos_core::command::Publish;
use oikos_os::config::Config;
use oikos_os::hub::{Hub, HubError};
use tokio::sync::mpsc;

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(
        PathBuf::from(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../oikos-core/tests/fixtures/discovery/"
        ))
        .join(name),
    )
    .expect("fixture")
}

fn hub(enabled: bool) -> (Arc<Hub>, mpsc::Receiver<Publish>) {
    let mut cfg = Config::default();
    cfg.topics.discovery_prefixes = vec!["oikos".to_owned()];
    cfg.climate.enabled = enabled;
    let (tx, rx) = mpsc::channel(64);
    let hub = Hub::new(&cfg, tx);
    hub.set_connection(ConnectionStatus::Online);
    (hub, rx)
}

/// The recorded Daikin `n` (0..=3), placed in `area` by override; returns its entity id.
async fn feed_daikin(hub: &Hub, n: u8, area: &str) -> String {
    let name = format!("climate__daikin_192_168_1_15{n}__climate.json");
    let topic = format!("oikos/climate/daikin_192_168_1_15{n}/climate/config");
    hub.ingest(&topic, &fixture(&name));
    place(hub, &format!("daikin_192_168_1_15{n}"), area).await
}

/// Puts the entity whose id starts with `prefix` in `area`.
async fn place(hub: &Hub, prefix: &str, area: &str) -> String {
    let id = hub
        .snapshot()
        .entities
        .into_iter()
        .find(|e| e.id.starts_with(prefix))
        .unwrap_or_else(|| panic!("no entity {prefix}"))
        .id;
    let mut patch = serde_json::Map::new();
    patch.insert("area".to_owned(), serde_json::Value::from(area));
    hub.set_override(&id, &patch).await.expect("area override");
    id
}

fn drain(rx: &mut mpsc::Receiver<Publish>) -> Vec<Publish> {
    let mut out = Vec::new();
    while let Ok(p) = rx.try_recv() {
        out.push(p);
    }
    out
}

fn active() -> ZoneSettings {
    ZoneSettings {
        state: ZoneState::Active,
        ..ZoneSettings::default()
    }
}

#[tokio::test]
async fn disabled_has_no_filters_and_refuses() {
    let (hub, _rx) = hub(false);
    assert!(!hub.extra_filters().iter().any(|f| f.contains("climate")));
    assert!(!hub.climate_enabled());
    assert!(matches!(hub.climate_view(), Err(HubError::Publish(m)) if m == "climate disabled"));
    assert!(matches!(
        hub.set_climate_zone("living", active()),
        Err(HubError::Publish(m)) if m == "climate disabled"
    ));
    // A climate-looking topic is just an unknown topic while the engine is off.
    hub.ingest_message("oikos/climate/house", b"{}", true);
}

#[tokio::test]
async fn enabled_subscribes_house_and_zones_only() {
    let (hub, _rx) = hub(true);
    let f = hub.extra_filters();
    assert!(
        f.contains(&"oikos/climate/house".to_owned())
            && f.contains(&"oikos/climate/zone/+/+".to_owned())
    );
    assert!(!f.iter().any(|t| t.ends_with('#')));
}

#[tokio::test]
async fn zone_settings_doc_is_not_a_device() {
    let (hub, _rx) = hub(true);
    let changes = hub.ingest_message(
        "oikos/climate/zone/living/settings",
        br#"{"state":"active"}"#,
        true,
    );
    assert!(changes.is_empty());
    assert!(
        hub.snapshot()
            .entities
            .iter()
            .all(|e| !e.id.contains("living"))
    );
    assert!(hub.snapshot().entities.is_empty());
}

#[tokio::test]
async fn climate_group_discovery_still_reaches_registry() {
    let (hub, _rx) = hub(true);
    let text = String::from_utf8(fixture("climate__daikin_192_168_1_150__climate.json"))
        .unwrap()
        .replace("daikin_192_168_1_150", "group_g1");
    hub.ingest_message("oikos/climate/group/g1/config", text.as_bytes(), true);
    assert!(
        hub.snapshot()
            .entities
            .iter()
            .any(|e| e.id.starts_with("group_g1")),
        "a group's discovery config under the climate prefix is still a device"
    );
}

#[tokio::test]
async fn store_keeps_invalid_with_reason() {
    let (hub, _rx) = hub(true);
    let mut events = hub.subscribe();
    hub.ingest_message(
        "oikos/climate/zone/attic/settings",
        br#"{"state":"active"}"#,
        true,
    );
    assert!(matches!(
        events.recv().await.unwrap(),
        WsEvent::ClimateChanged { zone: Some(z) } if z == "attic"
    ));
    hub.ingest_message(
        "oikos/climate/zone/attic/settings",
        br#"{"state":"bogus"}"#,
        true,
    );
    let view = hub.climate_view().unwrap();
    let zone = view.zones.iter().find(|z| z.zone == "attic").unwrap();
    assert_eq!(zone.settings.state, ZoneState::Active);
    assert!(zone.settings_invalid.is_some());

    hub.ingest_message("oikos/climate/house", br#"{"frost":40.0}"#, true);
    let view = hub.climate_view().unwrap();
    assert!(view.house_invalid.is_some());
}

#[tokio::test]
async fn view_lists_area_zones_and_skips_groups() {
    let (hub, _rx) = hub(true);
    let a = feed_daikin(&hub, 0, "living").await;
    let b = feed_daikin(&hub, 1, "living").await;
    let text = String::from_utf8(fixture("climate__daikin_192_168_1_152__climate.json"))
        .unwrap()
        .replace("daikin_192_168_1_152", "group_g1");
    hub.ingest("oikos/climate/group/g1/config", text.as_bytes());
    place(&hub, "group_g1", "living").await;

    let view = hub.climate_view().unwrap();
    assert!(view.enabled && !view.history);
    assert_eq!(view.zones.len(), 1, "{:?}", view.zones);
    let zone = &view.zones[0];
    assert_eq!(zone.zone, "living");
    assert!(!zone.orphan);
    let mut want = vec![a, b];
    want.sort();
    assert_eq!(zone.entities, want);

    // `settings.entities` wins over the area's devices.
    let settings = ZoneSettings {
        entities: Some(vec![want[0].clone()]),
        ..active()
    };
    hub.ingest_message(
        "oikos/climate/zone/living/settings",
        serde_json::to_string(&settings).unwrap().as_bytes(),
        true,
    );
    let view = hub.climate_view().unwrap();
    assert_eq!(view.zones[0].entities, vec![want[0].clone()]);
}

#[tokio::test]
async fn zone_without_area_is_listed_orphan() {
    let (hub, _rx) = hub(true);
    hub.ingest_message(
        "oikos/climate/zone/attic/settings",
        br#"{"state":"active"}"#,
        true,
    );
    let view = hub.climate_view().unwrap();
    assert_eq!(view.zones.len(), 1);
    assert!(view.zones[0].orphan);
    assert_eq!(view.zones[0].zone, "attic");
}

#[tokio::test]
async fn set_zone_publishes_retained_and_applies() {
    let (hub, mut rx) = hub(true);
    feed_daikin(&hub, 0, "living").await;
    drain(&mut rx);
    let mut events = hub.subscribe();
    hub.set_climate_zone("living", active()).unwrap();
    let published = drain(&mut rx);
    assert_eq!(published.len(), 1);
    assert_eq!(published[0].topic, "oikos/climate/zone/living/settings");
    assert!(published[0].retain);
    let view = hub.climate_view().unwrap();
    assert_eq!(view.zones[0].settings.state, ZoneState::Active);
    assert!(matches!(
        events.recv().await.unwrap(),
        WsEvent::ClimateChanged { zone: Some(z) } if z == "living"
    ));
}

#[tokio::test]
async fn set_house_publishes_retained_and_applies() {
    let (hub, mut rx) = hub(true);
    let house = oikos_climate::doc::HouseSettings {
        heat_below: 17.0,
        ..Default::default()
    };
    hub.set_climate_house(house).unwrap();
    let published = drain(&mut rx);
    assert_eq!(published[0].topic, "oikos/climate/house");
    assert!(published[0].retain);
    assert_eq!(hub.climate_view().unwrap().house.heat_below, 17.0);
    let bad = oikos_climate::doc::HouseSettings {
        frost: 40.0,
        ..Default::default()
    };
    assert!(matches!(
        hub.set_climate_house(bad),
        Err(HubError::Rejected(_))
    ));
}

#[tokio::test]
async fn set_zone_rejects_invalid_rules() {
    let (hub, mut rx) = hub(true);
    feed_daikin(&hub, 0, "living").await;
    drain(&mut rx);
    let block = |from: u16, to: u16| Block {
        days: vec![Day::Mon],
        from: HhMm(from),
        to: HhMm(to),
        setpoint: Setpoint::Temp(20.0),
        evidence: None,
    };
    let overlapping = ZoneSettings {
        rules: Rules {
            heat: Some(ModeRules {
                base: Setpoint::Temp(18.0),
                blocks: vec![block(60, 300), block(120, 400)],
            }),
            ..Rules::default()
        },
        ..active()
    };
    assert!(matches!(
        hub.set_climate_zone("living", overlapping),
        Err(HubError::Rejected(_))
    ));
    assert!(drain(&mut rx).is_empty());
}

#[tokio::test]
async fn set_zone_rejects_bad_ids_and_unknown_areas() {
    let (hub, mut rx) = hub(true);
    feed_daikin(&hub, 0, "living").await;
    drain(&mut rx);
    for id in ["", "a/b", "a+b", "a#", "#"] {
        assert!(
            matches!(
                hub.set_climate_zone(id, active()),
                Err(HubError::Rejected(_))
            ),
            "{id:?}"
        );
    }
    assert!(matches!(
        hub.set_climate_zone("nowhere", active()),
        Err(HubError::UnknownArea(_))
    ));
    assert!(drain(&mut rx).is_empty());
}

// ----- the evaluation loop -----

mod engine {
    use std::collections::BTreeMap;
    use std::time::Duration;

    use chrono::TimeZone;
    use oikos_climate::doc::{
        Decision, DeviceTarget, HouseSettings, Mode, Override, OverrideOrigin, Source,
    };

    use super::*;

    /// What the simulated Daikins hold: mode, target and current temperature per unit.
    struct Sim {
        echo: bool,
        units: BTreeMap<u8, (String, f64, f64)>,
    }

    impl Sim {
        fn new() -> Self {
            Self {
                echo: true,
                units: BTreeMap::new(),
            }
        }

        /// Sets the state of unit `n` and reports it on its state topic.
        fn set(&mut self, hub: &Hub, n: u8, mode: &str, target: f64, current: f64) {
            self.units.insert(n, (mode.to_owned(), target, current));
            self.report(hub, n);
        }

        fn report(&self, hub: &Hub, n: u8) {
            let (mode, target, current) = &self.units[&n];
            let payload = serde_json::json!({
                "mode": mode, "target_temperature": target, "current_temperature": current,
            });
            hub.ingest(
                &format!("daikin/192.168.1.15{n}/ha/state"),
                payload.to_string().as_bytes(),
            );
        }

        /// What a Daikin does with a command: applies it and reports its state.
        fn on_publish(&mut self, hub: &Hub, p: &Publish) {
            // The broker hands the push history back to the hub.
            if p.topic == "oikos/push/history" {
                hub.ingest_message(&p.topic, p.payload.as_bytes(), false);
                return;
            }
            let Some(rest) = p.topic.strip_prefix("daikin/192.168.1.15") else {
                return;
            };
            if !self.echo {
                return;
            }
            let n: u8 = rest[..1].parse().expect("unit");
            let unit = self.units.get_mut(&n).expect("known unit");
            if rest.ends_with("/mode/set") {
                unit.0 = p.payload.clone();
            } else if rest.ends_with("/temperature/set") {
                unit.1 = p.payload.parse().expect("temperature");
            }
            self.report(hub, n);
        }
    }

    struct Rig {
        hub: Arc<Hub>,
        rx: mpsc::Receiver<Publish>,
        sim: Sim,
        /// The first daikin's entity id.
        id: String,
        /// "Now" of the test, real so the registry's report times look fresh.
        t0: u64,
    }

    impl Rig {
        async fn new(tz: Option<&str>) -> Rig {
            let mut cfg = Config::default();
            cfg.topics.discovery_prefixes = vec!["oikos".to_owned()];
            cfg.climate.enabled = true;
            cfg.flows.timezone = tz.map(str::to_owned);
            let (tx, rx) = mpsc::channel(64);
            let hub = Hub::new(&cfg, tx);
            hub.set_connection(ConnectionStatus::Online);
            let id = feed_daikin(&hub, 0, "living").await;
            let mut rig = Rig {
                hub,
                rx,
                sim: Sim::new(),
                id,
                t0: oikos_os::clock::now(),
            };
            rig.sim.set(&rig.hub, 0, "heat", 22.0, 20.0);
            drain(&mut rig.rx);
            rig
        }

        /// One evaluation pass at `t0 + at`; the daikins answer the commands. Returns what
        /// was published.
        async fn pass(&mut self, at: u64) -> Vec<Publish> {
            let hub = Arc::clone(&self.hub);
            let now = self.t0 + at;
            let mut task = tokio::spawn(async move { hub.evaluate_climate(now).await });
            let mut out = Vec::new();
            loop {
                tokio::select! {
                    published = self.rx.recv() => {
                        let published = published.expect("publisher open");
                        self.sim.on_publish(&self.hub, &published);
                        out.push(published);
                    }
                    done = &mut task => {
                        done.expect("pass");
                        break;
                    }
                }
            }
            for published in drain(&mut self.rx) {
                self.sim.on_publish(&self.hub, &published);
                out.push(published);
            }
            out
        }

        fn settings(&mut self, settings: ZoneSettings) {
            self.hub.set_climate_zone("living", settings).unwrap();
            drain(&mut self.rx);
        }

        fn zone(&self) -> oikos_climate::view::ZoneView {
            self.hub
                .climate_view()
                .unwrap()
                .zones
                .into_iter()
                .find(|z| z.zone == "living")
                .expect("living zone")
        }
    }

    fn heat_rules(base: f64) -> ZoneSettings {
        ZoneSettings {
            rules: Rules {
                heat: Some(ModeRules {
                    base: Setpoint::Temp(base),
                    blocks: Vec::new(),
                }),
                ..Rules::default()
            },
            ..active()
        }
    }

    /// The commands the daikins received, as (topic suffix, payload).
    fn commands(out: &[Publish]) -> Vec<(String, String)> {
        out.iter()
            .filter_map(|p| {
                let rest = p.topic.strip_prefix("daikin/192.168.1.15")?;
                Some((rest[1..].to_owned(), p.payload.clone()))
            })
            .collect()
    }

    fn decisions_published(out: &[Publish]) -> usize {
        out.iter()
            .filter(|p| p.topic == "oikos/climate/zone/living/decision")
            .count()
    }

    fn temperature_commands(out: &[Publish]) -> Vec<f64> {
        commands(out)
            .into_iter()
            .filter(|(topic, _)| topic == "/ha/temperature/set")
            .map(|(_, payload)| payload.parse().unwrap())
            .collect()
    }

    /// Makes the push service accept alerts, as it does once the broker has settled.
    async fn load_push(hub: &Hub) {
        hub.on_connected();
        hub.on_suback();
        tokio::time::sleep(Duration::from_secs(5)).await;
    }

    fn alert(hub: &Hub, id: &str) -> Option<oikos_api::AlertView> {
        hub.alerts().into_iter().find(|a| a.id == id)
    }

    #[tokio::test(start_paused = true)]
    async fn pending_zone_commands_nothing() {
        let mut rig = Rig::new(None).await;
        let out = rig.pass(0).await;
        assert!(commands(&out).is_empty(), "{out:?}");
        let decision = rig.zone().decision.expect("decision published");
        assert_eq!(decision.source, Source::Idle);
        assert!(!decision.applied);
    }

    #[tokio::test(start_paused = true)]
    async fn active_zone_commands_schedule_target() {
        let mut rig = Rig::new(None).await;
        rig.settings(heat_rules(17.0));
        let out = rig.pass(0).await;
        assert_eq!(
            commands(&out),
            vec![("/ha/temperature/set".to_owned(), "17".to_owned())],
            "{out:?}"
        );
        let decision = rig.zone().decision.unwrap();
        assert_eq!(decision.source, Source::Schedule);
        assert!(decision.applied);
    }

    #[tokio::test(start_paused = true)]
    async fn a_device_in_the_wrong_mode_gets_the_mode_first() {
        let mut rig = Rig::new(None).await;
        rig.sim.set(&rig.hub, 0, "off", 22.0, 20.0);
        rig.settings(heat_rules(17.0));
        let out = rig.pass(0).await;
        assert_eq!(
            commands(&out),
            vec![
                ("/ha/mode/set".to_owned(), "heat".to_owned()),
                ("/ha/temperature/set".to_owned(), "17".to_owned()),
            ]
        );
    }

    #[tokio::test(start_paused = true)]
    async fn frost_commands_pending_zone_then_restores() {
        let mut rig = Rig::new(None).await;
        load_push(&rig.hub).await;
        // A heater switched off in a freezing room.
        rig.sim.set(&rig.hub, 0, "off", 10.0, 5.0);
        let out = rig.pass(0).await;
        assert_eq!(
            commands(&out),
            vec![("/ha/mode/set".to_owned(), "heat".to_owned())],
            "{out:?}"
        );
        let frost = alert(&rig.hub, "climate-safety-living-frost").expect("alert raised");
        assert_eq!(serde_json::to_value(frost.severity).unwrap(), "critical");
        assert!(!frost.resolved);
        assert!(
            rig.zone().decision.unwrap().applied,
            "a sent safety command"
        );

        // Still frosty: nothing more to say.
        let out = rig.pass(61).await;
        assert!(commands(&out).is_empty(), "{out:?}");

        // The room warms up: what was found on the device is put back.
        rig.sim.set(&rig.hub, 0, "heat", 10.0, 18.0);
        let out = rig.pass(122).await;
        assert_eq!(
            commands(&out),
            vec![("/ha/mode/set".to_owned(), "off".to_owned())],
            "{out:?}"
        );
        assert!(
            alert(&rig.hub, "climate-safety-living-frost")
                .unwrap()
                .resolved
        );

        // Restored once.
        let out = rig.pass(183).await;
        assert!(commands(&out).is_empty(), "{out:?}");
        assert!(!rig.zone().decision.unwrap().applied);
    }

    #[tokio::test(start_paused = true)]
    async fn no_command_when_already_at_target() {
        let mut rig = Rig::new(None).await;
        rig.sim.set(&rig.hub, 0, "heat", 17.0, 20.0);
        rig.settings(heat_rules(17.0));
        let out = rig.pass(0).await;
        assert!(commands(&out).is_empty(), "{out:?}");
        assert!(
            rig.zone().decision.unwrap().applied,
            "nothing needed sending"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn rate_limited_to_one_per_minute() {
        let mut rig = Rig::new(None).await;
        rig.settings(heat_rules(17.0));
        assert_eq!(temperature_commands(&rig.pass(0).await), vec![17.0]);
        rig.settings(heat_rules(19.0));
        let out = rig.pass(30).await;
        assert!(commands(&out).is_empty(), "{out:?}");
        assert!(!rig.zone().decision.unwrap().applied, "wanted but not sent");
        assert_eq!(temperature_commands(&rig.pass(61).await), vec![19.0]);
    }

    #[tokio::test(start_paused = true)]
    async fn hand_change_creates_device_override() {
        let mut rig = Rig::new(None).await;
        rig.settings(heat_rules(17.0));
        rig.pass(0).await;
        // Ten minutes later someone turns it up.
        rig.sim.set(&rig.hub, 0, "heat", 22.0, 20.0);
        rig.hub.observe_climate(&rig.id, rig.t0 + 600);
        let overrides = rig.zone().overrides;
        assert_eq!(
            overrides,
            vec![Override {
                entity: Some(rig.id.clone()),
                setpoint: Some(Setpoint::Temp(22.0)),
                mode: Some(Mode::Heat),
                until: rig.t0 + 600 + 240 * 60,
                origin: OverrideOrigin::Hand,
            }]
        );
        // The device is held: the engine does not fight it.
        let out = rig.pass(661).await;
        assert!(commands(&out).is_empty(), "{out:?}");
        let decision = rig.zone().decision.unwrap();
        assert_eq!(decision.source, Source::Schedule);
        assert!(decision.entities[&rig.id].held);
        // More reports of the same state are not new hand changes: the hold does not slide.
        rig.hub.observe_climate(&rig.id, rig.t0 + 1200);
        assert_eq!(rig.zone().overrides, overrides);
    }

    #[tokio::test(start_paused = true)]
    async fn echo_within_30s_is_not_hand_change() {
        let mut rig = Rig::new(None).await;
        rig.settings(heat_rules(17.0));
        rig.pass(0).await;
        // The device still reports an in-between value right after the command.
        rig.sim.set(&rig.hub, 0, "heat", 20.0, 20.0);
        rig.hub.observe_climate(&rig.id, rig.t0 + 10);
        assert!(rig.zone().overrides.is_empty());
        // The same report well after the window is someone's doing.
        rig.hub.observe_climate(&rig.id, rig.t0 + 40);
        assert_eq!(rig.zone().overrides.len(), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn pending_zone_overrides_nothing() {
        let mut rig = Rig::new(None).await;
        rig.sim.set(&rig.hub, 0, "heat", 24.0, 20.0);
        rig.hub.observe_climate(&rig.id, rig.t0 + 600);
        assert!(rig.zone().overrides.is_empty());
    }

    #[tokio::test(start_paused = true)]
    async fn drift_after_reconnect_is_reasserted() {
        let mut rig = Rig::new(None).await;
        rig.settings(heat_rules(17.0));
        rig.pass(0).await;
        let offline = |hub: &Hub, state: &[u8]| {
            hub.ingest("daikin/bridge/availability", state);
        };
        offline(&rig.hub, b"offline");
        rig.hub.observe_climate(&rig.id, rig.t0 + 300);
        offline(&rig.hub, b"online");
        rig.hub.observe_climate(&rig.id, rig.t0 + 400);
        // It comes back with its own default.
        rig.sim.set(&rig.hub, 0, "heat", 24.0, 20.0);
        rig.hub.observe_climate(&rig.id, rig.t0 + 430);
        assert!(
            rig.zone().overrides.is_empty(),
            "drift is not a hand change"
        );
        let out = rig.pass(461).await;
        assert_eq!(temperature_commands(&out), vec![17.0], "{out:?}");
    }

    #[tokio::test(start_paused = true)]
    async fn restart_seeds_ledger_from_retained_decision() {
        let mut rig = Rig::new(None).await;
        rig.sim.set(&rig.hub, 0, "heat", 22.0, 20.0);
        let settings = heat_rules(20.0);
        rig.hub.ingest_message(
            "oikos/climate/zone/living/settings",
            serde_json::to_string(&settings).unwrap().as_bytes(),
            true,
        );
        // The decision the engine retained before the restart: 20 °C, applied.
        let decision = Decision {
            zone: "living".into(),
            mode: Some(Mode::Heat),
            setpoint: Some(Setpoint::Temp(20.0)),
            source: Source::Schedule,
            reason: "Schedule base: 20.0 °C".into(),
            reading: Some(19.0),
            reading_age_s: Some(30),
            outdoor_mean: None,
            home: true,
            applied: true,
            at: rig.t0 - 3600,
            entities: BTreeMap::from([(
                rig.id.clone(),
                DeviceTarget {
                    setpoint: Some(20.0),
                    mode: "heat".into(),
                    held: false,
                },
            )]),
        };
        rig.hub.ingest_message(
            "oikos/climate/zone/living/decision",
            serde_json::to_string(&decision).unwrap().as_bytes(),
            true,
        );
        // It changed while the hub was down: a hand change, not drift (nothing came back
        // from unavailable).
        let out = rig.pass(0).await;
        let overrides = rig.zone().overrides;
        assert_eq!(overrides.len(), 1, "{overrides:?}");
        assert_eq!(overrides[0].entity.as_deref(), Some(rig.id.as_str()));
        assert_eq!(overrides[0].setpoint, Some(Setpoint::Temp(22.0)));
        assert!(commands(&out).is_empty(), "{out:?}");
    }

    #[tokio::test(start_paused = true)]
    async fn an_older_retained_decision_never_overwrites_a_newer_one() {
        let mut rig = Rig::new(None).await;
        let second = feed_daikin(&rig.hub, 1, "other").await;
        rig.sim.set(&rig.hub, 1, "heat", 22.0, 20.0);
        let both = |first: f64, at: u64| {
            let mut entities = BTreeMap::new();
            for id in [&rig.id, &second] {
                entities.insert(
                    id.clone(),
                    DeviceTarget {
                        setpoint: Some(first),
                        mode: "heat".into(),
                        held: false,
                    },
                );
            }
            (entities, at)
        };
        // The same entity listed by two zones' retained decisions (it moved): the newer wins.
        let publish = |zone: &str, first: f64, at: u64| {
            let (entities, at) = both(first, at);
            let decision = Decision {
                zone: zone.into(),
                mode: Some(Mode::Heat),
                setpoint: None,
                source: Source::Schedule,
                reason: String::new(),
                reading: None,
                reading_age_s: None,
                outdoor_mean: None,
                home: true,
                applied: true,
                at,
                entities,
            };
            rig.hub.ingest_message(
                &format!("oikos/climate/zone/{zone}/decision"),
                serde_json::to_string(&decision).unwrap().as_bytes(),
                true,
            );
        };
        publish("living", 22.0, rig.t0 - 100);
        publish("other", 18.0, rig.t0 - 7200);
        for zone in ["living", "other"] {
            rig.hub.set_climate_zone(zone, heat_rules(22.0)).unwrap();
        }
        drain(&mut rig.rx);
        rig.pass(0).await;
        // Both devices are at 22, which is what the newest decision says.
        for zone in rig.hub.climate_view().unwrap().zones {
            assert!(
                zone.overrides.is_empty(),
                "{}: {:?}",
                zone.zone,
                zone.overrides
            );
        }
    }

    #[tokio::test(start_paused = true)]
    async fn three_failures_pause_and_alert() {
        let mut rig = Rig::new(None).await;
        load_push(&rig.hub).await;
        rig.settings(heat_rules(17.0));
        rig.sim.echo = false;
        let alert_id = format!("climate-command-{}", rig.id);
        for at in [0, 61, 122] {
            let out = rig.pass(at).await;
            assert_eq!(temperature_commands(&out), vec![17.0], "at {at}: {out:?}");
        }
        let raised = alert(&rig.hub, &alert_id)
            .unwrap_or_else(|| panic!("alert after 3 failures: {:?}", rig.hub.alerts()));
        assert!(!raised.resolved);
        // Paused for 15 minutes.
        let out = rig.pass(183).await;
        assert!(commands(&out).is_empty(), "{out:?}");
        // Tried again once the pause is over; the device answers this time.
        rig.sim.echo = true;
        let out = rig.pass(122 + 15 * 60).await;
        assert_eq!(temperature_commands(&out), vec![17.0], "{out:?}");
        assert!(alert(&rig.hub, &alert_id).unwrap().resolved);
    }

    #[tokio::test(start_paused = true)]
    async fn failed_publish_is_not_recorded() {
        let mut rig = Rig::new(None).await;
        rig.sim.set(&rig.hub, 0, "heat", 17.0, 20.0);
        rig.settings(heat_rules(17.0));
        rig.pass(0).await;
        rig.settings(heat_rules(19.0));
        rig.hub.set_connection(ConnectionStatus::Offline {
            reason: "test".into(),
        });
        let out = rig.pass(61).await;
        assert!(commands(&out).is_empty(), "{out:?}");
        // The broker is back; the device still reports what it has.
        rig.hub.set_connection(ConnectionStatus::Online);
        rig.hub.observe_climate(&rig.id, rig.t0 + 200);
        assert!(
            rig.zone().overrides.is_empty(),
            "nothing was commanded, so nothing differs from the ledger"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn decision_published_only_when_outcome_changes() {
        let mut rig = Rig::new(None).await;
        rig.settings(heat_rules(17.0));
        let out = rig.pass(0).await;
        assert_eq!(decisions_published(&out), 1);
        // Same outcome, the reading moved: nothing to publish.
        rig.sim.set(&rig.hub, 0, "heat", 17.0, 19.5);
        let out = rig.pass(61).await;
        assert_eq!(decisions_published(&out), 0, "{out:?}");
        rig.settings(heat_rules(18.0));
        let out = rig.pass(122).await;
        assert_eq!(decisions_published(&out), 1, "{out:?}");
    }

    #[tokio::test(start_paused = true)]
    async fn zone_drops_removed_entity() {
        let mut rig = Rig::new(None).await;
        let second = feed_daikin(&rig.hub, 1, "living").await;
        rig.sim.set(&rig.hub, 1, "heat", 22.0, 20.0);
        drain(&mut rig.rx);
        rig.settings(heat_rules(17.0));
        rig.pass(0).await;
        rig.sim.set(&rig.hub, 0, "heat", 24.0, 20.0);
        rig.hub.observe_climate(&rig.id, rig.t0 + 600);
        assert_eq!(rig.zone().overrides.len(), 1);

        // The held device disappears from the house; the other keeps following the rules.
        rig.hub
            .ingest("oikos/climate/daikin_192_168_1_150/climate/config", b"");
        assert!(rig.hub.snapshot().entities.iter().all(|e| e.id != rig.id));
        rig.sim.set(&rig.hub, 1, "heat", 22.0, 20.0);
        let out = rig.pass(700).await;
        assert!(rig.zone().overrides.is_empty(), "its override is dropped");
        assert!(
            out.iter().all(|p| !p.topic.contains("150")),
            "never commanded: {out:?}"
        );
        assert_eq!(temperature_commands(&out), vec![17.0]);
        assert!(rig.zone().decision.unwrap().entities.contains_key(&second));
    }

    #[tokio::test(start_paused = true)]
    async fn hold_until_across_dst() {
        let mut rig = Rig::new(Some("Europe/Paris")).await;
        // Sunday 2026-03-29: 02:30 does not exist in Paris, the clocks jump to 03:00.
        let local = |h, m| {
            chrono_tz::Europe::Paris
                .with_ymd_and_hms(2026, 3, 29, h, m, 0)
                .earliest()
        };
        let settings = ZoneSettings {
            rules: Rules {
                heat: Some(ModeRules {
                    base: Setpoint::Temp(17.0),
                    blocks: vec![Block {
                        days: vec![Day::Sun],
                        from: HhMm(150),
                        to: HhMm(1380),
                        setpoint: Setpoint::Temp(18.0),
                        evidence: None,
                    }],
                }),
                ..Rules::default()
            },
            ..active()
        };
        rig.settings(settings);
        let start = local(0, 50).unwrap().timestamp() as u64;
        rig.t0 = start;
        rig.pass(0).await;
        rig.sim.set(&rig.hub, 0, "heat", 22.0, 20.0);
        rig.hub.observe_climate(&rig.id, start + 600);
        let overrides = rig.zone().overrides;
        assert_eq!(overrides.len(), 1);
        // The next change is "02:30", which is 03:00 once the clocks have moved.
        let three = chrono::Utc
            .with_ymd_and_hms(2026, 3, 29, 1, 0, 0)
            .unwrap()
            .timestamp() as u64;
        assert_eq!(overrides[0].until, three);
    }

    #[tokio::test(start_paused = true)]
    async fn away_after_grace() {
        let mut rig = Rig::new(None).await;
        rig.hub.ingest(
            "oikos/switch/presence/home/config",
            br#"{"name":"Home","unique_id":"presence_home","state_topic":"t/home","command_topic":"t/home/set"}"#,
        );
        let switch = rig
            .hub
            .snapshot()
            .entities
            .into_iter()
            .find(|e| e.id.contains("presence_home"))
            .expect("presence switch")
            .id;
        rig.hub.ingest("t/home", b"ON");
        rig.hub
            .set_climate_house(HouseSettings {
                presence: vec![switch],
                ..HouseSettings::default()
            })
            .unwrap();
        rig.settings(heat_rules(17.0));
        assert_eq!(temperature_commands(&rig.pass(0).await), vec![17.0]);

        // Nobody home for ten minutes: still the schedule.
        rig.hub.ingest("t/home", b"OFF");
        let out = rig.pass(600).await;
        assert!(commands(&out).is_empty(), "{out:?}");
        assert_eq!(rig.zone().decision.unwrap().source, Source::Schedule);
        // Sixteen minutes: away.
        let out = rig.pass(960).await;
        assert_eq!(temperature_commands(&out), vec![16.0], "{out:?}");
        assert_eq!(rig.zone().decision.unwrap().source, Source::Away);
    }

    #[tokio::test(start_paused = true)]
    async fn the_loop_waits_for_the_broker_to_settle_then_commands() {
        let mut rig = Rig::new(None).await;
        rig.settings(heat_rules(17.0));
        let task = rig.hub.start_climate().expect("enabled");
        tokio::time::sleep(Duration::from_secs(5)).await;
        assert!(drain(&mut rig.rx).is_empty(), "still settling");
        let first = tokio::time::timeout(Duration::from_secs(30), async {
            loop {
                let published = rig.rx.recv().await.expect("publisher open");
                rig.sim.on_publish(&rig.hub, &published);
                if published.topic.ends_with("/ha/temperature/set") {
                    return published;
                }
            }
        })
        .await
        .expect("the loop commands the device");
        assert_eq!(first.payload, "17");
        task.abort();
    }

    #[tokio::test]
    async fn no_loop_while_disabled() {
        let (hub, _rx) = hub(false);
        assert!(hub.start_climate().is_none());
        hub.evaluate_climate(1).await;
        hub.observe_climate("climate.none", 1);
    }

    #[tokio::test(start_paused = true)]
    async fn view_reading_is_live_not_the_retained_decision() {
        let mut rig = Rig::new(None).await;
        rig.sim.set(&rig.hub, 0, "heat", 22.0, 19.0);
        let decision = Decision {
            zone: "living".into(),
            mode: Some(Mode::Heat),
            setpoint: None,
            source: Source::Idle,
            reason: String::new(),
            reading: Some(99.0),
            reading_age_s: Some(9999),
            outdoor_mean: None,
            home: true,
            applied: false,
            at: rig.t0,
            entities: BTreeMap::new(),
        };
        rig.hub.ingest_message(
            "oikos/climate/zone/living/decision",
            serde_json::to_string(&decision).unwrap().as_bytes(),
            true,
        );
        let zone = rig.zone();
        assert_eq!(zone.reading, Some(19.0));
        assert!(zone.reading_age_s.unwrap() < 60, "{:?}", zone.reading_age_s);
    }
}
