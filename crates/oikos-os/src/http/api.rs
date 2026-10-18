//! The JSON API: `/health` and the `/api/v1` routes, with hub errors mapped to status codes.

use std::sync::Arc;

use axum::Json;
use axum::Router;
use axum::extract::rejection::JsonRejection;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, patch, post, put};
use oikos_api::{
    ApiError, AreaView, CommandResponse, ConnectionStatus, EntityView, IgnoredView, OverridePatch,
    Snapshot,
};
use oikos_core::model::{AreasDocument, Command};
use serde::Serialize;

use crate::hub::{Hub, HubError};

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
            HubError::UnknownEntity(_) => StatusCode::NOT_FOUND,
            HubError::Rejected(_) | HubError::Command(_) => StatusCode::UNPROCESSABLE_ENTITY,
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
