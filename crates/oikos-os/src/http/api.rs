//! The JSON API: `/health` and the `/api/v1` routes, with hub errors mapped to status codes.

use std::sync::Arc;

use axum::Json;
use axum::Router;
use axum::body::Body as HttpBody;
use axum::extract::rejection::JsonRejection;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::http::{HeaderMap, HeaderValue, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, patch, post, put};
use oikos_api::{
    AlertView, ApiError, AreaView, CommandResponse, ConnectionStatus, EntityView, FlowView,
    IgnoredView, OverridePatch, RunView, SchedulePreview, Snapshot,
};
use oikos_core::flow::{Flow, Schedule};
use oikos_core::model::{AreasDocument, Command};
use serde::Serialize;

use crate::hub::{Hub, HubError};
use crate::media::MediaError;

type HubState = State<Arc<Hub>>;

/// A JSON body, or the rejection that becomes a JSON error reply.
type Body<T> = Result<Json<T>, JsonRejection>;

pub(super) fn routes() -> Router<Arc<Hub>> {
    Router::new()
        .route("/snapshot", get(snapshot))
        .route("/entities/{id}/command", post(command))
        .route(
            "/entities/{id}/override",
            patch(set_override).delete(clear_override),
        )
        .route("/areas", put(set_areas))
        .route("/areas/{id}", patch(rename_area))
        .route("/areas/{id}/groups/{kind}", patch(set_area_group))
        .route("/ignored", get(ignored))
        .route("/alerts", get(alerts))
        .route("/alerts/{id}/actions/{action}", post(alert_action))
        .route("/media", get(media))
        .route("/flows", get(flows))
        .route("/schedule", get(schedule_preview))
        .route(
            "/flows/{id}",
            get(get_flow).put(put_flow).delete(delete_flow),
        )
        .route("/flows/{id}/enabled", post(set_flow_enabled))
        .route("/flows/{id}/run", post(run_flow))
        .fallback(not_found)
}

/// An error reply: the status and an [`ApiError`] body.
pub(super) struct HttpError {
    status: StatusCode,
    error: String,
}

impl IntoResponse for HttpError {
    fn into_response(self) -> Response {
        (self.status, Json(ApiError { error: self.error })).into_response()
    }
}

impl From<HubError> for HttpError {
    fn from(err: HubError) -> Self {
        let status = match &err {
            HubError::UnknownEntity(_)
            | HubError::UnknownAlert(_)
            | HubError::UnknownAction(_)
            | HubError::UnknownFlow(_)
            | HubError::UnknownArea(_) => StatusCode::NOT_FOUND,
            HubError::Rejected(_) | HubError::Command(_) | HubError::InvalidFlow(_) => {
                StatusCode::UNPROCESSABLE_ENTITY
            }
            HubError::Patch(_) => StatusCode::BAD_REQUEST,
            HubError::Publish(_) => StatusCode::SERVICE_UNAVAILABLE,
        };
        Self {
            status,
            error: err.to_string(),
        }
    }
}

impl From<JsonRejection> for HttpError {
    fn from(rejection: JsonRejection) -> Self {
        Self {
            status: rejection.status(),
            error: rejection.body_text(),
        }
    }
}

/// The `/health` body.
#[derive(Serialize)]
pub(super) struct Health {
    status: &'static str,
    connection: ConnectionStatus,
    entities: usize,
}

pub(super) async fn health(State(hub): HubState) -> Json<Health> {
    let connection = hub.connection().borrow().clone();
    let (entities, _) = hub.counts();
    Json(Health {
        status: "ok",
        connection,
        entities,
    })
}

pub(super) async fn not_found() -> HttpError {
    HttpError {
        status: StatusCode::NOT_FOUND,
        error: "no such route".to_owned(),
    }
}

async fn snapshot(State(hub): HubState) -> Json<Snapshot> {
    Json(hub.snapshot())
}

async fn command(
    State(hub): HubState,
    Path(id): Path<String>,
    body: Body<Command>,
) -> Result<Json<CommandResponse>, HttpError> {
    let Json(cmd) = body?;
    Ok(Json(hub.send_command(&id, cmd).await?))
}

