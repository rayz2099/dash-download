//! Tauri app 入口: Rust 常驻核心 (引擎 + localhost API) + webview UI.
//! 关窗只隐藏, 进程随托盘存活, 下载不中断 (ADR 0005).

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod api;

use dd_core::{Engine, EngineConfig};
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

fn main() {
    let cfg_dir = config_dir();
    let _ = std::fs::create_dir_all(&cfg_dir);
    let download_dir = dirs::download_dir()
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join("Downloads"));

    // 引擎与 API 跑在独立 tokio runtime 线程上;
    // UI/扩展只通过 localhost API 访问引擎, tauri 主线程不碰引擎
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let engine_cfg = EngineConfig::new(cfg_dir.join("tasks.sqlite"), download_dir.clone());
    let engine = rt.block_on(async { Engine::new(engine_cfg) }).expect("engine init");
    let app_slot: Arc<std::sync::Mutex<Option<tauri::AppHandle>>> =
        Arc::new(std::sync::Mutex::new(None));
    let api_ctx = Arc::new(api::ApiCtx {
        engine,
        app: app_slot.clone(),
    });
    rt.spawn(async move {
        if let Err(e) = api::serve(api_ctx, API_PORT).await {
            eprintln!("API server 退出: {e}");
        }
    });
    // runtime 生命周期与进程一致, 有意泄漏避免 drop 时杀掉下载任务
    std::mem::forget(rt);

    let boot = Boot {
        port: API_PORT,
        default_dir: download_dir.to_string_lossy().into_owned(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    };

    tauri::Builder::default()
        .manage(boot)
        .invoke_handler(tauri::generate_handler![bootstrap, reveal, open_path])
        .setup({
            let app_slot = app_slot.clone();
            move |app| {
            *app_slot.lock().unwrap() = Some(app.handle().clone());
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
            }
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
