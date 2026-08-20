//! localhost REST + WS API: Chrome 扩展与 webview UI 共用的唯一入口 (ADR 0003).
//! 仅绑定 127.0.0.1; 鉴权用静态 token (header `x-dd-token`, WS 用 query `?token=`).

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use dd_core::{AddTaskOptions, Engine, RequestContext};
use serde::Deserialize;
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};

pub struct ApiCtx {
    pub engine: Engine,
    pub token: String,
}

struct ApiError(dd_core::CoreError);

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let code = match &self.0 {
            dd_core::CoreError::NotFound(_) => StatusCode::NOT_FOUND,
            dd_core::CoreError::Other(_) => StatusCode::BAD_REQUEST,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };
        (code, Json(json!({ "error": self.0.to_string() }))).into_response()
    }
}

impl From<dd_core::CoreError> for ApiError {
    fn from(e: dd_core::CoreError) -> Self {
        ApiError(e)
    }
}

pub async fn serve(ctx: Arc<ApiCtx>, port: u16) -> std::io::Result<()> {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let authed = Router::new()
        .route("/api/tasks", get(list_tasks).post(add_task))
        .route("/api/tasks/{id}/pause", post(pause_task))
        .route("/api/tasks/{id}/resume", post(resume_task))
        .route("/api/tasks/{id}/redownload", post(redownload_task))
        .route("/api/tasks/{id}", delete(remove_task))
        .route("/api/pause-all", post(pause_all))
        .route("/api/resume-all", post(resume_all))
        .layer(middleware::from_fn_with_state(ctx.clone(), check_token));

    let app = Router::new()
        // ping 不鉴权: 扩展配对前的健康检查
        .route("/api/ping", get(ping))
        // WS 在 handler 内部校验 query token (浏览器 WS 无法带自定义 header)
        .route("/api/ws", get(ws_handler))
        .merge(authed)
        .layer(cors)
        .with_state(ctx);

    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port)).await?;
    axum::serve(listener, app).await
}

async fn check_token(
    State(ctx): State<Arc<ApiCtx>>,
    headers: HeaderMap,
    req: axum::extract::Request,
    next: Next,
) -> Response {
    let ok = headers
        .get("x-dd-token")
        .and_then(|v| v.to_str().ok())
        .map(|v| v == ctx.token)
        .unwrap_or(false);
    if !ok {
        return (StatusCode::UNAUTHORIZED, Json(json!({ "error": "token 无效" }))).into_response();
    }
    next.run(req).await
}

async fn ping() -> impl IntoResponse {
    Json(json!({ "name": "dash-download", "version": env!("CARGO_PKG_VERSION") }))
}

async fn list_tasks(State(ctx): State<Arc<ApiCtx>>) -> Result<Response, ApiError> {
    Ok(Json(ctx.engine.list()?).into_response())
}

#[derive(Deserialize)]
struct AddReq {
    url: String,
    dir: Option<String>,
    name: Option<String>,
    segments: Option<u32>,
    #[serde(default)]
    queue_only: bool,
    #[serde(default)]
    headers: Vec<(String, String)>,
}

async fn add_task(
    State(ctx): State<Arc<ApiCtx>>,
    Json(req): Json<AddReq>,
) -> Result<Response, ApiError> {
    let opts = AddTaskOptions {
        dir: req.dir,
        name: req.name,
        segments: req.segments,
        queue_only: req.queue_only,
        ctx: RequestContext { headers: req.headers },
    };
    Ok(Json(ctx.engine.add(&req.url, opts)?).into_response())
}

async fn pause_task(
    State(ctx): State<Arc<ApiCtx>>,
    Path(id): Path<i64>,
) -> Result<Response, ApiError> {
    ctx.engine.pause(id)?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

async fn resume_task(
    State(ctx): State<Arc<ApiCtx>>,
    Path(id): Path<i64>,
) -> Result<Response, ApiError> {
    ctx.engine.resume(id)?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

async fn redownload_task(
    State(ctx): State<Arc<ApiCtx>>,
    Path(id): Path<i64>,
) -> Result<Response, ApiError> {
    ctx.engine.redownload(id)?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

#[derive(Deserialize)]
struct RemoveQuery {
    #[serde(default)]
    delete_file: bool,
}

async fn remove_task(
    State(ctx): State<Arc<ApiCtx>>,
    Path(id): Path<i64>,
    Query(q): Query<RemoveQuery>,
) -> Result<Response, ApiError> {
    ctx.engine.remove(id, q.delete_file)?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

async fn pause_all(State(ctx): State<Arc<ApiCtx>>) -> Result<Response, ApiError> {
    ctx.engine.pause_all()?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

async fn resume_all(State(ctx): State<Arc<ApiCtx>>) -> Result<Response, ApiError> {
    ctx.engine.resume_all()?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

async fn ws_handler(
    State(ctx): State<Arc<ApiCtx>>,
    Query(params): Query<HashMap<String, String>>,
    upgrade: WebSocketUpgrade,
) -> Response {
    if params.get("token") != Some(&ctx.token) {
        return (StatusCode::UNAUTHORIZED, "token 无效").into_response();
    }
    upgrade.on_upgrade(move |socket| ws_loop(socket, ctx))
}

/// WS 推送: 连接时先发全量快照, 之后转发引擎事件流.
/// 客户端断线由 send 失败自然终止循环, 无需心跳 (localhost 不存在中间设备超时)
async fn ws_loop(mut socket: WebSocket, ctx: Arc<ApiCtx>) {
    let mut events = ctx.engine.subscribe();
    let snapshot = match ctx.engine.list() {
        Ok(tasks) => json!({ "type": "snapshot", "tasks": tasks }),
        Err(e) => json!({ "type": "error", "error": e.to_string() }),
    };
    if socket.send(Message::text(snapshot.to_string())).await.is_err() {
        return;
    }
    loop {
        tokio::select! {
            ev = events.recv() => {
                let ev = match ev {
                    Ok(ev) => ev,
                    // 消费太慢被挤掉队 (lagged): 重发快照对齐状态
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                        if let Ok(tasks) = ctx.engine.list() {
                            let msg = json!({ "type": "snapshot", "tasks": tasks });
                            if socket.send(Message::text(msg.to_string())).await.is_err() {
                                return;
                            }
                        }
                        continue;
                    }
                    Err(_) => return,
                };
                let payload = serde_json::to_string(&ev).unwrap_or_default();
                if socket.send(Message::text(payload)).await.is_err() {
                    return;
                }
            }
            msg = socket.recv() => {
                match msg {
                    None | Some(Err(_)) => return,
                    Some(Ok(_)) => {} // 客户端消息忽略, 控制面走 REST
                }
            }
        }
    }
}
