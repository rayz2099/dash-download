//! Tauri app 入口: Rust 常驻核心 (引擎 + localhost API) + webview UI.
//! 关窗只隐藏, 进程随托盘存活, 下载不中断 (ADR 0005).

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod api;

use dd_core::{Engine, EngineConfig};
use rand::Rng;
use serde::Serialize;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::Manager;

const API_PORT: u16 = 41320;

/// UI 启动时通过 invoke 拿到的引导信息, 之后全部流量走 localhost API
#[derive(Clone, Serialize)]
struct Boot {
    port: u16,
    token: String,
    default_dir: String,
    version: String,
}

#[tauri::command]
fn bootstrap(state: tauri::State<Boot>) -> Boot {
    state.inner().clone()
}

/// 在 Finder 中显示文件 (macOS); 其他平台后续补齐
#[tauri::command]
fn reveal(path: String) {
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open").arg("-R").arg(&path).spawn();
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = path;
    }
}

/// 用系统默认程序打开文件 (右键菜单"打开")
#[tauri::command]
fn open_path(path: String) {
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open").arg(&path).spawn();
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = path;
    }
}

fn config_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("dash-download")
}

/// token 首次生成后持久化, app 与扩展凭它配对
fn load_or_create_token(dir: &PathBuf) -> String {
    let path = dir.join("token");
    if let Ok(t) = std::fs::read_to_string(&path) {
        let t = t.trim().to_string();
        if !t.is_empty() {
            return t;
        }
    }
    let token: String = {
        const CHARS: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";
        let mut rng = rand::thread_rng();
        let body: String =
            (0..32).map(|_| CHARS[rng.gen_range(0..CHARS.len())] as char).collect();
        format!("dd_{body}")
    };
    let _ = std::fs::create_dir_all(dir);
    let _ = std::fs::write(&path, &token);
    token
}

fn main() {
    let cfg_dir = config_dir();
    let token = load_or_create_token(&cfg_dir);
    let download_dir = dirs::download_dir()
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join("Downloads"));

    // 引擎与 API 跑在独立 tokio runtime 线程上;
    // UI/扩展只通过 localhost API 访问引擎, tauri 主线程不碰引擎
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let engine_cfg = EngineConfig::new(cfg_dir.join("tasks.sqlite"), download_dir.clone());
    let engine = rt.block_on(async { Engine::new(engine_cfg) }).expect("engine init");
    let api_ctx = Arc::new(api::ApiCtx { engine, token: token.clone() });
    rt.spawn(async move {
        if let Err(e) = api::serve(api_ctx, API_PORT).await {
            eprintln!("API server 退出: {e}");
        }
    });
    // runtime 生命周期与进程一致, 有意泄漏避免 drop 时杀掉下载任务
    std::mem::forget(rt);

    let boot = Boot {
        port: API_PORT,
        token,
        default_dir: download_dir.to_string_lossy().into_owned(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    };

    tauri::Builder::default()
        .manage(boot)
        .invoke_handler(tauri::generate_handler![bootstrap, reveal, open_path])
        .setup(|app| {
            let show = MenuItem::with_id(app, "show", "显示主窗口", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "退出 Dash Download", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show, &quit])?;
            TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .menu(&menu)
                .show_menu_on_left_click(true)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => {
                        if let Some(win) = app.get_webview_window("main") {
                            let _ = win.show();
                            let _ = win.set_focus();
                        }
                    }
                    "quit" => app.exit(0),
                    _ => {}
                })
                .build(app)?;
            Ok(())
        })
        .on_window_event(|window, event| {
            // 关窗 = 隐藏, 引擎继续跑; 真正退出走托盘菜单
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let _ = window.hide();
                api.prevent_close();
            }
        })
        .run(tauri::generate_context!())
        .expect("tauri run");
}
