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
use oikos_core::model::{Availability, EntityState, StateValue};
use oikos_os::history::db::Db;
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
            values: vec![value_row("a", 1_000_000, 1.0), value_row("a", 2_000_000, 2.0)],
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
    tdb.db
        .upsert_hourly(&[("a".into(), first)])
        .await
        .unwrap();
    tdb.db
        .upsert_hourly(&[("a".into(), second.clone())])
        .await
        .unwrap();
    let hours = tdb
        .db
        .hourly("a", 0, 3_600_000_000)
        .await
        .unwrap();
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
    let carry = tdb
        .db
        .hourly_carry(2 * hour, &["a".into()])
        .await
        .unwrap();
    assert_eq!(carry.get("a"), Some(&1.0));

    tdb.drop_db().await;
}

