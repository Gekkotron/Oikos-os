//! The event stream: a snapshot first, then every hub event as a JSON text frame.

use std::sync::Arc;

use axum::extract::State;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::response::Response;
use futures_util::stream::SplitSink;
use futures_util::{SinkExt, StreamExt};
use oikos_api::WsEvent;
use tokio::sync::broadcast::error::RecvError;

use crate::hub::Hub;

pub(super) async fn upgrade(State(hub): State<Arc<Hub>>, ws: WebSocketUpgrade) -> Response {
    ws.on_upgrade(move |socket| run(socket, hub))
}

/// Subscribes before taking the snapshot so no event falls between the two; a client that
/// lags behind the broadcast gets a fresh snapshot instead of being dropped. Incoming frames
/// are ignored (axum answers pings by itself); a close or a transport error ends the stream.
async fn run(socket: WebSocket, hub: Arc<Hub>) {
    let mut events = hub.subscribe();
    let (mut sink, mut stream) = socket.split();
    if send(&mut sink, &WsEvent::Snapshot(hub.snapshot()))
        .await
        .is_err()
    {
        return;
    }
    loop {
        tokio::select! {
            event = events.recv() => {
                let event = match event {
                    Ok(event) => event,
                    Err(RecvError::Lagged(missed)) => {
                        tracing::debug!(missed, "websocket client lagged; resending the snapshot");
                        WsEvent::Snapshot(hub.snapshot())
                    }
                    Err(RecvError::Closed) => return,
                };
                if send(&mut sink, &event).await.is_err() {
                    return;
                }
            }
            incoming = stream.next() => match incoming {
                None | Some(Err(_)) | Some(Ok(Message::Close(_))) => return,
                Some(Ok(_)) => {}
            }
        }
    }
}

async fn send(sink: &mut SplitSink<WebSocket, Message>, event: &WsEvent) -> Result<(), ()> {
    let json = match serde_json::to_string(event) {
        Ok(json) => json,
        Err(err) => {
            tracing::error!(error = %err, "could not encode a websocket event");
            return Ok(());
        }
    };
    sink.send(Message::Text(json.into())).await.map_err(|err| {
        tracing::debug!(error = %err, "websocket client gone");
    })
}
