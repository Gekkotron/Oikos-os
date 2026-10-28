//! The setup API under `/api/v1/setup`: location, MQTT, database, progress.
//!
//! Every write goes through [`SetupCtx::update_file`] and answers `503` once a restart is
//! pending. No response carries a password.

// The `Err` side is an axum `Response`, built at most once per request.
#![allow(clippy::result_large_err)]

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use oikos_api::{
    DatabaseInput, DatabaseView, GeoPlace, Location, MqttInput, MqttView, Secret, SecretView,
    SetupStatus, SetupStep, TestResult,
};
use serde::Deserialize;
use serde_json::json;
use url::Url;

use crate::config::MqttConfig;
use crate::setup::bundled::{self, Bundled};
use crate::setup::restart::schedule_restart;
use crate::setup::{HubFile, Mode, SetupCtx, probe, secret};
use crate::weather::open_meteo::{self, Endpoints};

const RESTART_DELAY: Duration = Duration::from_millis(500);
const GEOCODE_LIMIT: u8 = 5;
/// Where the compose file's own broker and database listen (host networking).
const BUNDLED_HOST: &str = "localhost";
const BUNDLED_MQTT_PORT: u16 = 1883;
const BUNDLED_DB_PORT: u16 = 5432;

#[derive(Clone)]
struct RouteState {
    ctx: Arc<SetupCtx>,
    geocoding: Endpoints,
    client: reqwest::Client,
}

/// The setup routes, with their own state. `geocoding` is where city lookups go.
pub fn router<S: Clone + Send + Sync + 'static>(
    ctx: Arc<SetupCtx>,
    geocoding: Endpoints,
) -> Router<S> {
    let client = open_meteo::client().unwrap_or_default();
    Router::new()
        .route("/api/v1/setup", get(status))
        .route("/api/v1/setup/geocode", get(geocode))
        .route("/api/v1/setup/location", put(put_location))
        .route("/api/v1/setup/test/mqtt", post(test_mqtt))
        .route("/api/v1/setup/mqtt", put(put_mqtt))
        .route("/api/v1/setup/test/database", post(test_database))
        .route("/api/v1/setup/database", put(put_database))
        .route("/api/v1/setup/step", put(put_step))
        .route("/api/v1/setup/finish", post(finish))
        .with_state(RouteState {
            ctx,
            geocoding,
            client,
        })
}

fn error(status: StatusCode, message: &str) -> Response {
    (status, Json(json!({ "error": message }))).into_response()
}

fn restarting(ctx: &SetupCtx) -> Option<Response> {
    ctx.restarting
        .load(Ordering::SeqCst)
        .then(|| error(StatusCode::SERVICE_UNAVAILABLE, "restarting"))
}

