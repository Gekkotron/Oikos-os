//! Database-backed tests for the history layer.
//!
//! These tests need a reachable TimescaleDB. Set
//! `TEST_DATABASE_URL=postgres://oikos:change-me@localhost:5432/oikos` to run them against
//! the compose file in `deploy/timescaledb/`. Without the variable each test prints
//! `skipping: TEST_DATABASE_URL unset` and returns, so the suite stays green on machines
//! without a database.

use std::sync::atomic::{AtomicU64, Ordering};

use oikos_core::history::buckets::Bucket;
use oikos_core::history::{AvailabilityRow, Incoming, Rows, ValueRow};
use oikos_core::model::{Availability, ClimateState, EntityState, StateValue};
use oikos_os::history::climate::ClimateDecisionRow;
use oikos_os::history::db::Db;
use oikos_os::history::rollup::rollup_once;
use sqlx::{PgPool, postgres::PgPoolOptions};
use url::Url;

static COUNTER: AtomicU64 = AtomicU64::new(0);

struct TestDb {
    db: Db,
    admin: PgPool,
    name: String,
}

impl TestDb {
    async fn drop_db(self) {
        self.db.close().await;
        let _ = sqlx::query(&format!(
            "SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE datname = '{}'",
            self.name
        ))
        .execute(&self.admin)
        .await;
        let _ = sqlx::query(&format!("DROP DATABASE IF EXISTS \"{}\"", self.name))
            .execute(&self.admin)
            .await;
        self.admin.close().await;
    }
}

async fn fresh_db() -> Option<TestDb> {
    let base_url = match std::env::var("TEST_DATABASE_URL") {
        Ok(v) if !v.is_empty() => v,
        _ => {
            eprintln!("skipping: TEST_DATABASE_URL unset");
            return None;
        }
    };
    let n = COUNTER.fetch_add(1, Ordering::SeqCst);
    let name = format!("oikos_test_{}_{}", std::process::id(), n);
    let admin = PgPoolOptions::new()
        .max_connections(1)
        .connect(&base_url)
        .await
        .ok()?;
    sqlx::query(&format!("CREATE DATABASE \"{name}\""))
        .execute(&admin)
        .await
        .ok()?;
    let mut url = Url::parse(&base_url).ok()?;
    url.set_path(&format!("/{name}"));
    let db = Db::connect_lazy(url.as_str()).ok()?;
    db.migrate().await.ok()?;
    Some(TestDb { db, admin, name })
}

fn value_row(id: &str, at_micros: i64, num: f64) -> ValueRow {
    ValueRow {
        at: at_micros,
        id: id.into(),
        value: StateValue::Number(num),
        num: Some(num),
    }
}

fn availability_row(id: &str, at_micros: i64, availability: Availability) -> AvailabilityRow {
    AvailabilityRow {
        at: at_micros,
        id: id.into(),
        availability,
    }
}

fn make_bucket(start: i64, last: f64, max: f64, increase: f64) -> Bucket {
    Bucket {
        start,
        covered: 3_600_000_000,
        integral: last,
        min: last,
        max,
        last,
        increase,
    }
}

#[tokio::test]
async fn insert_then_load_latest_round_trips() {
    let Some(tdb) = fresh_db().await else { return };
    let rows = Rows {
        values: vec![
            value_row("a", 1_000_000, 21.0),
            value_row("a", 1_000_001, 22.0),
        ],
        availability: vec![availability_row("a", 1_000_000, Availability::Online)],
    };
    tdb.db.insert(&rows).await.unwrap();

    let mut lw = tdb.db.load_latest().await.unwrap();
    let mut out = Rows::default();
    lw.diff(
        &Incoming {
            at: 2_000_000,
            id: "a".into(),
            state: EntityState {
                value: StateValue::Number(22.0),
                attributes: serde_json::Map::new(),
                availability: Availability::Online,
                last_update: Some(2),
            },
            num: Some(22.0),
        },
        &mut out,
    );
    assert!(out.is_empty(), "{out:?}");

    tdb.drop_db().await;
}

#[tokio::test]
async fn same_microsecond_keeps_insert_order() {
    let Some(tdb) = fresh_db().await else { return };
    let rows = Rows {
        values: vec![
            value_row("a", 1_000_000, 1.0),
            value_row("a", 1_000_000, 2.0),
        ],
        availability: vec![],
    };
    tdb.db.insert(&rows).await.unwrap();
    let got = tdb.db.samples(0, 2_000_000, Some("a")).await.unwrap();
    assert_eq!(got.len(), 2);
    assert_eq!(got[0].1.num, 1.0);
    assert_eq!(got[1].1.num, 2.0);

    tdb.drop_db().await;
}

