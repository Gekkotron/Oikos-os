//! The WebSocket client: feeds [`House`] from the hub's `/ws` and reconnects with back-off.

use futures_util::StreamExt;
use gloo_net::websocket::{Message, futures::WebSocket};
use gloo_timers::future::TimeoutFuture;
use leptos::prelude::Set;
use oikos_api::{ConnectionStatus, WsEvent};
use wasm_bindgen_futures::spawn_local;

use crate::house::House;
use crate::i18n::current_lang;
use crate::i18n::errors::{ErrorText, error_text};

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
                    let mut closed =
                        error_text(current_lang(), ErrorText::ConnectionClosed).to_owned();
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
                                    // Weather, home settings, the solar forecast and the ambiances are
                                    // not in the snapshot: refetch them on every (re)connect.
                                    if connected {
                                        house.refresh_weather();
                                        house.refresh_solar();
                                        house.refresh_ambiances();
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
    let no_window = || error_text(current_lang(), ErrorText::NoWindow).to_owned();
    let location = web_sys::window().ok_or_else(no_window)?.location();
    let protocol = location.protocol().map_err(|_| no_window())?;
    let host = location.host().map_err(|_| no_window())?;
    let scheme = if protocol == "https:" { "wss" } else { "ws" };
    Ok(format!("{scheme}://{host}{path}"))
}
