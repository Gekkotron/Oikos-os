//! The hub's REST API. Every failure comes back as an [`ApiError`] whose text is fit for a toast.

use gloo_net::http::{Request, Response};
use oikos_api::{
    ApiError, CommandResponse, EntityView, FlowView, OverridePatch, RunView, SchedulePreview,
};
use oikos_core::flow::Flow;
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

/// `id` as one URL path segment: everything but the unreserved characters (`A-Za-z0-9-._~`)
/// becomes `%XX` over its UTF-8 bytes, so `/`, `#`, `?`, `%` and spaces cannot change the route.
///
/// Hand-rolled rather than `js_sys::encode_uri_component` (which also leaves `!'()*` alone and
/// panics off wasm, so it cannot be unit-tested natively) or the `percent-encoding` crate (an
/// extra dependency for ten lines).
pub fn encode_id(id: &str) -> String {
    let mut out = String::with_capacity(id.len());
    for b in id.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(char::from(b));
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

pub async fn send_command(id: &str, cmd: Command) -> Result<CommandResponse, ApiError> {
    let url = format!("/api/v1/entities/{}/command", encode_id(id));
    let response = check(Request::post(&url).json(&cmd), Some(BROKER_OFFLINE)).await?;
    read_json(response).await
}

/// Presses alert `id`'s button `action`; the hub forwards it to the producer.
pub async fn send_alert_action(id: &str, action: &str) -> Result<(), ApiError> {
    let url = format!(
        "/api/v1/alerts/{}/actions/{}",
        encode_id(id),
        encode_id(action)
    );
    check(Request::post(&url).build(), Some(BROKER_OFFLINE)).await?;
    Ok(())
}

pub async fn set_override(id: &str, patch: OverridePatch) -> Result<EntityView, ApiError> {
    let url = format!("/api/v1/entities/{}/override", encode_id(id));
    let response = check(Request::patch(&url).json(&Value::Object(patch)), None).await?;
    read_json(response).await
}

pub async fn clear_override(id: &str) -> Result<(), ApiError> {
    let url = format!("/api/v1/entities/{}/override", encode_id(id));
    check(Request::delete(&url).build(), None).await?;
    Ok(())
}

/// Enables or disables flow `id`; the reply is the flow as the hub now has it.
pub async fn set_flow_enabled(id: &str, enabled: bool) -> Result<FlowView, ApiError> {
    let url = format!("/api/v1/flows/{}/enabled", encode_id(id));
    let body = serde_json::json!({ "enabled": enabled });
    let response = check(Request::post(&url).json(&body), None).await?;
    read_json(response).await
}

/// Replaces flow `id`'s definition on the hub. The id in `flow` must match the path.
pub async fn put_flow(flow: &Flow) -> Result<FlowView, ApiError> {
    let url = format!("/api/v1/flows/{}", encode_id(&flow.id));
    let response = check(Request::put(&url).json(flow), None).await?;
    read_json(response).await
}

/// The next three ticks of `cron` in the house time zone; a `422` (the hub's message names what
/// is wrong with the expression) comes back as an `ApiError`.
pub async fn schedule_preview(cron: &str) -> Result<SchedulePreview, ApiError> {
    let url = format!("/api/v1/schedule?cron={}&count=3", encode_id(cron));
    let response = check(Request::get(&url).build(), None).await?;
    read_json(response).await
}

/// Starts flow `id` by hand; the hub answers 202 with the new run.
pub async fn run_flow(id: &str) -> Result<RunView, ApiError> {
    let url = format!("/api/v1/flows/{}/run", encode_id(id));
    let response = check(Request::post(&url).build(), None).await?;
    read_json(response).await
}

/// Renames area `id`; the hub answers with the area list it now has.
pub async fn rename_area(id: &str, name: &str) -> Result<Vec<oikos_api::AreaView>, ApiError> {
    let url = format!("/api/v1/areas/{}", encode_id(id));
    let body = serde_json::json!({ "name": name });
    let response = check(Request::patch(&url).json(&body), None).await?;
    read_json(response).await
}

/// The body of a room-group edit: absent fields are left out, `Some(None)` clears the name.
fn group_body(name: Option<Option<&str>>, favorite: Option<bool>) -> Value {
    let mut body = serde_json::Map::new();
    if let Some(name) = name {
        body.insert("name".to_owned(), serde_json::json!(name));
    }
    if let Some(favorite) = favorite {
        body.insert("favorite".to_owned(), Value::Bool(favorite));
    }
    Value::Object(body)
}

/// Edits the `kind` group (`cover` or `light`) of area `id`: its custom name and whether it is a
/// favourite. The hub answers with the area list it now has.
pub async fn set_area_group(
    id: &str,
    kind: &str,
    name: Option<Option<&str>>,
    favorite: Option<bool>,
) -> Result<Vec<oikos_api::AreaView>, ApiError> {
    let url = format!("/api/v1/areas/{}/groups/{}", encode_id(id), encode_id(kind));
    let body = group_body(name, favorite);
    let response = check(Request::patch(&url).json(&body), Some(BROKER_OFFLINE)).await?;
    read_json(response).await
}

// Reserved: a full area editor is not built yet, so nothing calls this.
pub async fn set_areas(doc: AreasDocument) -> Result<Vec<oikos_api::AreaView>, ApiError> {
    let response = check(Request::put("/api/v1/areas").json(&doc), None).await?;
    read_json(response).await
}

#[cfg(test)]
mod tests {
    use super::{decode_error, encode_id, group_body};
    use serde_json::json;

    #[test]
    fn ordinary_ids_pass_through() {
        assert_eq!(
            encode_id("light.kitchen_lamp-1~x"),
            "light.kitchen_lamp-1~x"
        );
    }

    #[test]
    fn reserved_characters_are_escaped() {
        assert_eq!(encode_id("a/b#c?d e%f"), "a%2Fb%23c%3Fd%20e%25f");
        assert_eq!(encode_id("é"), "%C3%A9");
    }

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

    #[test]
    fn group_body_distinguishes_null_from_absent() {
        assert_eq!(group_body(Some(None), None), json!({"name": null}));
        assert_eq!(group_body(None, None), json!({}));
        assert_eq!(group_body(None, Some(true)), json!({"favorite": true}));
        assert_eq!(
            group_body(Some(Some("Blinds")), Some(false)),
            json!({"name": "Blinds", "favorite": false})
        );
    }
}
