use axum::{
    extract::{ws::{Message, WebSocket, WebSocketUpgrade}, State, Request},
    http::{StatusCode, HeaderValue, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex;
use tower_http::services::ServeDir;

#[derive(Serialize, Deserialize, Clone)]
pub struct VitalStats {
    pub heart_rate: Option<u32>,
    pub owner_present: Option<bool>,
    pub face_count: u32,
}

#[derive(Clone)]
pub struct AppState {
    pub vitals: Arc<Mutex<VitalStats>>,
    pub token: String,
}

pub struct ApiServer {
    state: AppState,
}

impl ApiServer {
    pub fn new(token: String) -> Self {
        Self {
            state: AppState {
                vitals: Arc::new(Mutex::new(VitalStats {
                    heart_rate: Some(85),
                    owner_present: Some(true),
                    face_count: 1,
                })),
                token,
            },
        }
    }

    pub async fn start(&self) {
        let app = Router::new()
            .route("/api/status", get(status_handler))
            .route("/api/vitals", get(vitals_handler))
            .route("/api/stream", get(ws_handler))
            .route("/api/toggle", post(toggle_handler))
            .layer(middleware::from_fn_with_state(self.state.clone(), auth_middleware))
            .fallback_service(ServeDir::new("../aegis-mobile/dist"))
            .with_state(self.state.clone());

        println!("Listening on 0.0.0.0:8817");
        let listener = tokio::net::TcpListener::bind("0.0.0.0:8817").await.unwrap();
        axum::serve(listener, app).await.unwrap();
    }
}

async fn auth_middleware(
    State(state): State<AppState>,
    req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    if req.uri().path() == "/" || req.uri().path().starts_with("/assets") {
        return Ok(next.run(req).await);
    }
    
    let auth_header = req.headers().get(header::AUTHORIZATION);
    if let Some(auth) = auth_header {
        if let Ok(auth_str) = auth.to_str() {
            if auth_str == format!("Bearer {}", state.token) {
                return Ok(next.run(req).await);
            }
        }
    }
    
    Err(StatusCode::UNAUTHORIZED)
}

async fn status_handler() -> impl IntoResponse {
    Json(serde_json::json!({ "status": "running" }))
}

async fn vitals_handler(State(state): State<AppState>) -> impl IntoResponse {
    let vitals = state.vitals.lock().await;
    Json(vitals.clone())
}

#[derive(Deserialize)]
struct ToggleReq {
    module: String,
    on: bool,
}

async fn toggle_handler(Json(req): Json<ToggleReq>) -> impl IntoResponse {
    Json(serde_json::json!({ "module": req.module, "on": req.on }))
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

async fn handle_socket(mut socket: WebSocket, state: AppState) {
    loop {
        let vitals = {
            let v = state.vitals.lock().await;
            serde_json::to_string(&*v).unwrap()
        };
        if socket.send(Message::Text(vitals)).await.is_err() {
            break;
        }
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    }
}
