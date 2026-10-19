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
    AlertView, ApiError, AreaView, CommandResponse, ConnectionStatus, EntityView, IgnoredView,
    OverridePatch, Snapshot,
};
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
        .route("/ignored", get(ignored))
        .route("/alerts", get(alerts))
        .route("/alerts/{id}/actions/{action}", post(alert_action))
        .route("/media", get(media))
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
            | HubError::UnknownFlow(_) => StatusCode::NOT_FOUND,
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