async fn set_override(
    State(hub): HubState,
    Path(id): Path<String>,
    body: Body<OverridePatch>,
) -> Result<Json<EntityView>, HttpError> {
    let Json(patch) = body?;
    Ok(Json(hub.set_override(&id, &patch).await?))
}

async fn clear_override(
    State(hub): HubState,
    Path(id): Path<String>,
) -> Result<StatusCode, HttpError> {
    hub.clear_override(&id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn set_areas(
    State(hub): HubState,
    body: Body<AreasDocument>,
) -> Result<Json<Vec<AreaView>>, HttpError> {
    let Json(doc) = body?;
    Ok(Json(hub.set_areas(doc).await?))
}

/// The `PATCH /areas/{id}` body.
#[derive(serde::Deserialize)]
struct RenameArea {
    name: String,
}

async fn rename_area(
    State(hub): HubState,
    Path(id): Path<String>,
    body: Body<RenameArea>,
) -> Result<Json<Vec<AreaView>>, HttpError> {
    let Json(RenameArea { name }) = body?;
    Ok(Json(hub.rename_area(&id, &name).await?))
}

/// The `PATCH /areas/{id}/groups/{kind}` body. `name` absent leaves the name, `null` clears it.
#[derive(serde::Deserialize)]
struct SetAreaGroup {
    #[serde(default, deserialize_with = "present")]
    name: Option<Option<String>>,
    favorite: Option<bool>,
}

/// Any present value, `null` included, becomes `Some`; an absent field stays `None` through
/// `#[serde(default)]`.
fn present<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<Option<String>>, D::Error> {
    serde::Deserialize::deserialize(d).map(Some)
}

async fn set_area_group(
    State(hub): HubState,
    Path((id, kind)): Path<(String, String)>,
    body: Body<SetAreaGroup>,
) -> Result<Json<Vec<AreaView>>, HttpError> {
    let Json(SetAreaGroup { name, favorite }) = body?;
    Ok(Json(hub.set_area_group(&id, &kind, name, favorite).await?))
}

async fn ignored(State(hub): HubState) -> Json<Vec<IgnoredView>> {
    Json(hub.ignored())
}

async fn alerts(State(hub): HubState) -> Json<Vec<AlertView>> {
    Json(hub.alerts())
}

#[derive(serde::Deserialize)]
pub(super) struct MediaQuery {
    url: Option<String>,
}

/// Serves a Frigate snapshot or clip through the hub: the upstream status, the headers a
/// `<img>` or `<video>` needs and the streamed body. `400` without a valid absolute `url`,
/// `403` off the Frigate origin or without a `[frigate]` section, `502` when Frigate is out
/// of reach or refuses the login.
async fn media(
    State(hub): HubState,
    Query(query): Query<MediaQuery>,
    headers: HeaderMap,
) -> Result<Response, HttpError> {
    let url = query
        .url
        .filter(|u| crate::media::origin_of(u).is_some())
        .ok_or(HttpError {
            status: StatusCode::BAD_REQUEST,
            error: "missing or invalid `url`".to_owned(),
        })?;
    let proxy = hub.media().ok_or(HttpError {
        status: StatusCode::FORBIDDEN,
        error: "no Frigate configured".to_owned(),
    })?;
    let range = headers.get(header::RANGE).and_then(|v| v.to_str().ok());
    let upstream = proxy.fetch(&url, range).await.map_err(|err| HttpError {
        status: match err {
            MediaError::Forbidden => StatusCode::FORBIDDEN,
            MediaError::Unavailable(_) => StatusCode::BAD_GATEWAY,
        },
        error: err.to_string(),
    })?;
    let mut response = Response::new(HttpBody::empty());
    *response.status_mut() =
        StatusCode::from_u16(upstream.status).unwrap_or(StatusCode::BAD_GATEWAY);
    let pairs = [
        (header::CONTENT_TYPE, upstream.content_type),
        (header::CONTENT_LENGTH, upstream.content_length),
        (header::CONTENT_RANGE, upstream.content_range),
        (header::ACCEPT_RANGES, upstream.accept_ranges),
    ];
    for (name, value) in pairs {
        if let Some(value) = value.as_deref().and_then(|v| HeaderValue::from_str(v).ok()) {
            response.headers_mut().insert(name, value);
        }
    }
    *response.body_mut() = upstream.body;
    Ok(response)
}

/// Publishes the reply for a tapped alert button; `202` once it is handed to the broker task.
async fn alert_action(
    State(hub): HubState,
    Path((id, action)): Path<(String, String)>,
) -> Result<StatusCode, HttpError> {
    hub.reply(&id, &action)?;
    Ok(StatusCode::ACCEPTED)
}

/// Every flow with its engine state.
async fn flows(State(hub): HubState) -> Json<Vec<FlowView>> {
    Json(hub.flows())
}

/// One flow; `404` for an unknown id.
async fn get_flow(
    State(hub): HubState,
    Path(id): Path<String>,
) -> Result<Json<FlowView>, HttpError> {
    hub.flow(&id)
        .map(Json)
        .ok_or_else(|| HubError::UnknownFlow(id).into())
}

/// Creates or replaces a flow. The path id wins: a body without an `id` takes it, a body with
/// another one is `400 id mismatch`. `422` for a flow that does not validate, `503` while the
/// broker link is down.
async fn put_flow(
    State(hub): HubState,
    Path(id): Path<String>,
    body: Body<serde_json::Value>,
) -> Result<Json<FlowView>, HttpError> {
    let Json(mut value) = body?;
    if let Some(object) = value.as_object_mut() {
        match object.get("id").and_then(|v| v.as_str()) {
            None | Some("") => {
                object.insert("id".to_owned(), serde_json::Value::String(id.clone()));
            }
            Some(body_id) if body_id != id => {
                return Err(HttpError {
                    status: StatusCode::BAD_REQUEST,
                    error: "id mismatch".to_owned(),
                });
            }
            Some(_) => {}
        }
    }
    let flow = Flow::decode(value.to_string().as_bytes()).map_err(|error| HttpError {
        status: StatusCode::UNPROCESSABLE_ENTITY,
        error,
    })?;
    Ok(Json(hub.set_flow(flow).await?))
}

/// Removes a flow (204); `404` for an unknown id.
async fn delete_flow(
    State(hub): HubState,
    Path(id): Path<String>,
) -> Result<StatusCode, HttpError> {
    hub.delete_flow(&id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// The `POST /flows/{id}/enabled` body.
#[derive(serde::Deserialize)]
struct EnabledBody {
    enabled: bool,
}

/// Enables or disables a flow and returns it as it now stands.
async fn set_flow_enabled(
    State(hub): HubState,
    Path(id): Path<String>,
    body: Body<EnabledBody>,
) -> Result<Json<FlowView>, HttpError> {
    let Json(EnabledBody { enabled }) = body?;
    Ok(Json(hub.set_flow_enabled(&id, enabled).await?))
}

/// Starts a run by hand; `202` with the run, `422` when the flow is disabled, invalid or busy.
async fn run_flow(
    State(hub): HubState,
    Path(id): Path<String>,
) -> Result<(StatusCode, Json<RunView>), HttpError> {
    Ok((StatusCode::ACCEPTED, Json(hub.run_flow(&id)?)))
}

#[derive(serde::Deserialize)]
struct ScheduleQuery {
    cron: String,
    count: Option<usize>,
}

/// The next ticks of a cron expression in the house time zone: `count` of them (default 3,
/// at most 10). `422` for an expression that does not parse.
async fn schedule_preview(
    State(hub): HubState,
    Query(query): Query<ScheduleQuery>,
) -> Result<Json<SchedulePreview>, HttpError> {
    let schedule = Schedule::parse(&query.cron).map_err(|err| HttpError {
        status: StatusCode::UNPROCESSABLE_ENTITY,
        error: err.to_string(),
    })?;
    let zone = hub.zone();
    let mut at = chrono::Utc::now().with_timezone(&zone);
    let mut next = Vec::new();
    for _ in 0..query.count.unwrap_or(3).clamp(1, 10) {
        let Some(tick) = schedule.next_after(&at) else {
            break;
        };
        next.push(tick.to_rfc3339());
        at = tick;
    }
    Ok(Json(SchedulePreview {
        timezone: zone.name().to_owned(),
        next,
    }))
}