fn save_failed() -> Response {
    error(StatusCode::INTERNAL_SERVER_ERROR, "cannot save hub.json")
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = bytes
            .get(i + 1..i + 3)
            .and_then(|h| std::str::from_utf8(h).ok())
            .and_then(|h| u8::from_str_radix(h, 16).ok());
        match (bytes[i], hex) {
            (b'%', Some(b)) => {
                out.push(b);
                i += 3;
            }
            (b, _) => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn mqtt_view(file: &HubFile, bundled: Option<&Bundled>) -> Option<MqttView> {
    if let Some(m) = &file.mqtt {
        return Some(MqttView {
            host: m.host.clone(),
            port: m.port,
            user: m.user.clone(),
            password: SecretView {
                set: m.password.is_some(),
            },
            client_id: Some(m.client_id.clone()),
            z2m_base: file.z2m_base.clone(),
        });
    }
    bundled.map(|b| MqttView {
        host: BUNDLED_HOST.into(),
        port: BUNDLED_MQTT_PORT,
        user: Some(b.mqtt_user.clone()),
        password: SecretView { set: true },
        client_id: None,
        z2m_base: None,
    })
}

fn database_view(file: &HubFile, bundled: Option<&Bundled>) -> Option<DatabaseView> {
    if let Some(url) = &file.database_url {
        let url = Url::parse(url).ok()?;
        return Some(DatabaseView {
            host: url.host_str()?.to_owned(),
            port: url.port().unwrap_or(BUNDLED_DB_PORT),
            database: percent_decode(url.path().trim_start_matches('/')),
            user: percent_decode(url.username()),
            password: SecretView {
                set: url.password().is_some(),
            },
        });
    }
    bundled.map(|b| DatabaseView {
        host: BUNDLED_HOST.into(),
        port: BUNDLED_DB_PORT,
        database: b.db_name.clone(),
        user: b.db_user.clone(),
        password: SecretView { set: true },
    })
}

async fn status(State(s): State<RouteState>) -> Json<SetupStatus> {
    let bundled = bundled::load(&s.ctx.data_dir);
    let file = s.ctx.file.lock().await.clone();
    Json(SetupStatus {
        mode: match s.ctx.mode {
            Mode::Setup => "setup",
            Mode::Run => "running",
        }
        .into(),
        step: file.step,
        finished: file.finished,
        bundled: bundled.is_some(),
        location: file.location.clone(),
        mqtt: mqtt_view(&file, bundled.as_ref()),
        database: database_view(&file, bundled.as_ref()),
        restarting: s.ctx.restarting.load(Ordering::SeqCst),
    })
}

#[derive(Deserialize)]
struct GeocodeQuery {
    #[serde(default)]
    city: String,
    #[serde(default)]
    country: Option<String>,
}

async fn geocode(State(s): State<RouteState>, Query(q): Query<GeocodeQuery>) -> Response {
    let city = q.city.trim();
    if city.is_empty() {
        return Json(Vec::<GeoPlace>::new()).into_response();
    }
    let country = q
        .country
        .as_deref()
        .map(str::trim)
        .filter(|c| !c.is_empty());
    match open_meteo::geocode_many(&s.client, &s.geocoding, city, country, GEOCODE_LIMIT).await {
        Ok(places) => Json(
            places
                .into_iter()
                .map(|p| GeoPlace {
                    name: p.name,
                    admin: p.admin,
                    country: p.country,
                    latitude: p.latitude,
                    longitude: p.longitude,
                    timezone: p.timezone,
                })
                .collect::<Vec<_>>(),
        )
        .into_response(),
        Err(_) => error(StatusCode::BAD_GATEWAY, "the city lookup failed"),
    }
}

/// Saves under the file lock, then answers with the new status and restarts when due.
async fn finish_write(
    s: &RouteState,
    restart_when_complete: bool,
    edit: impl FnOnce(&mut HubFile),
) -> Response {
    let mut was_complete = false;
    let mut now_complete = false;
    let saved = s
        .ctx
        .update_file(|f| {
            was_complete = f.is_complete();
            edit(f);
            now_complete = f.is_complete();
        })
        .await;
    if saved.is_err() {
        return save_failed();
    }
    let in_run = s.ctx.mode == Mode::Run;
    if in_run || (restart_when_complete && !was_complete && now_complete) {
        schedule_restart(&s.ctx, RESTART_DELAY);
    }
    status(State(s.clone())).await.into_response()
}

async fn put_location(State(s): State<RouteState>, Json(loc): Json<Location>) -> Response {
    if let Some(r) = restarting(&s.ctx) {
        return r;
    }
    if loc.city.trim().is_empty()
        || !(-90.0..=90.0).contains(&loc.latitude)
        || !(-180.0..=180.0).contains(&loc.longitude)
        || loc.timezone.parse::<chrono_tz::Tz>().is_err()
    {
        return error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "The location is not valid",
        );
    }
    finish_write(&s, false, |f| {
        f.location = Some(loc);
        // The chosen place supersedes anything the import left as a fallback.
        f.pending_city = None;
        f.timezone = None;
        f.step = f.step.max(SetupStep::Mqtt);
    })
    .await
}

/// The broker address and password the stored settings (or the bundled ones) hold.
async fn stored_mqtt(ctx: &SetupCtx) -> (Option<MqttConfig>, Option<String>, Option<String>) {
    let file = ctx.file.lock().await.clone();
    if let Some(m) = file.mqtt {
        let (host, pw) = (m.host.clone(), m.password.clone());
        return (Some(m), Some(host), pw);
    }
    match bundled::load(&ctx.data_dir) {
        Some(b) => (None, Some(BUNDLED_HOST.into()), Some(b.mqtt_password)),
        None => (None, None, None),
    }
}

/// The config a request would save, or the refusal.
async fn mqtt_candidate(ctx: &SetupCtx, input: &MqttInput) -> Result<MqttConfig, Response> {
    let (stored, old_host, stored_pw) = stored_mqtt(ctx).await;
    secret::require_for_new_address(old_host.as_deref(), &input.host, &input.password)
        .map_err(|m| error(StatusCode::UNPROCESSABLE_ENTITY, m))?;
    let mut cfg = stored.unwrap_or_default();
    cfg.host = input.host.trim().to_owned();
    cfg.port = input.port;
    cfg.user = input.user.clone().filter(|u| !u.is_empty());
    cfg.password = secret::apply(stored_pw, input.password.clone());
    if let Some(id) = input.client_id.as_deref().filter(|c| !c.is_empty()) {
        cfg.client_id = id.to_owned();
    }
    Ok(cfg)
}

async fn test_mqtt(State(s): State<RouteState>, Json(input): Json<MqttInput>) -> Response {
    match mqtt_candidate(&s.ctx, &input).await {
        Ok(cfg) => Json(probe::mqtt(&cfg).await).into_response(),
        Err(r) => r,
    }
}

async fn put_mqtt(State(s): State<RouteState>, Json(input): Json<MqttInput>) -> Response {
    if let Some(r) = restarting(&s.ctx) {
        return r;
    }
    let cfg = match mqtt_candidate(&s.ctx, &input).await {
        Ok(cfg) => cfg,
        Err(r) => return r,
    };
    // Probe first: wrong credentials must never reach hub.json, or a running house would
    // restart into a broker it cannot reach.
    let result = probe::mqtt(&cfg).await;
    if !result.ok {
        return (StatusCode::UNPROCESSABLE_ENTITY, Json(result)).into_response();
    }
    let z2m = input.z2m_base.clone().filter(|z| !z.is_empty());
    finish_write(&s, true, |f| {
        f.mqtt = Some(cfg);
        if z2m.is_some() {
            f.z2m_base = z2m;
        }
        f.step = f.step.max(SetupStep::Database);
    })
    .await
}

/// The URL a request would save, built on the stored URL (so a kept password stays as it is).
async fn database_candidate(ctx: &SetupCtx, input: &DatabaseInput) -> Result<String, Response> {
    let file = ctx.file.lock().await.clone();
    let stored = file
        .database_url
        .as_deref()
        .and_then(|u| Url::parse(u).ok());
    let bundled = bundled::load(&ctx.data_dir);
    let old_host = match (&stored, &bundled) {
        (Some(u), _) => u.host_str().map(str::to_owned),
        (None, Some(_)) => Some(BUNDLED_HOST.to_owned()),
        (None, None) => None,
    };
    secret::require_for_new_address(old_host.as_deref(), &input.host, &input.password)
        .map_err(|m| error(StatusCode::UNPROCESSABLE_ENTITY, m))?;
    let invalid = || error(StatusCode::UNPROCESSABLE_ENTITY, "The address is not valid");
    let mut url = match stored {
        Some(u) => u,
        None => {
            let mut u = Url::parse("postgres://placeholder/").map_err(|_| invalid())?;
            if let Some(b) = &bundled {
                u.set_password(Some(&b.db_password))
                    .map_err(|_| invalid())?;
            }
            u
        }
    };
    url.set_host(Some(input.host.trim()))
        .map_err(|_| invalid())?;
    url.set_port(Some(input.port)).map_err(|_| invalid())?;
    url.set_username(&input.user).map_err(|_| invalid())?;
    url.set_path(&format!("/{}", input.database));
    match &input.password {
        Secret::Keep => {}
        Secret::Clear => url.set_password(None).map_err(|_| invalid())?,
        Secret::Set(p) => url.set_password(Some(p)).map_err(|_| invalid())?,
    }
    Ok(url.into())
}

async fn test_database(State(s): State<RouteState>, Json(input): Json<DatabaseInput>) -> Response {
    match database_candidate(&s.ctx, &input).await {
        Ok(url) => Json(probe::database(&url).await).into_response(),
        Err(r) => r,
    }
}

async fn put_database(State(s): State<RouteState>, Json(input): Json<DatabaseInput>) -> Response {
    if let Some(r) = restarting(&s.ctx) {
        return r;
    }
    let url = match database_candidate(&s.ctx, &input).await {
        Ok(url) => url,
        Err(r) => return r,
    };
    let result: TestResult = probe::database(&url).await;
    if !result.ok {
        return (StatusCode::UNPROCESSABLE_ENTITY, Json(result)).into_response();
    }
    finish_write(&s, true, |f| {
        f.database_url = Some(url);
        f.step = f.step.max(SetupStep::Devices);
    })
    .await
}

#[derive(Deserialize)]
struct StepBody {
    step: SetupStep,
}

async fn put_step(State(s): State<RouteState>, Json(body): Json<StepBody>) -> Response {
    if let Some(r) = restarting(&s.ctx) {
        return r;
    }
    let saved = s.ctx.update_file(|f| f.step = body.step).await;
    if saved.is_err() {
        return save_failed();
    }
    status(State(s)).await.into_response()
}

async fn finish(State(s): State<RouteState>) -> Response {
    if let Some(r) = restarting(&s.ctx) {
        return r;
    }
    let saved = s
        .ctx
        .update_file(|f| {
            f.finished = true;
            f.step = SetupStep::Done;
        })
        .await;
    if saved.is_err() {
        return save_failed();
    }
    status(State(s)).await.into_response()
}
