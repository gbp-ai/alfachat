use axum::{
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    extract::State,
    response::IntoResponse,
    routing::get,
    Router,
};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use tower_http::services::ServeDir;

const MAX_HISTORY: usize = 100;

#[derive(Clone, Serialize, Deserialize)]
struct ChatMessage {
    username: String,
    content: String,
    timestamp: String,
}

struct AppState {
    tx: broadcast::Sender<String>,
    history: RwLock<VecDeque<ChatMessage>>,
}

#[tokio::main]
async fn main() {
    let (tx, _) = broadcast::channel::<String>(100);
    let state = Arc::new(AppState {
        tx,
        history: RwLock::new(VecDeque::new()),
    });

    let app = Router::new()
        .route("/ws", get(ws_handler))
        .fallback_service(ServeDir::new("static"))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3333").await.unwrap();
    println!("🦀 AlfaChat running on http://0.0.0.0:3333");
    axum::serve(listener, app).await.unwrap();
}

async fn ws_handler(ws: WebSocketUpgrade, State(state): State<Arc<AppState>>) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

async fn handle_socket(mut socket: WebSocket, state: Arc<AppState>) {
    // Send history on connect
    {
        let history = state.history.read().await;
        for msg in history.iter() {
            let json = serde_json::to_string(msg).unwrap();
            let _ = socket.send(Message::Text(json.into())).await;
        }
    }

    let mut rx = state.tx.subscribe();
    let tx = state.tx.clone();
    let state_clone = state.clone();

    loop {
        tokio::select! {
            // Receive from client
            Some(Ok(msg)) = socket.recv() => {
                if let Message::Text(text) = msg {
                    if let Ok(chat_msg) = serde_json::from_str::<ChatMessage>(&text) {
                        let msg_with_time = ChatMessage {
                            username: chat_msg.username,
                            content: chat_msg.content,
                            timestamp: chrono::Utc::now().format("%H:%M:%S").to_string(),
                        };
                        let json = serde_json::to_string(&msg_with_time).unwrap();
                        
                        // Add to history
                        {
                            let mut h: tokio::sync::RwLockWriteGuard<'_, VecDeque<ChatMessage>> = state_clone.history.write().await;
                            h.push_back(msg_with_time);
                            if h.len() > MAX_HISTORY {
                                h.pop_front();
                            }
                        }
                        
                        // Broadcast
                        let _ = tx.send(json);
                    }
                }
            }
            // Receive broadcasts
            Ok(msg) = rx.recv() => {
                let _ = socket.send(Message::Text(msg.into())).await;
            }
            else => break,
        }
    }
}
