use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::IntoResponse;
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::{info, warn};

use crate::routes::AppState;

/// GET /ws – upgrade to WebSocket for live alert streaming.
pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let rx = state.alert_broadcast.subscribe();
    ws.on_upgrade(move |socket| handle_socket(socket, rx))
}

async fn handle_socket(mut socket: WebSocket, mut rx: broadcast::Receiver<String>) {
    info!("WebSocket client connected");

    loop {
        tokio::select! {
            // Forward alerts from broadcast channel to the WS client
            result = rx.recv() => {
                match result {
                    Ok(json) => {
                        if socket.send(Message::Text(json.into())).await.is_err() {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        warn!(missed = n, "WebSocket client lagged");
                    }
                    Err(_) => break,
                }
            }
            // If the client sends a close frame, stop
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Close(_))) | None => break,
                    _ => {} // ignore pings / pongs / text from client
                }
            }
        }
    }

    info!("WebSocket client disconnected");
}
