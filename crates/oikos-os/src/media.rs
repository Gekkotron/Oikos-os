//! Frigate media behind the hub: alert snapshots and clips live on Frigate's login port, which
//! the browser cannot reach without the session cookie. The hub rewrites those URLs to
//! `/api/v1/media?url=…` and fetches them itself with the `frigate_token` cookie.

use std::sync::Arc;

use axum::body::Body;
use futures_util::TryStreamExt;
use reqwest::header::{
    ACCEPT_RANGES, CONTENT_LENGTH, CONTENT_RANGE, CONTENT_TYPE, COOKIE, RANGE, SET_COOKIE,
};
use tokio::sync::Mutex;
use url::Url;

use crate::config::FrigateConfig;

/// The path the web fetches media through; the original URL goes in the `url` query key.
pub const MEDIA_PATH: &str = "/api/v1/media";
const COOKIE_NAME: &str = "frigate_token";

/// `scheme://host[:port]` of an absolute http(s) URL, host lowercased, default port dropped.
pub fn origin_of(url: &str) -> Option<String> {
    let parsed = Url::parse(url).ok()?;
    if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
        return None;
    }
    Some(parsed.origin().ascii_serialization())
}

/// The hub path that serves `url`, when `url` is on `media_origin`; `None` leaves it alone.
pub fn proxied(url: &str, media_origin: &str) -> Option<String> {
    if origin_of(url)? != media_origin {
        return None;
    }
    let query: String = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("url", url)
        .finish();
    Some(format!("{MEDIA_PATH}?{query}"))
}

/// Why a media fetch did not reach Frigate.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MediaError {
    /// The URL is not on the configured Frigate origin (or no Frigate is configured).
    #[error("media URL is not on the Frigate origin")]
    Forbidden,
    #[error("frigate: {0}")]
    Unavailable(String),
}

/// Frigate's reply, as far as the browser needs it.
pub struct Upstream {
    pub status: u16,
    pub content_type: Option<String>,
    pub content_length: Option<String>,
    pub content_range: Option<String>,
    pub accept_ranges: Option<String>,
    pub body: Body,
}

/// Fetches Frigate media with the session cookie, logging in on demand.
pub struct MediaProxy {
    client: reqwest::Client,
    base: String,
    origin: String,
    login: Option<(String, String)>,
    token: Mutex<Option<String>>,
}

impl MediaProxy {
    /// `None` without a `url`. The login is used only when both `user` and `password` are set.
    pub fn new(cfg: FrigateConfig) -> Option<Arc<MediaProxy>> {
        let raw = cfg.url.as_deref()?.trim().trim_end_matches('/').to_owned();
        let origin = origin_of(&raw)?;
        let login = match (cfg.user, cfg.password) {
            (Some(user), Some(password)) if !user.is_empty() && !password.is_empty() => {
                Some((user, password))
            }
            _ => None,
        };
        Some(Arc::new(MediaProxy {
            client: reqwest::Client::new(),
            base: raw,
            origin,
            login,
            token: Mutex::new(None),
        }))
    }

    /// `scheme://host[:port]` the proxy serves.
    pub fn origin(&self) -> &str {
        &self.origin
    }

    /// Fetches `url` (which must be on the origin) with the cookie when a login is configured,
    /// passing `range` through. A 401 triggers one new login and one retry; a second 401 is
    /// returned as is.
    pub async fn fetch(&self, url: &str, range: Option<&str>) -> Result<Upstream, MediaError> {
        if origin_of(url).as_deref() != Some(self.origin.as_str()) {
            return Err(MediaError::Forbidden);
        }
        let mut retried = false;
        loop {
            let token = match &self.login {
                Some(_) => Some(self.token().await?),
                None => None,
            };
            let mut request = self.client.get(url);
            if let Some(token) = &token {
                request = request.header(COOKIE, format!("{COOKIE_NAME}={token}"));
            }
            if let Some(range) = range {
                request = request.header(RANGE, range);
            }
            let response = request
                .send()
                .await
                .map_err(|err| MediaError::Unavailable(err.to_string()))?;
            if response.status() == reqwest::StatusCode::UNAUTHORIZED && token.is_some() && !retried
            {
                retried = true;
                *self.token.lock().await = None;
                continue;
            }
            return Ok(upstream(response));
        }
    }

