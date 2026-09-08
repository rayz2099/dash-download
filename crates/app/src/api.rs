//! 进程内控制面: UI invoke 与 native-host IPC 共用 dispatch.
//! 不再 bind TCP. 浏览器 CSRF 面随 41320 一起消失.

use crate::launch;
use dd_core::{AddTaskOptions, CoreError, Engine, EngineSettings, ProxyCfg, RequestContext};
use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use tauri::Manager;

pub struct ApiCtx {
    pub engine: Engine,
    /// setup 之后填入, 扩展接管时用来把主窗口拉到前台
    pub app: Arc<Mutex<Option<tauri::AppHandle>>>,
    pub cfg_dir: std::path::PathBuf,
    pub prefs: crate::prefs::Store,
}

fn err(e: impl ToString) -> String {
    e.to_string()
}

fn core(e: CoreError) -> String {
    e.to_string()
}

fn need_i64(req: &Value, k: &str) -> Result<i64, String> {
    req.get(k)
        .and_then(|v| v.as_i64())
        .ok_or_else(|| format!("缺少 {k}"))
}

fn ok() -> Value {
    json!({ "ok": true })
}

/// GET 不回传明文密码, 只带 pass_set 让设置页知道已保存过.
fn settings_json(s: EngineSettings) -> Value {
    let pass_set = !s.proxy.pass.is_empty();
    let mut s = s;
    s.proxy.pass.clear();
    let mut v = serde_json::to_value(&s).expect("settings 可序列化");
    v["proxy"]["pass_set"] = json!(pass_set);
    v["bt_direct"] = json!(s.bt_direct());
    v
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
struct AddReq {
    url: String,
    dir: Option<String>,
    name: Option<String>,
    segments: Option<u32>,
    #[serde(default)]
    queue_only: bool,
    #[serde(default)]
    headers: Vec<(String, String)>,
    #[serde(default)]
    content_b64: Option<String>,
    #[serde(default)]
    mime: Option<String>,
}

#[derive(Deserialize)]
struct AddTorrentReq {
    magnet: Option<String>,
    torrent_b64: Option<String>,
    torrent_url: Option<String>,
    dir: Option<String>,
    #[serde(default)]
    headers: Vec<(String, String)>,
}

#[derive(Deserialize)]
struct ProxyTestReq {
    url: String,
    proxy: ProxyCfg,
}

async fn add_task(ctx: &ApiCtx, req: AddReq) -> Result<Value, String> {
    let task = if let Some(b64) = req.content_b64 {
        let bytes = STANDARD
            .decode(b64.trim())
            .map_err(|e| format!("content_b64 非法: {e}"))?;
        ctx.engine
            .import_bytes(&req.url, req.name, req.mime, &bytes)
            .map_err(core)?
    } else {
        let opts = AddTaskOptions {
            dir: req.dir,
            name: req.name,
            segments: req.segments,
            queue_only: req.queue_only,
            ctx: RequestContext {
                headers: req.headers,
            },
        };
        ctx.engine.add(&req.url, opts).map_err(core)?
    };
    show_main(ctx);
    serde_json::to_value(task).map_err(err)
}

async fn add_torrent(ctx: &ApiCtx, req: AddTorrentReq) -> Result<Value, String> {
    let t = if let Some(m) = req.magnet.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        ctx.engine.add_magnet(m, req.dir).map_err(core)?
    } else if let Some(b64) = req.torrent_b64.as_deref() {
        let bytes = STANDARD
            .decode(b64.trim())
            .map_err(|e| format!("torrent_b64 非法: {e}"))?;
        ctx.engine
            .add_torrent_bytes(&bytes, "torrent", req.dir)
            .map_err(core)?
    } else if let Some(url) = req
        .torrent_url
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        let bytes = ctx
            .engine
            .fetch_torrent_url(url, &req.headers)
            .await
            .map_err(core)?;
        ctx.engine
            .add_torrent_bytes(&bytes, url, req.dir)
            .map_err(core)?
    } else {
        return Err("需要 magnet / torrent_b64 / torrent_url".into());
    };
    show_main(ctx);
    serde_json::to_value(t).map_err(err)
}

async fn put_settings(ctx: &ApiCtx, mut req: EngineSettings) -> Result<Value, String> {
    let prev = ctx.engine.settings();
    if req.proxy.pass.is_empty() {
        req.proxy.pass = prev.proxy.pass.clone();
    }
    let applied = ctx.engine.apply_settings(req).map_err(core)?;
    if let Err(e) = ctx.prefs.patch(|p| p.apply_engine(&applied)) {
        ctx.engine.apply_settings(prev).map_err(|rb| {
            format!("写盘失败 ({e}) 且回滚失败: {rb}")
        })?;
        return Err(e);
    }
    ctx.engine.pump_queue();
    Ok(settings_json(applied))
}

