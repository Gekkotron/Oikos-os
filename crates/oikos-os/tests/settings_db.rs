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

#[tokio::test]
async fn import_copy_fills_the_table_and_sets_the_flag() {
    use std::sync::Arc;

    use oikos_os::history::db::Db;
    use oikos_os::setup::boot::spawn_import_copy;
    use oikos_os::setup::{HubFile, Mode, RestartHook, SetupCtx};

    let Some(tdb) = fresh_db().await else { return };
    let tmp = tempfile::tempdir().unwrap();
    let file = HubFile {
        database_url: Some(tdb.url.clone()),
        ..HubFile::default()
    };
    file.save(tmp.path()).unwrap();
    let imported = sample();
    let ctx = Arc::new(SetupCtx::new(
        tmp.path().to_path_buf(),
        Mode::Run,
        file,
        Some(Db::connect_lazy(&tdb.url).unwrap()),
        imported.clone(),
        RestartHook::Exec,
    ));
    spawn_import_copy(Arc::clone(&ctx)).await.unwrap();
    assert_eq!(load_sections(&tdb.db).await.unwrap(), imported);
    assert!(ctx.file.lock().await.settings_imported);
    assert!(
        HubFile::load(tmp.path())
            .unwrap()
            .unwrap()
            .settings_imported
    );
    tdb.drop_db().await;
}