#[tokio::test]
async fn last_changes_and_recorded() {
    let Some(tdb) = fresh_db().await else { return };
    tdb.db
        .insert(&Rows {
            values: vec![
                value_row("a", 1_000_000, 1.0),
                value_row("a", 2_000_000, 2.0),
            ],
            availability: vec![],
        })
        .await
        .unwrap();

    assert!(tdb.db.is_recorded("a").await.unwrap());
    assert!(!tdb.db.is_recorded("ghost").await.unwrap());

    let changes = tdb
        .db
        .last_changes(&["a".into(), "ghost".into()])
        .await
        .unwrap();
    assert_eq!(changes.len(), 1);
    assert_eq!(changes.get("a"), Some(&2_000_000));

    tdb.drop_db().await;
}

#[tokio::test]
async fn hourly_upsert_replaces() {
    let Some(tdb) = fresh_db().await else { return };
    let first = make_bucket(0, 1.0, 1.0, 0.0);
    let second = Bucket {
        last: 5.0,
        max: 5.0,
        increase: 2.0,
        ..first.clone()
    };
    tdb.db.upsert_hourly(&[("a".into(), first)]).await.unwrap();
    tdb.db
        .upsert_hourly(&[("a".into(), second.clone())])
        .await
        .unwrap();
    let hours = tdb.db.hourly("a", 0, 3_600_000_000).await.unwrap();
    assert_eq!(hours.len(), 1);
    assert_eq!(hours[0].last, 5.0);
    assert_eq!(hours[0].max, 5.0);
    assert_eq!(hours[0].increase, 2.0);

    tdb.drop_db().await;
}

#[tokio::test]
async fn hourly_carry_takes_latest_before() {
    let Some(tdb) = fresh_db().await else { return };
    let hour = 3_600_000_000i64;
    tdb.db
        .upsert_hourly(&[
            ("a".into(), make_bucket(0, 1.0, 1.0, 0.0)),
            ("a".into(), make_bucket(2 * hour, 3.0, 3.0, 0.0)),
        ])
        .await
        .unwrap();
    let carry = tdb.db.hourly_carry(2 * hour, &["a".into()]).await.unwrap();
    assert_eq!(carry.get("a"), Some(&1.0));

    tdb.drop_db().await;
}

const HOUR: i64 = 3_600_000_000;
const MINUTE: i64 = 60_000_000;

#[tokio::test]
async fn replay_after_restart_writes_nothing() {
    let Some(tdb) = fresh_db().await else { return };
    tdb.db
        .insert(&Rows {
            values: vec![value_row("a", 1_000_000, 20.0)],
            availability: vec![availability_row("a", 1_000_000, Availability::Online)],
        })
        .await
        .unwrap();
    let mut lw = tdb.db.load_latest().await.unwrap();
    let mut rows = Rows::default();
    lw.diff(
        &Incoming {
            at: 2_000_000,
            id: "a".into(),
            state: EntityState {
                value: StateValue::Number(20.0),
                attributes: serde_json::Map::new(),
                availability: Availability::Online,
                last_update: Some(2),
            },
            num: Some(20.0),
        },
        &mut rows,
    );
    assert!(rows.is_empty(), "same state after restart writes nothing");

    let mut rows = Rows::default();
    lw.diff(
        &Incoming {
            at: 3_000_000,
            id: "a".into(),
            state: EntityState {
                value: StateValue::Number(21.0),
                attributes: serde_json::Map::new(),
                availability: Availability::Online,
                last_update: Some(3),
            },
            num: Some(21.0),
        },
        &mut rows,
    );
    assert_eq!(rows.values.len(), 1);
    assert_eq!(rows.values[0].num, Some(21.0));

    tdb.drop_db().await;
}

#[tokio::test]
async fn rollup_fills_hourly_with_carry() {
    let Some(tdb) = fresh_db().await else { return };
    tdb.db
        .insert(&Rows {
            values: vec![
                value_row("a", 15 * MINUTE, 10.0),
                value_row("a", HOUR + 30 * MINUTE, 20.0),
            ],
            availability: vec![],
        })
        .await
        .unwrap();

    rollup_once(&tdb.db, 3 * HOUR, i64::MAX).await.unwrap();
    let hours = tdb.db.hourly("a", 0, 3 * HOUR).await.unwrap();
    assert_eq!(hours.len(), 2, "{hours:?}");
    assert_eq!(hours[1].min, 10.0);
    assert_eq!(hours[1].max, 20.0);
    assert_eq!(hours[1].integral, 15.0);

    tdb.drop_db().await;
}