/// 扩展与 UI 共用的 op 分发. 未知 op 直接失败, 不静默吞.
pub async fn dispatch(ctx: &ApiCtx, req: Value) -> Result<Value, String> {
    let op = req.get("op").and_then(|v| v.as_str()).ok_or("缺少 op")?;
    match op {
        "ping" | "wake" => Ok(json!({
            "ok": true,
            "name": "dash-download",
            "version": env!("CARGO_PKG_VERSION"),
            "p2p": ctx.engine.settings().p2p,
        })),
        "focus" => {
            show_main(ctx);
            Ok(ok())
        }
        "list_tasks" => serde_json::to_value(ctx.engine.list().map_err(core)?).map_err(err),
        "add_task" => {
            let body: AddReq = serde_json::from_value(req).map_err(err)?;
            add_task(ctx, body).await
        }
        "pause_task" => {
            ctx.engine.pause(need_i64(&req, "id")?).map_err(core)?;
            Ok(ok())
        }
        "resume_task" => {
            ctx.engine.resume(need_i64(&req, "id")?).map_err(core)?;
            Ok(ok())
        }
        "cancel_task" => {
            ctx.engine.cancel(need_i64(&req, "id")?).map_err(core)?;
            Ok(ok())
        }
        "redownload_task" => {
            ctx.engine.redownload(need_i64(&req, "id")?).map_err(core)?;
            Ok(ok())
        }
        "set_connections" => {
            let id = need_i64(&req, "id")?;
            let n = req.get("n").and_then(|v| v.as_u64()).ok_or("缺少 n")? as u32;
            ctx.engine.set_connections(id, n).map_err(core)?;
            Ok(ok())
        }
        "remove_task" => {
            let id = need_i64(&req, "id")?;
            let del = req.get("delete_file").and_then(|v| v.as_bool()).unwrap_or(false);
            ctx.engine.remove(id, del).map_err(core)?;
            Ok(ok())
        }
        "pause_all" => {
            ctx.engine.pause_all().map_err(core)?;
            Ok(ok())
        }
        "resume_all" => {
            ctx.engine.resume_all().map_err(core)?;
            Ok(ok())
        }
        "get_settings" => Ok(settings_json(ctx.engine.settings())),
        "put_settings" => {
            let mut body = req.clone();
            if let Some(obj) = body.as_object_mut() {
                obj.remove("op");
                if let Some(inner) = obj.remove("settings") {
                    body = inner;
                }
            }
            let body: EngineSettings = serde_json::from_value(body).map_err(err)?;
            put_settings(ctx, body).await
        }
        "test_proxy" => {
            let mut body: ProxyTestReq = serde_json::from_value(req).map_err(err)?;
            if body.proxy.pass.is_empty() {
                body.proxy.pass = ctx.engine.settings().proxy.pass;
            }
            let r = ctx.engine.probe_url(&body.proxy, &body.url).await.map_err(core)?;
            serde_json::to_value(r).map_err(err)
        }
        "list_torrents" => {
            serde_json::to_value(ctx.engine.list_torrents().map_err(core)?).map_err(err)
        }
        "add_torrent" => {
            let body: AddTorrentReq = serde_json::from_value(req).map_err(err)?;
            add_torrent(ctx, body).await
        }
        "pause_torrent" => {
            ctx.engine
                .pause_torrent(need_i64(&req, "id")?)
                .map_err(core)?;
            Ok(ok())
        }
        "resume_torrent" => {
            ctx.engine
                .resume_torrent(need_i64(&req, "id")?)
                .map_err(core)?;
            Ok(ok())
        }
        "select_files" => {
            let id = need_i64(&req, "id")?;
            let selected: Vec<u32> = serde_json::from_value(
                req.get("selected").cloned().unwrap_or(json!([])),
            )
            .map_err(err)?;
            serde_json::to_value(
                ctx.engine.select_torrent_files(id, selected).map_err(core)?,
            )
            .map_err(err)
        }
        "remove_torrent" => {
            let id = need_i64(&req, "id")?;
            let del = req.get("delete_file").and_then(|v| v.as_bool()).unwrap_or(false);
            ctx.engine.remove_torrent(id, del).await.map_err(core)?;
            Ok(ok())
        }
        "remember_origin" => {
            let origin = req
                .get("origin")
                .and_then(|v| v.as_str())
                .ok_or("缺少 origin")?;
            launch::remember_origin(&ctx.cfg_dir, origin)?;
            Ok(ok())
        }
        _ => Err(format!("未知 op: {op}")),
    }
}
