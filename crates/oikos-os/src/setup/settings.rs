//! `GET`/`PUT /api/v1/settings/{section}` and the Solcast and Frigate connection tests.
//!
//! A `PUT` is Run mode only. It builds the candidate sections, checks them the way the boot
//! will (`assemble` then `validate_all`), saves them to the database, then to the cache, and
//! restarts the hub so every service picks the change up. No body, log line or error from here
//! carries a secret: reads show `{"set": bool}` and failures use fixed text.

// The `Err` side is an axum `Response`, built at most once per request.
#![allow(clippy::result_large_err)]

use std::sync::atomic::Ordering;
use std::time::Duration;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use oikos_api::{
    FrigateInput, FrigateView, PushInput, PushView, Secret, SecretView, SolcastInput, SolcastView,
    TestResult,
};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use crate::config::FrigateConfig;
use crate::setup::routes::{RouteState, error, restarting};
use crate::setup::sections::{
    AdvancedSection, PushSection, SECTION_KEYS, Sections, SolcastSection, WeatherSection, assemble,
};
use crate::setup::store::{save_sections, write_cache};
use crate::setup::{HubFile, Mode, probe, restart, secret};

const RESTART_DELAY: Duration = Duration::from_millis(500);

pub(super) fn routes() -> Router<RouteState> {
    Router::new()
        .route(
            "/api/v1/settings/{section}",
            get(get_section).put(put_section),
        )
        .route("/api/v1/setup/test/solcast", post(test_solcast))
        .route("/api/v1/setup/test/frigate", post(test_frigate))
}

fn unprocessable(message: &str) -> Response {
    error(StatusCode::UNPROCESSABLE_ENTITY, message)
}

fn known(section: &str) -> bool {
    SECTION_KEYS.contains(&section)
}

/// The project id a service-account key JSON names. The JSON and any parse error stay unlogged.
fn project_id(key: &str) -> Option<String> {
    serde_json::from_str::<Value>(key)
        .ok()?
        .get("project_id")?
        .as_str()
        .map(str::to_owned)
}

fn view(section: &str, s: &Sections) -> Value {
    fn to<T: serde::Serialize>(v: &T) -> Value {
        serde_json::to_value(v).expect("a view serializes")
    }
    let set = |v: &Option<String>| SecretView { set: v.is_some() };
    match section {
        "weather" => to(&s.weather),
        "solcast" => to(&SolcastView {
            enabled: s.solcast.enabled,
            sites: s.solcast.sites.clone(),
            daily_limit: s.solcast.daily_limit,
            api_key: set(&s.solcast.api_key),
        }),
        "frigate" => {
            let f = &s.frigate;
            to(&FrigateView {
                url: f.url.clone(),
                user: f.user.clone(),
                password: set(&f.password),
                media_url: f.media_url.clone(),
                bridge: f.bridge,
                labels: f.labels.clone(),
                cameras: f.cameras.clone(),
                zones: f.zones.clone(),
                cooldown: f.cooldown,
                severity: f.severity.clone(),
                ttl: f.ttl,
                topic_prefix: f.topic_prefix.clone(),
                live: f.live,
            })
        }
        "push" => {
            let p = &s.push;
            to(&PushView {
                service_account: set(&p.service_account),
                project_id: p.service_account.as_deref().and_then(project_id),
                prefix: p.prefix.clone(),
                legacy_prefix: p.legacy_prefix.clone(),
                retry_interval: p.retry_interval,
                max_retries: p.max_retries,
                history_size: p.history_size,
                history_days: p.history_days,
            })
        }
        "climate" => to(&s.climate),
        _ => to(&s.advanced),
    }
}

async fn get_section(State(s): State<RouteState>, Path(section): Path<String>) -> Response {
    if !known(&section) {
        return error(StatusCode::NOT_FOUND, "unknown section");
    }
    let sections = s.ctx.sections.lock().await.clone();
    Json(view(&section, &sections)).into_response()
}

/// The body as `T`, or a fixed refusal (a serde error could quote the value).
fn parse<T: DeserializeOwned>(body: Value, section: &str) -> Result<T, Response> {
    serde_json::from_value(body)
        .map_err(|_| unprocessable(&format!("The {section} settings are not valid")))
}

/// `scheme://host[:port]` of a Frigate address; a path or a default port never counts.
fn address_of(url: &str) -> String {
    crate::media::origin_of(url).unwrap_or_else(|| url.trim().to_lowercase())
}

/// The Frigate config a request would save, on top of the stored one.
fn frigate_candidate(old: &FrigateConfig, input: FrigateInput) -> Result<FrigateConfig, Response> {
    let clean = |v: Option<String>| v.map(|v| v.trim().to_owned()).filter(|v| !v.is_empty());
    let url = clean(input.url).map(|u| u.trim_end_matches('/').to_owned());
    if let Some(url) = &url {
        if crate::media::origin_of(url).is_none() {
            return Err(unprocessable("The Frigate address is not valid"));
        }
        let old_address = old.url.as_deref().map(address_of);
        secret::require_for_new_address(old_address.as_deref(), &address_of(url), &input.password)
            .map_err(unprocessable)?;
    }
    let mut cfg = old.clone();
    cfg.url = url;
    cfg.user = clean(input.user);
    cfg.password = secret::apply(old.password.clone(), input.password);
    cfg.media_url = clean(input.media_url);
    cfg.bridge = input.bridge;
    cfg.labels = input.labels;
    cfg.cameras = input.cameras;
    cfg.zones = input.zones;
    cfg.live = input.live;
    if let Some(v) = input.cooldown {
        cfg.cooldown = v;
    }
    if let Some(v) = input.severity {
        cfg.severity = v;
    }
    if let Some(v) = input.ttl {
        cfg.ttl = v;
    }
    if let Some(v) = input.topic_prefix {
        cfg.topic_prefix = v;
    }
    Ok(cfg)
}

