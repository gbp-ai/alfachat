use axum::{
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    extract::State,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use tower_http::services::ServeDir;
use uuid::Uuid;

const MAX_HISTORY: usize = 100;
const WEBHOOK_URL: &str = "http://localhost:7007/hooks/alfachat";

#[derive(Clone, Serialize, Deserialize, Debug)]
struct ChatMessage {
    username: String,
    content: String,
    timestamp: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    msg_type: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
struct UserList {
    users: Vec<String>,
    msg_type: String,
}

#[derive(Clone, Serialize, Deserialize)]
struct JoinRequest {
    username: String,
    msg_type: String,
}

#[derive(Clone, Serialize, Deserialize)]
struct JoinResponse {
    success: bool,
    error: Option<String>,
    msg_type: String,
}

struct AppState {
    tx: broadcast::Sender<String>,
    history: RwLock<VecDeque<ChatMessage>>,
    users: RwLock<HashMap<String, String>>, // session_id -> username
    taken_usernames: RwLock<HashSet<String>>,
}

#[tokio::main]
async fn main() {
    let (tx, _) = broadcast::channel::<String>(100);
    let state = Arc::new(AppState {
        tx,
        history: RwLock::new(VecDeque::new()),
        users: RwLock::new(HashMap::new()),
        taken_usernames: RwLock::new(HashSet::new()),
    });

    let app = Router::new()
        .route("/ws", get(ws_handler))
        .route("/api/messages", get(get_messages))
        .route("/api/messages", post(post_message))
        .route("/api/users", get(get_users))
        .fallback_service(ServeDir::new("static"))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3333").await.unwrap();
    println!("🦀 AlfaChat running on http://0.0.0.0:3333");
    axum::serve(listener, app).await.unwrap();
}

// GET /api/messages
async fn get_messages(State(state): State<Arc<AppState>>) -> Json<Vec<ChatMessage>> {
    let history = state.history.read().await;
    Json(history.iter().cloned().collect())
}

// GET /api/users
async fn get_users(State(state): State<Arc<AppState>>) -> Json<Vec<String>> {
    let users = state.users.read().await;
    Json(users.values().cloned().collect())
}

// POST /api/messages
async fn post_message(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<ChatMessage>,
) -> Json<ChatMessage> {
    let username = payload.username.to_lowercase();
    let msg = ChatMessage {
        username: username.clone(),
        content: payload.content.clone(),
        timestamp: chrono::Utc::now().format("%H:%M:%S").to_string(),
        msg_type: Some("chat".to_string()),
    };
    
    let json = serde_json::to_string(&msg).unwrap();
    
    {
        let mut h = state.history.write().await;
        h.push_back(msg.clone());
        if h.len() > MAX_HISTORY {
            h.pop_front();
        }
    }
    
    let _ = state.tx.send(json);
    
    // Webhook for @georges mentions
    if payload.content.to_lowercase().contains("@georges") {
        tokio::spawn(send_webhook(msg.clone()));
    }
    
    Json(msg)
}

async fn send_webhook(msg: ChatMessage) {
    let client = reqwest::Client::new();
    let _ = client.post(WEBHOOK_URL)
        .json(&msg)
        .send()
        .await;
}

async fn ws_handler(ws: WebSocketUpgrade, State(state): State<Arc<AppState>>) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

async fn handle_socket(mut socket: WebSocket, state: Arc<AppState>) {
    let session_id = Uuid::new_v4().to_string();
    let mut current_username: Option<String> = None;
    
    // Send history on connect
    {
        let history = state.history.read().await;
        for msg in history.iter() {
            let json = serde_json::to_string(msg).unwrap();
            let _ = socket.send(Message::Text(json.into())).await;
        }
    }
    
    // Send current user list
    {
        let users = state.users.read().await;
        let user_list = UserList {
            users: users.values().cloned().collect(),
            msg_type: "userlist".to_string(),
        };
        let json = serde_json::to_string(&user_list).unwrap();
        let _ = socket.send(Message::Text(json.into())).await;
    }

    let mut rx = state.tx.subscribe();
    let tx = state.tx.clone();
    let state_clone = state.clone();
    let session_id_clone = session_id.clone();

    loop {
        tokio::select! {
            Some(Ok(msg)) = socket.recv() => {
                if let Message::Text(text) = msg {
                    // Try to parse as join request
                    if let Ok(join_req) = serde_json::from_str::<JoinRequest>(&text) {
                        if join_req.msg_type == "join" {
                            let username = join_req.username.to_lowercase();
                            
                            // Check if username is taken
                            let mut taken = state_clone.taken_usernames.write().await;
                            let mut users = state_clone.users.write().await;
                            
                            // Allow if it's our own username or not taken
                            let is_own = current_username.as_ref() == Some(&username);
                            let is_taken = taken.contains(&username) && !is_own;
                            
                            if is_taken {
                                let response = JoinResponse {
                                    success: false,
                                    error: Some(format!("Username '{}' is already taken", username)),
                                    msg_type: "join_response".to_string(),
                                };
                                let json = serde_json::to_string(&response).unwrap();
                                let _ = socket.send(Message::Text(json.into())).await;
                            } else {
                                // Remove old username if changing
                                if let Some(old) = &current_username {
                                    taken.remove(old);
                                }
                                
                                // Set new username
                                taken.insert(username.clone());
                                users.insert(session_id_clone.clone(), username.clone());
                                current_username = Some(username.clone());
                                
                                let response = JoinResponse {
                                    success: true,
                                    error: None,
                                    msg_type: "join_response".to_string(),
                                };
                                let json = serde_json::to_string(&response).unwrap();
                                let _ = socket.send(Message::Text(json.into())).await;
                                
                                // Broadcast updated user list
                                drop(taken);
                                let user_list = UserList {
                                    users: users.values().cloned().collect(),
                                    msg_type: "userlist".to_string(),
                                };
                                let json = serde_json::to_string(&user_list).unwrap();
                                let _ = tx.send(json);
                                
                                // System message
                                let sys_msg = ChatMessage {
                                    username: "system".to_string(),
                                    content: format!("{} has joined", username),
                                    timestamp: chrono::Utc::now().format("%H:%M:%S").to_string(),
                                    msg_type: Some("system".to_string()),
                                };
                                let json = serde_json::to_string(&sys_msg).unwrap();
                                let _ = tx.send(json);
                            }
                            continue;
                        }
                    }
                    
                    // Regular chat message
                    if let Ok(chat_msg) = serde_json::from_str::<ChatMessage>(&text) {
                        if current_username.is_none() {
                            continue; // Must join first
                        }
                        
                        let username = current_username.clone().unwrap();
                        let msg_with_time = ChatMessage {
                            username: username.clone(),
                            content: chat_msg.content.clone(),
                            timestamp: chrono::Utc::now().format("%H:%M:%S").to_string(),
                            msg_type: Some("chat".to_string()),
                        };
                        let json = serde_json::to_string(&msg_with_time).unwrap();
                        
                        {
                            let mut h = state_clone.history.write().await;
                            h.push_back(msg_with_time.clone());
                            if h.len() > MAX_HISTORY {
                                h.pop_front();
                            }
                        }
                        
                        let _ = tx.send(json);
                        
                        // Webhook for @georges mentions
                        if chat_msg.content.to_lowercase().contains("@georges") {
                            tokio::spawn(send_webhook(msg_with_time));
                        }
                    }
                }
            }
            Ok(msg) = rx.recv() => {
                let _ = socket.send(Message::Text(msg.into())).await;
            }
            else => break,
        }
    }
    
    // Cleanup on disconnect
    if let Some(username) = current_username {
        let mut taken = state.taken_usernames.write().await;
        let mut users = state.users.write().await;
        taken.remove(&username);
        users.remove(&session_id);
        
        // Broadcast updated user list
        let user_list = UserList {
            users: users.values().cloned().collect(),
            msg_type: "userlist".to_string(),
        };
        let json = serde_json::to_string(&user_list).unwrap();
        let _ = state.tx.send(json);
        
        // System message
        let sys_msg = ChatMessage {
            username: "system".to_string(),
            content: format!("{} has left", username),
            timestamp: chrono::Utc::now().format("%H:%M:%S").to_string(),
            msg_type: Some("system".to_string()),
        };
        let json = serde_json::to_string(&sys_msg).unwrap();
        let _ = state.tx.send(json);
    }
}
