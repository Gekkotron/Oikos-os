//! The WebSocket client: feeds [`House`] from the hub's `/ws` and reconnects with back-off.

use futures_util::StreamExt;
use gloo_net::websocket::{Message, futures::WebSocket};
use gloo_timers::future::TimeoutFuture;
use leptos::prelude::Set;
use oikos_api::{ConnectionStatus, WsEvent};
use wasm_bindgen_futures::spawn_local;

use crate::house::House;

const FIRST_BACKOFF_MS: u32 = 1_000;
const MAX_BACKOFF_MS: u32 = 10_000;

/// Keeps a WebSocket to `path` (e.g. `/ws`, on the page's own host) open, applying every event
/// to `house`.
pub fn connect(path: &str, house: House) {
    let path = path.to_owned();
    spawn_local(async move {
        let mut backoff = FIRST_BACKOFF_MS;
        loop {
            house.connection.set(ConnectionStatus::Connecting);
            let reason = match absolute_url(&path)
                .and_then(|u| WebSocket::open(&u).map_err(|e| e.to_string()))
            {
                Ok(mut socket) => {
                    let mut closed = String::from("connection closed");
                    while let Some(frame) = socket.next().await {
                        match frame {
                            Ok(Message::Text(text)) => match serde_json::from_str::<WsEvent>(&text)
                            {
                                Ok(event) => {
                                    let connected = matches!(event, WsEvent::Snapshot(_));
                                    if connected {
                                        backoff = FIRST_BACKOFF_MS;
                                    }
                                    house.apply(event);
                                    // Weather, home settings and the solar forecast are not in
                                    // the snapshot: refetch them on every (re)connect.
                                    if connected {
                                        house.refresh_weather();
                                        house.refresh_solar();
                                        house.refresh_home_settings();
                                    }
                                }
                                Err(e) => {
                                    web_sys::console::warn_1(&format!("bad ws event: {e}").into())
                                }
                            },
                            Ok(Message::Bytes(_)) => {}
                            Err(e) => {
                                closed = e.to_string();
                                break;
                            }
                        }
                    }
                    closed
                }
                Err(e) => e,
            };
            house.connection.set(ConnectionStatus::Offline { reason });
            TimeoutFuture::new(backoff).await;
            backoff = (backoff * 2).min(MAX_BACKOFF_MS);
        }
    });
}

/// `ws://host/path` or `wss://host/path`, following the page's protocol.
fn absolute_url(path: &str) -> Result<String, String> {
    let location = web_sys::window().ok_or("no window")?.location();
    let protocol = location.protocol().map_err(|_| "no location.protocol")?;
    let host = location.host().map_err(|_| "no location.host")?;
    let scheme = if protocol == "https:" { "wss" } else { "ws" };
    Ok(format!("{scheme}://{host}{path}"))
}
