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
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::Manager;
use tauri_plugin_autostart::ManagerExt;

use launch::API_PORT;

/// 只在 debug / `tauri dev` 开日志. 同时写 stderr 和 cfg_dir/debug.log, 方便 agent tail.
fn init_debug_log(cfg_dir: &PathBuf) {
    #[cfg(debug_assertions)]
    {
        let path = cfg_dir.join("debug.log");
        let file = match std::fs::File::create(&path) {
            Ok(f) => f,
            Err(e) => {
                eprintln!("无法写 {}: {e}", path.display());
                let _ = tracing_subscriber::fmt()
                    .with_env_filter(dev_filter())
                    .with_target(true)
                    .with_file(true)
                    .with_line_number(true)
                    .try_init();
                return;
            }
        };
        eprintln!("dd debug log: {}", path.display());
        let tee = Tee {
            file: std::sync::Arc::new(std::sync::Mutex::new(file)),
        };
        let _ = tracing_subscriber::fmt()
            .with_env_filter(dev_filter())
            .with_target(true)
            .with_file(true)
            .with_line_number(true)
            .with_writer(tee)
            .try_init();
    }
    #[cfg(not(debug_assertions))]
    {
        let _ = cfg_dir;
    }
}

#[cfg(debug_assertions)]
fn dev_filter() -> tracing_subscriber::EnvFilter {
    tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        tracing_subscriber::EnvFilter::new(
            "dd_core=debug,dd_app=debug,librqbit=info,librqbit_dht=error",
        )
    })
}

#[cfg(debug_assertions)]
#[derive(Clone)]
struct Tee {
    file: std::sync::Arc<std::sync::Mutex<std::fs::File>>,
}

#[cfg(debug_assertions)]
impl std::io::Write for Tee {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let _ = std::io::Write::write_all(&mut std::io::stderr(), buf);
        std::io::Write::write_all(&mut *self.file.lock().unwrap(), buf)?;
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        let _ = std::io::Write::flush(&mut std::io::stderr());
        std::io::Write::flush(&mut *self.file.lock().unwrap())
    }
}

#[cfg(debug_assertions)]
impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Tee {
    type Writer = Tee;
    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

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

fn spawn_open(path: &PathBuf) {
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open").arg(path).spawn();
    }
    #[cfg(target_os = "linux")]
    {
        let _ = std::process::Command::new("xdg-open").arg(path).spawn();
    }
    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("explorer").arg(path).spawn();
    }
}

/// 目标在就打开; 文件/子目录还没写下时打开 fallback / 默认下载目录, 避免 click 无反馈.
fn open_existing_or_dir(path: &str, fallback: Option<&str>, default_dir: &str) {
    for c in [Some(path), fallback, Some(default_dir)].into_iter().flatten() {
        let p = PathBuf::from(c);
        if p.exists() {
            spawn_open(&p);
            return;
        }
    }
    // 默认目录也被删了: 建出来再打开, Finder 必须有窗口
    let dest = PathBuf::from(default_dir);
    if !dest.as_os_str().is_empty() {
        let _ = std::fs::create_dir_all(&dest);
        spawn_open(&dest);
    }
}

/// 在 Finder 中显示文件; 文件不存在则打开默认下载目录
#[tauri::command]
fn reveal(path: String, fallback: Option<String>, boot: tauri::State<Boot>) {
    let p = PathBuf::from(&path);
    if p.is_file() {
        #[cfg(target_os = "macos")]
        {
            let _ = std::process::Command::new("open").arg("-R").arg(&p).spawn();
            return;
        }
        #[cfg(not(target_os = "macos"))]
        {
            spawn_open(&p);
            return;
        }
    }
    open_existing_or_dir(&path, fallback.as_deref(), &boot.default_dir);
}

/// 打开文件或目录. 不传 fallback 时文件不存在直接报错, 不偷偷打开目录.
#[tauri::command]
fn open_path(path: String, fallback: Option<String>, boot: tauri::State<Boot>) -> Result<(), String> {
    let p = PathBuf::from(&path);
    if p.exists() {
        spawn_open(&p);
        return Ok(());
    }
    if fallback.is_none() {
        return Err("文件不存在".into());
    }
    open_existing_or_dir(&path, fallback.as_deref(), &boot.default_dir);
    Ok(())
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

fn show_main_window(app: &tauri::AppHandle) {
    #[cfg(target_os = "macos")]
    let _ = app.show();
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.unminimize();
        let _ = win.center();
        let _ = win.show();
        let _ = win.set_focus();
    } else {
        eprintln!("main window 不存在, 无法前置");
    }
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
    init_debug_log(&cfg_dir);
    // just dev 和托盘正式版抢 41320; 占着时 UI 会连到旧进程, 窗口也像没起来
    if launch::api_up() {
        eprintln!(
            "127.0.0.1:{API_PORT} 已被占用. 退出托盘里的 Dash Download 后再跑 just dev."
        );
        std::process::exit(1);
    }
    let user_prefs = match prefs::Store::load(&cfg_dir) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };
    let snapshot = user_prefs.get();
    let download_dir = if snapshot.engine.default_dir.trim().is_empty() {
        dirs::download_dir()
            .unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join("Downloads"))
    } else {
        PathBuf::from(&snapshot.engine.default_dir)
    };
    if let Err(e) = launch::install_native_host(&cfg_dir) {
        eprintln!("native host 注册失败: {e}");
    }

    // 引擎与 API 跑在独立 tokio runtime 线程上;
    // UI/扩展只通过 localhost API 访问引擎, tauri 主线程不碰引擎
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let mut settings = snapshot.engine.clone();
    settings.default_dir = download_dir.to_string_lossy().into_owned();
    settings.fill_listen_port();
    let engine_cfg = EngineConfig::with_settings(cfg_dir.join("tasks.sqlite"), settings);
    let engine = rt
        .block_on(async { Engine::new(engine_cfg).await })
        .expect("engine init");
    let applied = engine.settings();
    let _ = user_prefs.patch(|p| p.apply_engine(&applied));
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

    let builder = tauri::Builder::default();
    // just dev 与托盘里的正式版同 identifier; debug 挂上会被吃掉, 窗口不出现
    #[cfg(not(debug_assertions))]
    let builder = builder.plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
        if let Some(win) = app.get_webview_window("main") {
            let _ = win.show();
            let _ = win.unminimize();
            let _ = win.set_focus();
        }
    }));
    builder
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
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Regular);
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
                // 左键直接出窗口: 刘海把菜单栏图标挤没时, 点一下还能用
                .show_menu_on_left_click(false)
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        show_main_window(tray.app_handle());
                    }
                })
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => show_main_window(app),
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
        .build(tauri::generate_context!())
        .expect("tauri build")
        .run(move |app, event| match event {
            // setup 里 show 太早, 事件循环起来后再抬一次
            tauri::RunEvent::Ready => {
                if !start_hidden {
                    show_main_window(app);
                }
            }
            // 点 Dock 图标: 刘海挡住托盘时这是唯一入口
            tauri::RunEvent::Reopen { .. } => show_main_window(app),
            _ => {}
        });
}
