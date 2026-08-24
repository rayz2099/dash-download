//! 开机自启 + Chrome Native Messaging 最小 host.
//! ADR 0003 的数据面仍是 localhost API; native host 只负责在 app 没跑时被扩展拉起,
//! 否则 Takeover 会先 abort 浏览器下载再失败.

use serde_json::{json, Value};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

pub const HOST_NAME: &str = "dev.ray.dash_download";
pub const API_PORT: u16 = 41320;
/// 扩展 manifest.key 算出的稳定 ID, 未连上 API 也能写进 native host 白名单.
pub const EXT_ID: &str = "agdjpgikicokkkbdgmdmhdpbhljieech";
pub const EXT_ORIGIN: &str = "chrome-extension://agdjpgikicokkkbdgmdmhdpbhljieech/";

/// Chrome 的 Origin 通常无尾斜杠, native host 白名单必须有; 两者都认成本扩展.
pub fn is_our_ext_origin(origin: &str) -> bool {
    origin
        .trim()
        .trim_end_matches('/')
        .strip_prefix("chrome-extension://")
        == Some(EXT_ID)
}

pub fn load_origins(_cfg_dir: &Path) -> Vec<String> {
    vec![EXT_ORIGIN.to_string()]
}

/// 扩展连上后重装 native host. 只接受本扩展, 避免任意扩展把自己写进白名单.
pub fn remember_origin(cfg_dir: &Path, origin: &str) -> Result<(), String> {
    if !is_our_ext_origin(origin) {
        return Err("origin 不是本扩展".into());
    }
    install_native_host(cfg_dir)
}

pub fn api_up() -> bool {
    let sock = std::net::SocketAddr::from(([127, 0, 0, 1], API_PORT));
    TcpStream::connect_timeout(&sock, Duration::from_millis(200)).is_ok()
}

fn exe_path() -> Result<PathBuf, String> {
    std::env::current_exe().map_err(|e| e.to_string())
}

/// Chrome 的 native host `path` 不能带参数.
/// Unix 写包装脚本; Windows 复制一份名为 nm-host 的 exe, 入口靠文件名进 native host 模式.
fn host_bin(cfg_dir: &Path) -> Result<PathBuf, String> {
    let exe = exe_path()?;
    #[cfg(windows)]
    {
        // 副本靠文件名进 host 模式, 必须另记 GUI 路径, 否则 spawn 会再 exec 自己
        std::fs::write(cfg_dir.join("gui-exe.txt"), exe.to_string_lossy().as_bytes())
            .map_err(|e| e.to_string())?;
        let wrap = cfg_dir.join("nm-host.exe");
        copy_if_stale(&exe, &wrap)?;
        return Ok(wrap);
    }
    #[cfg(not(windows))]
    {
        let wrap = cfg_dir.join("nm-host.sh");
        let body = format!("#!/bin/sh\nexec \"{}\" --native-host\n", exe.display());
        std::fs::write(&wrap, body).map_err(|e| e.to_string())?;
        use std::os::unix::fs::PermissionsExt;
        let mut perm = std::fs::metadata(&wrap).map_err(|e| e.to_string())?.permissions();
        perm.set_mode(0o755);
        std::fs::set_permissions(&wrap, perm).map_err(|e| e.to_string())?;
        Ok(wrap)
    }
}

