//! Connection tests for the broker and the database. Nothing here returns or logs a password.

use std::time::Duration;

use oikos_api::TestResult;
use rumqttc::{AsyncClient, ConnectReturnCode, ConnectionError, Event, MqttOptions, Packet};
use serde_json::json;
use sqlx::Row;

use crate::config::MqttConfig;
use crate::history::db::Db;

const PROBE_TIMEOUT: Duration = Duration::from_secs(5);

fn result(ok: bool, message: &str) -> TestResult {
    TestResult {
        ok,
        message: message.to_owned(),
        detail: None,
    }
}

fn refusal(code: ConnectReturnCode) -> TestResult {
    match code {
        ConnectReturnCode::BadUserNamePassword | ConnectReturnCode::NotAuthorized => {
            result(false, "bad credentials")
        }
        _ => result(false, "refused"),
    }
}

/// Connects with a `<client_id>-probe` client id and waits for the broker's answer.
pub async fn mqtt(cfg: &MqttConfig) -> TestResult {
    let mut options = MqttOptions::new(
        format!("{}-probe", cfg.client_id),
        cfg.host.clone(),
        cfg.port,
    );
    if let Some(user) = &cfg.user {
        options.set_credentials(user, cfg.password.as_deref().unwrap_or(""));
    }
    options.set_clean_session(true);
    let (client, mut eventloop) = AsyncClient::new(options, 4);
    let outcome = tokio::time::timeout(PROBE_TIMEOUT, async {
        loop {
            match eventloop.poll().await {
                Ok(Event::Incoming(Packet::ConnAck(ack))) => {
                    return if ack.code == ConnectReturnCode::Success {
                        result(true, "connected")
                    } else {
                        refusal(ack.code)
                    };
                }
                Ok(_) => {}
                Err(ConnectionError::ConnectionRefused(code)) => return refusal(code),
                Err(_) => return result(false, "refused"),
            }
        }
    })
    .await
    .unwrap_or_else(|_| result(false, "timed out"));
    let _ = client.try_disconnect();
    outcome
}

fn db_failure(error: &sqlx::Error) -> TestResult {
    match error {
        sqlx::Error::PoolTimedOut => result(false, "timed out"),
        sqlx::Error::Database(e) if matches!(e.code().as_deref(), Some("28P01" | "28000")) => {
            result(false, "bad credentials")
        }
        sqlx::Error::Database(e) => result(false, e.message()),
        _ => result(false, "refused"),
    }
}

/// Connects, checks TimescaleDB is there, runs the migrations and reports how far back the
/// history goes (`detail.history_since`, RFC 3339 or null). `url` may hold a password: nothing
/// from it is echoed.
pub async fn database(url: &str) -> TestResult {
    let Ok(db) = Db::connect_lazy(url) else {
        return result(false, "the address is not valid");
    };
    let outcome = check_database(&db).await;
    db.close().await;
    outcome
}

async fn check_database(db: &Db) -> TestResult {
    let ext = sqlx::query("SELECT 1 FROM pg_available_extensions WHERE name = 'timescaledb'")
        .fetch_optional(db.pool())
        .await;
    match ext {
        Err(e) => return db_failure(&e),
        Ok(None) => return result(false, "TimescaleDB extension is not available"),
        Ok(Some(_)) => {}
    }
    if let Err(e) = db.migrate().await {
        return result(false, &format!("migration failed: {e}"));
    }
    let since = sqlx::query("SELECT MIN(time) FROM entity_state")
        .fetch_one(db.pool())
        .await
        .and_then(|row| row.try_get::<Option<chrono::DateTime<chrono::Utc>>, _>(0));
    match since {
        Ok(t) => TestResult {
            ok: true,
            message: "connected".into(),
            detail: Some(json!({ "history_since": t.map(|t| t.to_rfc3339()) })),
        },
        Err(e) => db_failure(&e),
    }
}