    /// The cached token, or a fresh one from `POST /api/login`. Logins are serialised so
    /// concurrent fetches share one.
    async fn token(&self) -> Result<String, MediaError> {
        let mut slot = self.token.lock().await;
        if let Some(token) = slot.as_ref() {
            return Ok(token.clone());
        }
        let (user, password) = self.login.as_ref().ok_or(MediaError::Forbidden)?;
        let response = self
            .client
            .post(format!("{}/api/login", self.base))
            .json(&serde_json::json!({ "user": user, "password": password }))
            .send()
            .await
            .map_err(|err| MediaError::Unavailable(format!("login: {err}")))?;
        let status = response.status();
        if !status.is_success() {
            return Err(MediaError::Unavailable(format!(
                "login http {}",
                status.as_u16()
            )));
        }
        let token = response
            .headers()
            .get_all(SET_COOKIE)
            .iter()
            .filter_map(|value| value.to_str().ok())
            .find_map(token_in)
            .ok_or_else(|| MediaError::Unavailable("no token in Frigate's login answer".into()))?;
        *slot = Some(token.clone());
        Ok(token)
    }
}

/// The `frigate_token` value of one `Set-Cookie` header.
fn token_in(set_cookie: &str) -> Option<String> {
    set_cookie
        .split(';')
        .map(str::trim)
        .find_map(|part| {
            part.strip_prefix(COOKIE_NAME)
                .and_then(|rest| rest.strip_prefix('='))
        })
        .filter(|token| !token.is_empty())
        .map(str::to_owned)
}

fn upstream(response: reqwest::Response) -> Upstream {
    let header = |name| {
        response
            .headers()
            .get(name)
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned)
    };
    Upstream {
        status: response.status().as_u16(),
        content_type: header(CONTENT_TYPE),
        content_length: header(CONTENT_LENGTH),
        content_range: header(CONTENT_RANGE),
        accept_ranges: header(ACCEPT_RANGES),
        body: Body::from_stream(response.bytes_stream().map_err(std::io::Error::other)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::FrigateConfig;

    #[test]
    fn token_in_finds_the_cookie() {
        assert_eq!(
            token_in("frigate_token=jwt; Path=/; HttpOnly").as_deref(),
            Some("jwt")
        );
        assert_eq!(token_in("other=1; Path=/"), None);
        assert_eq!(token_in("frigate_token=; Path=/"), None);
    }

    #[test]
    fn origin_of_normalises() {
        assert_eq!(
            origin_of("http://H:8971/api/events/1/snapshot.jpg").as_deref(),
            Some("http://h:8971")
        );
        assert_eq!(origin_of("http://h:80/").as_deref(), Some("http://h"));
        assert_eq!(origin_of("https://h/").as_deref(), Some("https://h"));
        assert_eq!(origin_of("nope"), None);
        assert_eq!(origin_of("/api/events/1/snapshot.jpg"), None);
    }

    #[test]
    fn proxied_only_same_origin() {
        assert_eq!(
            proxied("http://h:8971/api/events/1/snapshot.jpg", "http://h:8971").as_deref(),
            Some("/api/v1/media?url=http%3A%2F%2Fh%3A8971%2Fapi%2Fevents%2F1%2Fsnapshot.jpg")
        );
        assert_eq!(
            proxied("http://h:5000/api/events/1/snapshot.jpg", "http://h:8971"),
            None
        );
        assert_eq!(proxied("nope", "http://h:8971"), None);
    }

    #[test]
    fn proxy_needs_a_url() {
        assert!(MediaProxy::new(FrigateConfig::default()).is_none());
        let proxy = MediaProxy::new(FrigateConfig {
            url: Some("http://H:8971/".into()),
            user: None,
            password: None,
        })
        .expect("a proxy");
        assert_eq!(proxy.origin(), "http://h:8971");
    }
}