#[cfg(windows)]
fn copy_if_stale(src: &Path, dst: &Path) -> Result<(), String> {
    let src_meta = std::fs::metadata(src).map_err(|e| e.to_string())?;
    let stale = match std::fs::metadata(dst) {
        Err(_) => true,
        Ok(dst_meta) => {
            dst_meta.len() != src_meta.len()
                || src_meta.modified().ok() > dst_meta.modified().ok()
        }
    };
    if stale {
        std::fs::copy(src, dst).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn host_json(cfg_dir: &Path) -> Result<Value, String> {
    let path = host_bin(cfg_dir)?;
    let path_s = path.to_string_lossy().into_owned();
    Ok(json!({
        "name": HOST_NAME,
        "description": "Wake Dash Download",
        "path": path_s,
        "type": "stdio",
        "allowed_origins": load_origins(cfg_dir),
    }))
}

fn nm_dirs() -> Result<Vec<PathBuf>, String> {
    #[cfg(target_os = "macos")]
    {
        let home = dirs::home_dir().ok_or("找不到 home 目录")?;
        let app_s = home.join("Library/Application Support");
        return Ok(vec![
            app_s.join("Google/Chrome/NativeMessagingHosts"),
            app_s.join("Google/Chrome Canary/NativeMessagingHosts"),
            app_s.join("Chromium/NativeMessagingHosts"),
            app_s.join("Microsoft Edge/NativeMessagingHosts"),
            app_s.join("BraveSoftware/Brave-Browser/NativeMessagingHosts"),
            app_s.join("Arc/NativeMessagingHosts"),
            app_s.join("Vivaldi/NativeMessagingHosts"),
        ]);
    }
    #[cfg(target_os = "linux")]
    {
        let home = dirs::home_dir().ok_or("找不到 home 目录")?;
        let cfg = home.join(".config");
        return Ok(vec![
            cfg.join("google-chrome/NativeMessagingHosts"),
            cfg.join("google-chrome-beta/NativeMessagingHosts"),
            cfg.join("chromium/NativeMessagingHosts"),
            cfg.join("microsoft-edge/NativeMessagingHosts"),
            cfg.join("BraveSoftware/Brave-Browser/NativeMessagingHosts"),
            cfg.join("vivaldi/NativeMessagingHosts"),
        ]);
    }
    #[cfg(windows)]
    {
        let cfg = dirs::config_dir().ok_or("找不到 config 目录")?;
        Ok(vec![cfg.join("dash-download")])
    }
}

pub fn install_native_host(cfg_dir: &Path) -> Result<(), String> {
    let _ = std::fs::create_dir_all(cfg_dir);
    let spec = host_json(cfg_dir)?;
    let name = format!("{HOST_NAME}.json");
    for dir in nm_dirs()? {
        let _ = std::fs::create_dir_all(&dir);
        let dest = dir.join(&name);
        std::fs::write(&dest, serde_json::to_vec_pretty(&spec).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        #[cfg(windows)]
        {
            let dest_s = dest.to_string_lossy().into_owned();
            for key in [
                r"HKCU\Software\Google\Chrome\NativeMessagingHosts\",
                r"HKCU\Software\Chromium\NativeMessagingHosts\",
                r"HKCU\Software\Microsoft\Edge\NativeMessagingHosts\",
                r"HKCU\Software\BraveSoftware\Brave-Browser\NativeMessagingHosts\",
            ] {
                let _ = std::process::Command::new("reg")
                    .args(["add", &format!("{key}{HOST_NAME}"), "/ve", "/d", &dest_s, "/f"])
                    .status();
            }
        }
    }
    Ok(())
}

/// nm-host.exe 是 GUI 的副本, 再 exec current_exe 会递归进 native host.
fn gui_exe() -> Result<PathBuf, String> {
    let exe = exe_path()?;
    let is_nm = exe
        .file_stem()
        .and_then(|s| s.to_str())
        .is_some_and(|s| s.eq_ignore_ascii_case("nm-host"));
    if !is_nm {
        return Ok(exe);
    }
    let ptr = exe
        .parent()
        .ok_or("nm-host.exe 无父目录")?
        .join("gui-exe.txt");
    let raw = std::fs::read_to_string(&ptr).map_err(|e| format!("读 gui-exe.txt 失败: {e}"))?;
    let path = PathBuf::from(raw.trim());
    if !path.is_file() {
        return Err(format!("GUI 可执行文件不存在: {}", path.display()));
    }
    Ok(path)
}

fn spawn_gui() -> Result<(), String> {
    let exe = gui_exe()?;
    #[cfg(target_os = "macos")]
    {
        if let Some(bundle) = exe.ancestors().find(|p| p.extension().map(|e| e == "app").unwrap_or(false)) {
            std::process::Command::new("open")
                .arg("-a")
                .arg(bundle)
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
                .map_err(|e| e.to_string())?;
            return Ok(());
        }
    }
    std::process::Command::new(exe)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

fn nm_read() -> Result<Value, String> {
    let mut stdin = std::io::stdin().lock();
    let mut len_buf = [0u8; 4];
    stdin.read_exact(&mut len_buf).map_err(|e| e.to_string())?;
    let len = u32::from_le_bytes(len_buf) as usize;
    if len == 0 || len > 1024 * 1024 {
        return Err("native message 长度非法".into());
    }
    let mut buf = vec![0u8; len];
    stdin.read_exact(&mut buf).map_err(|e| e.to_string())?;
    serde_json::from_slice(&buf).map_err(|e| e.to_string())
}

fn nm_write(v: &Value) -> Result<(), String> {
    let bytes = serde_json::to_vec(v).map_err(|e| e.to_string())?;
    let mut stdout = std::io::stdout().lock();
    stdout
        .write_all(&(bytes.len() as u32).to_le_bytes())
        .map_err(|e| e.to_string())?;
    stdout.write_all(&bytes).map_err(|e| e.to_string())?;
    stdout.flush().map_err(|e| e.to_string())?;
    Ok(())
}

/// Chrome stdio host: 不写 stdout 日志. 已在跑则直接 ok, 否则拉起 GUI 并等到 API 端口起来.
pub fn run_native_host() {
    let _ = nm_read();
    let reply = match wake() {
        Ok(()) => json!({ "ok": true }),
        Err(e) => json!({ "ok": false, "error": e }),
    };
    let _ = nm_write(&reply);
}

fn wake() -> Result<(), String> {
    if api_up() {
        return Ok(());
    }
    spawn_gui()?;
    let deadline = Instant::now() + Duration::from_secs(12);
    while Instant::now() < deadline {
        if api_up() {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    Err("拉起后 API 未就绪".into())
}
