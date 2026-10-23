//! The home header's REST routes: the weather and the home settings.

use gloo_net::http::Request;
use oikos_api::{ApiError, HomeSettings, WeatherView};

use super::core::{check, read_json};

/// The current weather; `Ok(None)` while the hub has none (weather off, or not fetched yet: 503).
pub async fn get_weather() -> Result<Option<WeatherView>, ApiError> {
    let request = Request::get("/api/v1/weather").build();
    match check(request, Some("Weather unavailable")).await {
        Ok(response) => read_json(response).await.map(Some),
        // `check` turned the 503 into this exact text; any other failure keeps its own message.
        Err(e) if e.error == "Weather unavailable" => Ok(None),
        Err(e) => Err(e),
    }
}

/// The home settings document.
pub async fn get_home_settings() -> Result<HomeSettings, ApiError> {
    let response = check(Request::get("/api/v1/home/settings").build(), None).await?;
    read_json(response).await
}

/// Replaces the whole settings document; the reply is it as stored. A 422 comes back as an
/// `ApiError` whose text names the field.
pub async fn put_home_settings(settings: &HomeSettings) -> Result<HomeSettings, ApiError> {
    let response = check(Request::put("/api/v1/home/settings").json(settings), None).await?;
    read_json(response).await
}
