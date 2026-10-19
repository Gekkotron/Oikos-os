//! The hub's REST API. Every failure comes back as an [`ApiError`] whose text is fit for a toast.

use gloo_net::http::{Request, Response};
use oikos_api::{ApiError, CommandResponse, EntityView, OverridePatch};
use oikos_core::model::{AreasDocument, Command};
use serde::de::DeserializeOwned;
use serde_json::Value;

/// What a 503 on a command means: the hub cannot reach the broker.
pub const BROKER_OFFLINE: &str = "Broker offline — command refused";

fn network_error(e: impl std::fmt::Display) -> ApiError {
    ApiError {
        error: format!("network error: {e}"),
    }
}

/// The error for a non-2xx reply: the hub's own `ApiError` body, or `HTTP <status>`.
fn decode_error(status: u16, body: &str) -> ApiError {
    serde_json::from_str::<ApiError>(body).unwrap_or_else(|_| ApiError {
        error: format!("HTTP {status}"),
    })
}

/// Sends `request` and turns a transport failure or a status of 400 and up into an `ApiError`.
async fn check(
    request: Result<Request, gloo_net::Error>,
    on_503: Option<&str>,
) -> Result<Response, ApiError> {
    let response = request
        .map_err(network_error)?
        .send()
        .await
        .map_err(network_error)?;
    let status = response.status();
    if status == 503
        && let Some(text) = on_503
    {
        return Err(ApiError { error: text.into() });
    }
    if status >= 400 {
        let body = response.text().await.unwrap_or_default();
        return Err(decode_error(status, &body));
    }
    Ok(response)
}

async fn read_json<T: DeserializeOwned>(response: Response) -> Result<T, ApiError> {
    response.json::<T>().await.map_err(network_error)
}

pub async fn send_command(id: &str, cmd: Command) -> Result<CommandResponse, ApiError> {
    let url = format!("/api/v1/entities/{id}/command");
    let response = check(Request::post(&url).json(&cmd), Some(BROKER_OFFLINE)).await?;
    read_json(response).await
}

pub async fn set_override(id: &str, patch: OverridePatch) -> Result<EntityView, ApiError> {
    let url = format!("/api/v1/entities/{id}/override");
    let response = check(Request::patch(&url).json(&Value::Object(patch)), None).await?;
    read_json(response).await
}

pub async fn clear_override(id: &str) -> Result<(), ApiError> {
    let url = format!("/api/v1/entities/{id}/override");
    check(Request::delete(&url).build(), None).await?;
    Ok(())
}

pub async fn set_areas(doc: AreasDocument) -> Result<Vec<oikos_api::AreaView>, ApiError> {
    let response = check(Request::put("/api/v1/areas").json(&doc), None).await?;
    read_json(response).await
}

#[cfg(test)]
mod tests {
    use super::decode_error;

    #[test]
    fn server_message_is_surfaced() {
        let e = decode_error(422, r#"{"error":"light does not support color"}"#);
        assert_eq!(e.error, "light does not support color");
    }

    #[test]
    fn unparsable_body_falls_back_to_status() {
        assert_eq!(
            decode_error(502, "<html>bad gateway</html>").error,
            "HTTP 502"
        );
        assert_eq!(decode_error(500, "").error, "HTTP 500");
    }
}
