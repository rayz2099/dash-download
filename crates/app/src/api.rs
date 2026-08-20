//! localhost REST + WS API: Chrome 扩展与 webview UI 共用的唯一入口 (ADR 0003).
//! 仅绑定 127.0.0.1. 无配对 token: 浏览器 CSRF 靠 CORS 源白名单 + 自定义头 `x-dd-client`
//! 强制预检; WS 校验 Origin. 对齐 NDM "app 在跑就能接管" 的交互.

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderMap, HeaderName, HeaderValue, Method, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use dd_core::{AddTaskOptions, CoreError, Engine, EngineSettings, ProxyCfg, RequestContext};
use serde::Deserialize;
use serde_json::json;
use std::sync::{Arc, Mutex};
use tower_http::cors::{AllowOrigin, CorsLayer};
use tauri::Manager;

pub struct ApiCtx {
    pub engine: Engine,
    /// setup 之后填入, 扩展接管时用来把主窗口拉到前台
    pub app: Arc<Mutex<Option<tauri::AppHandle>>>,
    pub cfg_dir: std::path::PathBuf,
    pub prefs: crate::prefs::Store,
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
        .allow_origin(AllowOrigin::predicate(|origin, _req| origin_ok(origin)))
        .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE, Method::OPTIONS])
        .allow_headers([
            header::CONTENT_TYPE,
            HeaderName::from_static("x-dd-client"),
        ]);

    let authed = Router::new()
        .route("/api/tasks", get(list_tasks).post(add_task))
        .route("/api/tasks/{id}/pause", post(pause_task))
        .route("/api/tasks/{id}/resume", post(resume_task))
        .route("/api/tasks/{id}/cancel", post(cancel_task))
        .route("/api/tasks/{id}/redownload", post(redownload_task))
        .route("/api/tasks/{id}/connections", post(set_connections))
        .route("/api/tasks/{id}", delete(remove_task))
        .route("/api/pause-all", post(pause_all))
        .route("/api/resume-all", post(resume_all))
        .route("/api/settings", get(get_settings).put(put_settings))
        .route("/api/proxy-test", post(test_proxy))
        .route("/api/focus", post(focus_window))
        .route("/api/ext-origin", post(ext_origin))
        .layer(middleware::from_fn(check_client));

    let app = Router::new()
        // ping 不要求自定义头: 扩展 popup 健康检查
        .route("/api/ping", get(ping))
        // WS 无法带自定义 header, 只在 handler 里校验 Origin
        .route("/api/ws", get(ws_handler))
        .merge(authed)
        .layer(cors)
        .with_state(ctx);

    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port)).await?;
    axum::serve(listener, app).await
}

/// 允许的浏览器 Origin: 本机 webview / vite / 任意 chrome 扩展.
/// 恶意网页带自己的 Origin, 不在白名单, CORS 预检失败且中间件直接拒.
fn origin_ok(origin: &HeaderValue) -> bool {
    let Ok(s) = origin.to_str() else {
        return false;
    };
    s.starts_with("chrome-extension://")
        || s == "http://localhost:5173"
        || s == "http://127.0.0.1:5173"
        || s == "https://tauri.localhost"
        || s == "http://tauri.localhost"
        || s == "tauri://localhost"
}

fn origin_allowed(headers: &HeaderMap) -> bool {
    match headers.get(header::ORIGIN) {
        // curl / 本机工具不带 Origin, 放行; 浏览器跨站请求总会带
        None => true,
        Some(v) => origin_ok(v),
    }
}

/// 控制面: Origin 白名单 + 强制 `x-dd-client`, 让浏览器无法发 simple request CSRF.
async fn check_client(headers: HeaderMap, req: axum::extract::Request, next: Next) -> Response {
    if !origin_allowed(&headers) {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({ "error": "origin 不允许" })),
        )
            .into_response();
    }
    let client_ok = headers
        .get("x-dd-client")
        .and_then(|v| v.to_str().ok())
        .map(|v| !v.is_empty())
        .unwrap_or(false);
    if !client_ok {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "缺少 x-dd-client" })),
        )
            .into_response();
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
    let task = ctx.engine.add(&req.url, opts)?;
    show_main(&ctx);
    Ok(Json(task).into_response())
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

async fn cancel_task(
    State(ctx): State<Arc<ApiCtx>>,
    Path(id): Path<i64>,
) -> Result<Response, ApiError> {
    ctx.engine.cancel(id)?;
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
struct ConnReq {
    n: u32,
}

async fn set_connections(
    State(ctx): State<Arc<ApiCtx>>,
    Path(id): Path<i64>,
    Json(req): Json<ConnReq>,
) -> Result<Response, ApiError> {
    ctx.engine.set_connections(id, req.n)?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

async fn focus_window(State(ctx): State<Arc<ApiCtx>>) -> Response {
    show_main(&ctx);
    StatusCode::NO_CONTENT.into_response()
}

fn show_main(ctx: &ApiCtx) {
    let app = ctx.app.lock().unwrap();
    if let Some(app) = app.as_ref() {
        if let Some(win) = app.get_webview_window("main") {
            let _ = win.unminimize();
            let _ = win.show();
            let _ = win.set_focus();
        }
    }
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


async fn get_settings(State(ctx): State<Arc<ApiCtx>>) -> Json<EngineSettings> {
    Json(ctx.engine.settings())
}

/// 写 prefs.json 与热更新引擎必须同一次成功, 避免 UI 显示已保存但下载仍走旧代理.
async fn put_settings(
    State(ctx): State<Arc<ApiCtx>>,
    Json(req): Json<EngineSettings>,
) -> Result<Response, ApiError> {
    let applied = ctx.engine.apply_settings(req)?;
    ctx.prefs
        .patch(|p| p.apply_engine(&applied))
        .map_err(|e| ApiError(CoreError::Other(e)))?;
    Ok(Json(applied).into_response())
}


#[derive(Deserialize)]
struct ProxyTestReq {
    url: String,
    proxy: ProxyCfg,
}

async fn test_proxy(
    State(ctx): State<Arc<ApiCtx>>,
    Json(req): Json<ProxyTestReq>,
) -> Result<Response, ApiError> {
    let r = ctx.engine.probe_url(&req.proxy, &req.url).await?;
    Ok(Json(r).into_response())
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
    headers: HeaderMap,
    upgrade: WebSocketUpgrade,
) -> Response {
    if !origin_allowed(&headers) {
        return (StatusCode::FORBIDDEN, "origin 不允许").into_response();
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

#[derive(Deserialize)]
struct ExtOrigin {
    origin: String,
}

/// 扩展上报自身 origin, 写入 native host 白名单, 下次没跑 app 时仍能被拉起.
async fn ext_origin(State(ctx): State<Arc<ApiCtx>>, Json(req): Json<ExtOrigin>) -> Result<Response, ApiError> {
    crate::launch::remember_origin(&ctx.cfg_dir, &req.origin).map_err(|e| {
        dd_core::CoreError::Other(e)
    })?;
    Ok(Json(json!({ "ok": true })).into_response())
}