#[tokio::test]
async fn late_rows_recompute_their_hour() {
    let Some(tdb) = fresh_db().await else { return };
    tdb.db
        .insert(&Rows {
            values: vec![
                value_row("a", 15 * MINUTE, 10.0),
                value_row("a", HOUR + 30 * MINUTE, 20.0),
            ],
            availability: vec![],
        })
        .await
        .unwrap();
    rollup_once(&tdb.db, 3 * HOUR, i64::MAX).await.unwrap();

    tdb.db
        .insert(&Rows {
            values: vec![value_row("a", HOUR + 45 * MINUTE, 30.0)],
            availability: vec![],
        })
        .await
        .unwrap();
    rollup_once(&tdb.db, 3 * HOUR, HOUR).await.unwrap();

    let hours = tdb.db.hourly("a", HOUR, 2 * HOUR).await.unwrap();
    assert_eq!(hours.len(), 1);
    assert_eq!(hours[0].max, 30.0);
    assert_eq!(hours[0].last, 30.0);

    tdb.drop_db().await;
}

#[tokio::test]
async fn series_24h_carries_into_quiet_windows() {
    let Some(tdb) = fresh_db().await else { return };
    let now = 48 * HOUR; // arbitrary "present"
    tdb.db
        .insert(&Rows {
            values: vec![value_row("a", now - 2 * HOUR, 5.0)],
            availability: vec![],
        })
        .await
        .unwrap();
    rollup_once(&tdb.db, now, i64::MAX).await.unwrap();

    let history = oikos_os::history::History::with_db(tdb.db.clone(), chrono_tz::UTC);
    let series = oikos_os::history::query::series(&history, "a", oikos_api::HistoryRange::Day, now)
        .await
        .expect("series");

    // Points are (t, mean, min, max) with t in unix seconds.
    assert!(!series.points.is_empty(), "{series:?}");
    // The first point is at or after now - 2h (no point for the earlier quiet windows).
    let first_t = series.points[0].0;
    let two_hours_ago_s = (now - 2 * HOUR) / 1_000_000;
    assert!(
        first_t >= two_hours_ago_s - 600,
        "{first_t} vs {two_hours_ago_s}"
    );
    // Every point carries 5.0.
    for p in &series.points {
        assert_eq!(p.1, 5.0, "mean");
        assert_eq!(p.2, 5.0, "min");
        assert_eq!(p.3, 5.0, "max");
    }

    tdb.drop_db().await;
}

#[tokio::test]
async fn today_power_and_unrecorded() {
    let Some(tdb) = fresh_db().await else { return };
    // Reference "now" chosen so UTC midnight is a round micros count. HOUR = 1h.
    let midnight = 100 * 24 * HOUR;
    let now = midnight + 3 * HOUR;
    tdb.db
        .insert(&Rows {
            values: vec![value_row("a", midnight + HOUR, 1000.0)],
            availability: vec![],
        })
        .await
        .unwrap();

    let history = oikos_os::history::History::with_db(tdb.db.clone(), chrono_tz::UTC);
    let today = oikos_os::history::query::today(&history, "a", oikos_api::EnergyKind::Power, now)
        .await
        .expect("today");
    assert_eq!(today.since, midnight / 1_000_000);
    assert_eq!(today.kwh, Some(2.0));

    let err =
        oikos_os::history::query::series(&history, "ghost", oikos_api::HistoryRange::Day, now)
            .await
            .expect_err("not recorded");
    assert_eq!(err.code(), "not_recorded");

    let changes = oikos_os::history::query::last_change(&history, &["a".into(), "ghost".into()])
        .await
        .expect("last_change");
    assert_eq!(changes.items.len(), 2);
    assert!(changes.items[0].recorded);
    assert!(!changes.items[1].recorded);
    assert_eq!(changes.items[1].changed_at, None);

    tdb.drop_db().await;
}

#[tokio::test]
async fn writer_survives_unreachable_database_at_boot() {
    use std::time::Duration;

    let cfg = oikos_os::config::HistoryConfig {
        url: "postgres://nouser:nopass@127.0.0.1:1/none".into(),
        buffer_rows: 100,
    };
    let (history, handle) =
        oikos_os::history::History::spawn_writer_for_tests(&cfg, chrono_tz::UTC)
            .expect("lazy pool");

    history.offer(Incoming {
        at: 1_000_000,
        id: "a".into(),
        state: EntityState {
            value: StateValue::OnOff(true),
            attributes: serde_json::Map::new(),
            availability: Availability::Online,
            last_update: Some(1),
        },
        num: None,
    });

    // Wait long enough for sqlx's acquire_timeout (5 s) to fire plus a retry tick: the
    // regressed writer would have called migrate(), timed out, and returned by now.
    tokio::time::sleep(Duration::from_secs(8)).await;
    assert!(
        !handle.is_finished(),
        "writer must not exit when the database is unreachable at boot"
    );

    handle.abort();
}

