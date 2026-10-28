//! Database-backed tests for the `settings` table. They skip without `TEST_DATABASE_URL`,
//! like `history_db.rs`.

mod common;

use common::db::fresh_db;
use oikos_os::setup::sections::Sections;
use oikos_os::setup::store::{load_sections, save_sections};

fn sample() -> Sections {
    let mut s = Sections::default();
    s.solcast.enabled = true;
    s.solcast.api_key = Some("solcast-key".into());
    s.frigate.url = Some("http://192.0.2.9:5000".into());
    s.frigate.password = Some("frigate-pass".into());
    s
}

#[tokio::test]
async fn empty_table_loads_defaults() {
    let Some(tdb) = fresh_db().await else { return };
    assert_eq!(load_sections(&tdb.db).await.unwrap(), Sections::default());
    tdb.drop_db().await;
}

#[tokio::test]
async fn save_then_load_round_trips() {
    let Some(tdb) = fresh_db().await else { return };
    let s = sample();
    save_sections(&tdb.db, &s).await.unwrap();
    let back = load_sections(&tdb.db).await.unwrap();
    assert_eq!(back, s);
    assert_eq!(back.solcast.api_key.as_deref(), Some("solcast-key"));
    assert_eq!(back.frigate.password.as_deref(), Some("frigate-pass"));
    tdb.drop_db().await;
}

#[tokio::test]
async fn save_twice_overwrites() {
    let Some(tdb) = fresh_db().await else { return };
    let mut s = sample();
    save_sections(&tdb.db, &s).await.unwrap();
    s.weather.enabled = !s.weather.enabled;
    save_sections(&tdb.db, &s).await.unwrap();
    let back = load_sections(&tdb.db).await.unwrap();
    assert_eq!(back, s);
    let (n,): (i64,) = sqlx::query_as("SELECT count(*) FROM settings")
        .fetch_one(tdb.db.pool())
        .await
        .unwrap();
    assert_eq!(n as usize, oikos_os::setup::sections::SECTION_KEYS.len());
    tdb.drop_db().await;
}
