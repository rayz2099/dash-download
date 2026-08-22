//! Tauri app 入口: Rust 常驻核心 (引擎 + localhost API) + webview UI.
//! 关窗只隐藏, 进程随托盘存活, 下载不中断 (ADR 0005).

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod api;
mod launch;
mod prefs;
mod updater;

use dd_core::{Engine, EngineConfig};
use serde::Serialize;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::Manager;
use tauri_plugin_autostart::ManagerExt;

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

#[tauri::command]
fn update_status(up: tauri::State<updater::Updater>) -> updater::Status {
    up.snapshot()
}

#[tauri::command]
async fn check_update(
    app: tauri::AppHandle,
    up: tauri::State<'_, updater::Updater>,
) -> Result<updater::Status, String> {
    // 只探测, 有新版本由 UI 确认后再走 check_now 的安装路径.
    up.check(&app).await
}

#[tauri::command]
async fn check_now(
    app: tauri::AppHandle,
    up: tauri::State<'_, updater::Updater>,
) -> Result<updater::Status, String> {
    up.run(&app, true).await
}

#[tauri::command]
fn set_auto_update(
    app: tauri::AppHandle,
    up: tauri::State<updater::Updater>,
    enabled: bool,
) -> Result<updater::Status, String> {
    let st = up.set_auto_update(enabled)?;
    if enabled {
        let handle = app.clone();
        let up = up.inner().clone();
        tauri::async_runtime::spawn(async move {
            let _ = up.run(&handle, false).await;
        });
    }
    Ok(st)
}

#[tauri::command]
fn auto_start_on(app: tauri::AppHandle) -> Result<bool, String> {
    app.autolaunch().is_enabled().map_err(|e| e.to_string())
}

#[tauri::command]
fn set_auto_start(
    app: tauri::AppHandle,
    up: tauri::State<updater::Updater>,
    enabled: bool,
) -> Result<bool, String> {
    up.set_auto_start_pref(enabled)?;
    if enabled {
        app.autolaunch().enable().map_err(|e| e.to_string())?;
    } else {
        app.autolaunch().disable().map_err(|e| e.to_string())?;
    }
    Ok(enabled)
}

fn config_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("dash-download")
}

fn is_nm_host() -> bool {
    if std::env::args().any(|a| a == "--native-host") {
        return true;
    }
    // Windows 不能在 manifest path 里带参数, 复制出的 nm-host.exe 靠文件名进这个模式
    match std::env::current_exe() {
        Ok(p) => p
            .file_stem()
            .and_then(|s| s.to_str())
            .is_some_and(|s| s.eq_ignore_ascii_case("nm-host")),
        Err(_) => false,
    }
}

fn main() {
    if is_nm_host() {
        launch::run_native_host();
        return;
    }

    let cfg_dir = config_dir();
    let _ = std::fs::create_dir_all(&cfg_dir);
    let user_prefs = match prefs::Store::load(&cfg_dir) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };
    let snapshot = user_prefs.get();
    let download_dir = if snapshot.default_dir.trim().is_empty() {
        dirs::download_dir()
            .unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join("Downloads"))
    } else {
        PathBuf::from(&snapshot.default_dir)
    };
    if let Err(e) = launch::install_native_host(&cfg_dir) {
        eprintln!("native host 注册失败: {e}");
    }

    // 引擎与 API 跑在独立 tokio runtime 线程上;
    // UI/扩展只通过 localhost API 访问引擎, tauri 主线程不碰引擎
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let mut engine_cfg = EngineConfig::new(cfg_dir.join("tasks.sqlite"), download_dir.clone());
    engine_cfg.max_concurrent = snapshot.max_concurrent as usize;
    engine_cfg.max_segments = snapshot.max_segments;
    engine_cfg.proxy = snapshot.proxy.clone();
    let engine = rt.block_on(async { Engine::new(engine_cfg) }).expect("engine init");
    let app_slot: Arc<std::sync::Mutex<Option<tauri::AppHandle>>> =
        Arc::new(std::sync::Mutex::new(None));
    let api_ctx = Arc::new(api::ApiCtx {
        engine: engine.clone(),
        app: app_slot.clone(),
        cfg_dir: cfg_dir.clone(),
        prefs: user_prefs.clone(),
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
    let updater = updater::Updater::new(engine, user_prefs.clone());
    let start_hidden = std::env::args().any(|a| a == "--hidden");

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            if let Some(win) = app.get_webview_window("main") {
                let _ = win.show();
                let _ = win.unminimize();
                let _ = win.set_focus();
            }
        }))
        .plugin(
            tauri_plugin_autostart::Builder::new()
                .args(["--hidden"])
                .app_name("Dash Download")
                .build(),
        )
        // 设置页选目录必须走系统面板, 不能靠 webview 文件输入
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(boot)
        .manage(updater)
        .invoke_handler(tauri::generate_handler![
            bootstrap,
            reveal,
            open_path,
            update_status,
            check_update,
            check_now,
            set_auto_update,
            auto_start_on,
            set_auto_start
        ])
        .setup({
            let app_slot = app_slot.clone();
            move |app| {
            *app_slot.lock().unwrap() = Some(app.handle().clone());
            let auto = if snapshot.auto_start {
                app.autolaunch().enable()
            } else {
                app.autolaunch().disable()
            };
            if let Err(e) = auto {
                eprintln!("开机自启设置失败: {e}");
            }
            if start_hidden {
                if let Some(win) = app.get_webview_window("main") {
                    let _ = win.hide();
                }
            }
            updater::spawn_loop(app.handle().clone());
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