fn decision(at: i64, zone: &str, entity: &str, applied: bool, reason: &str) -> ClimateDecisionRow {
    ClimateDecisionRow {
        at,
        zone: zone.into(),
        entity_id: entity.into(),
        mode: "heat".into(),
        setpoint: Some(19.5),
        source: "schedule".into(),
        reason: reason.into(),
        applied,
    }
}

fn climate_row(id: &str, at: i64, target: Option<f64>, mode: &str) -> ValueRow {
    ValueRow {
        at,
        id: id.into(),
        value: StateValue::Climate(ClimateState {
            current: Some(20.0),
            target,
            mode: Some(mode.into()),
            fan_mode: None,
            action: None,
        }),
        num: None,
    }
}

#[tokio::test]
async fn climate_decisions_round_trip() {
    let Some(tdb) = fresh_db().await else { return };
    let mut empty = decision(3_000_000, "z1", "", false, "no devices");
    empty.mode = String::new();
    empty.setpoint = None;
    let rows = vec![
        decision(1_000_000, "z1", "climate.a", true, "first"),
        decision(2_000_000, "z1", "climate.a", false, "second"),
        empty.clone(),
        decision(2_500_000, "z2", "climate.b", true, "other zone"),
    ];
    tdb.db.insert_climate_decisions(&rows).await.unwrap();

    let got = tdb.db.climate_decisions("z1", 10).await.unwrap();
    assert_eq!(got.len(), 3);
    assert_eq!(got[0], empty);
    assert_eq!(got[1], rows[1]);
    assert_eq!(got[2], rows[0]);
    let limited = tdb.db.climate_decisions("z1", 1).await.unwrap();
    assert_eq!(limited, vec![empty]);

    tdb.drop_db().await;
}

#[tokio::test]
async fn climate_commands_filters_applied_and_entities() {
    let Some(tdb) = fresh_db().await else { return };
    let rows = vec![
        decision(1_000_000, "z1", "climate.a", true, "in"),
        decision(2_000_000, "z1", "climate.a", false, "not applied"),
        decision(3_000_000, "z1", "climate.b", true, "other entity"),
        decision(4_000_000, "z1", "climate.a", true, "also in"),
        decision(9_000_000, "z1", "climate.a", true, "after window"),
    ];
    tdb.db.insert_climate_decisions(&rows).await.unwrap();

    let got = tdb
        .db
        .climate_commands(&["climate.a".to_string()], 0, 5_000_000)
        .await
        .unwrap();
    let reasons: Vec<&str> = got.iter().map(|r| r.reason.as_str()).collect();
    assert_eq!(reasons, ["in", "also in"]);
    assert!(
        tdb.db
            .climate_commands(&[], 0, 5_000_000)
            .await
            .unwrap()
            .is_empty()
    );

    tdb.drop_db().await;
}

#[tokio::test]
async fn climate_targets_reads_jsonb_and_carries_prior_value() {
    let Some(tdb) = fresh_db().await else { return };
    let t: i64 = 1_800_000_000_000_000;
    let h = 3_600_000_000;
    let m = 60_000_000;
    let rows = Rows {
        values: vec![
            climate_row("climate.a", t - 2 * h, Some(18.0), "heat"),
            climate_row("climate.a", t - 30 * m, Some(20.0), "heat"),
            climate_row("climate.a", t + 10 * m, None, "off"),
            climate_row("climate.b", t - 15 * m, Some(25.0), "cool"),
            value_row("climate.a", t - 5 * m, 7.0),
        ],
        availability: vec![],
    };
    tdb.db.insert(&rows).await.unwrap();

    let got = tdb
        .db
        .climate_targets("climate.a", t - h, t + h)
        .await
        .unwrap();
    assert_eq!(
        got,
        vec![
            (t - h, Some(18.0), Some("heat".to_string())),
            (t - 30 * m, Some(20.0), Some("heat".to_string())),
            (t + 10 * m, None, Some("off".to_string())),
        ]
    );

    tdb.drop_db().await;
}

#[tokio::test]
async fn mean_since_averages_num() {
    let Some(tdb) = fresh_db().await else { return };
    let rows = Rows {
        values: vec![
            value_row("out", 1_000_000, 10.0),
            value_row("out", 2_000_000, 20.0),
            value_row("out", 9_000_000, 99.0),
            value_row("other", 1_500_000, 50.0),
        ],
        availability: vec![],
    };
    tdb.db.insert(&rows).await.unwrap();

    assert_eq!(
        tdb.db.mean_since("out", 0, 5_000_000).await.unwrap(),
        Some(15.0)
    );
    assert_eq!(tdb.db.mean_since("nope", 0, 5_000_000).await.unwrap(), None);

    tdb.drop_db().await;
}