/// Applies `body` to the `section` of `next`.
fn merge(section: &str, body: Value, next: &mut Sections, file: &HubFile) -> Result<(), Response> {
    match section {
        "weather" => {
            let weather: WeatherSection = parse(body, section)?;
            if weather.enabled && file.location.is_none() {
                return Err(unprocessable("Set the home location first"));
            }
            next.weather = weather;
        }
        "solcast" => {
            let input: SolcastInput = parse(body, section)?;
            let old = &next.solcast;
            next.solcast = SolcastSection {
                enabled: input.enabled,
                sites: input.sites.iter().map(|s| s.trim().to_owned()).collect(),
                daily_limit: input.daily_limit.unwrap_or(old.daily_limit),
                api_key: secret::apply(old.api_key.clone(), input.api_key),
            };
        }
        "frigate" => next.frigate = frigate_candidate(&next.frigate, parse(body, section)?)?,
        "push" => {
            let input: PushInput = parse(body, section)?;
            if let Secret::Set(key) = &input.service_account
                && project_id(key).is_none()
            {
                return Err(unprocessable("The service account key is not valid"));
            }
            let old = &next.push;
            next.push = PushSection {
                service_account: secret::apply(old.service_account.clone(), input.service_account),
                prefix: input.prefix.unwrap_or_else(|| old.prefix.clone()),
                legacy_prefix: input
                    .legacy_prefix
                    .unwrap_or_else(|| old.legacy_prefix.clone()),
                retry_interval: input.retry_interval.unwrap_or(old.retry_interval),
                max_retries: input.max_retries.unwrap_or(old.max_retries),
                history_size: input.history_size.unwrap_or(old.history_size),
                history_days: input.history_days.unwrap_or(old.history_days),
            };
        }
        "climate" => next.climate = parse(body, section)?,
        _ => next.advanced = parse::<AdvancedSection>(body, section)?,
    }
    Ok(())
}

async fn put_section(
    State(s): State<RouteState>,
    Path(section): Path<String>,
    Json(body): Json<Value>,
) -> Response {
    if !known(&section) {
        return error(StatusCode::NOT_FOUND, "unknown section");
    }
    if let Some(r) = restarting(&s.ctx) {
        return r;
    }
    if s.ctx.mode != Mode::Run {
        return error(StatusCode::CONFLICT, "finish MQTT and database setup first");
    }
    let Some(db) = s.ctx.db.as_ref() else {
        return error(StatusCode::SERVICE_UNAVAILABLE, "database unreachable");
    };
    // Held from the snapshot to the replacement, so this save and the import copy never
    // overwrite each other with a stale copy.
    let mut held = s.ctx.sections.lock().await;
    if s.ctx.restarting.load(Ordering::SeqCst) {
        return error(StatusCode::SERVICE_UNAVAILABLE, "restarting");
    }
    let file = s.ctx.file.lock().await.clone();
    let mut next = held.clone();
    if let Err(r) = merge(&section, body, &mut next, &file) {
        return r;
    }
    if let Err(message) = assemble(&file, &next).validate_all() {
        return unprocessable(&message);
    }
    if let Err(error) = save_sections(db, &next).await {
        tracing::warn!(%error, "settings: cannot save to the database");
        return self::error(StatusCode::SERVICE_UNAVAILABLE, "database unreachable");
    }
    if let Err(error) = write_cache(&s.ctx.data_dir, &next) {
        // The database holds the change; the next save refreshes the cache.
        tracing::error!(error = %error.kind(), "settings: cannot write the settings cache");
    }
    *held = next;
    restart::schedule_restart(&s.ctx, RESTART_DELAY);
    Json(json!({ "restarting": true })).into_response()
}

fn failed(message: &str) -> Response {
    Json(TestResult {
        ok: false,
        message: message.to_owned(),
        detail: None,
    })
    .into_response()
}

async fn test_solcast(State(s): State<RouteState>, Json(input): Json<SolcastInput>) -> Response {
    let stored = s.ctx.sections.lock().await.solcast.api_key.clone();
    let key = secret::apply(stored, input.api_key).filter(|k| !k.trim().is_empty());
    let Some(key) = key else {
        return failed("Enter the API key");
    };
    Json(probe::solcast(&s.solcast_base, &key).await).into_response()
}

async fn test_frigate(State(s): State<RouteState>, Json(input): Json<FrigateInput>) -> Response {
    let old = s.ctx.sections.lock().await.frigate.clone();
    let cfg = match frigate_candidate(&old, input) {
        Ok(cfg) => cfg,
        Err(r) => return r,
    };
    if cfg.url.is_none() {
        return failed("Enter the Frigate address");
    }
    Json(probe::frigate(&cfg).await).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn address_ignores_the_path_and_default_ports() {
        assert_eq!(
            address_of("http://Frigate.example.com:80/a/b"),
            address_of("http://frigate.example.com")
        );
        assert_ne!(
            address_of("http://frigate.example.com:5000"),
            address_of("https://frigate.example.com:5000")
        );
    }

    #[test]
    fn project_id_never_fails_loudly() {
        assert_eq!(
            project_id(r#"{"project_id":"p-1"}"#).as_deref(),
            Some("p-1")
        );
        assert_eq!(project_id("{not json"), None);
        assert_eq!(project_id(r#"{"project_id":3}"#), None);
    }
}
