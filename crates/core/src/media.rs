//! Manifest downloads use isolated yt-dlp/FFmpeg processes. No shell or user config.
use crate::{CoreError, ProxyCfg, ProxyKind, RequestContext, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    path::{Path, PathBuf},
    process::Stdio,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex, OnceLock,
    },
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, BufReader},
    process::Command,
    sync::watch,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaOptions {
    #[serde(default = "best")]
    pub format: String,
    #[serde(default)]
    pub container: Container,
}
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Container {
    #[default]
    Mp4,
    Mkv,
}
impl Container {
    pub fn extension(self) -> &'static str {
        match self {
            Self::Mp4 => "mp4",
            Self::Mkv => "mkv",
        }
    }
}

// Keep a few short-lived probe results in memory, scoped to URL, credentials and proxy.
// Reuse the actual extractor result for downloading, not just the quality labels.
type ProbeEntries = Vec<(String, Instant, Arc<Value>)>;
static PROBES: OnceLock<Mutex<ProbeEntries>> = OnceLock::new();
fn probe_key(url: &str, ctx: &RequestContext, proxy: &ProxyCfg) -> String {
    serde_json::to_string(&(url, &ctx.headers, proxy)).unwrap()
}
fn cached_probe(key: &str) -> Option<Arc<Value>> {
    let mut entries = PROBES.get_or_init(Default::default).lock().unwrap();
    entries.retain(|(_, at, _)| at.elapsed() < Duration::from_secs(30));
    entries
        .iter()
        .find(|(k, _, _)| k == key)
        .map(|(_, _, v)| v.clone())
}
fn save_probe(key: String, value: Arc<Value>) {
    let mut entries = PROBES.get_or_init(Default::default).lock().unwrap();
    entries.retain(|(k, at, _)| k != &key && at.elapsed() < Duration::from_secs(30));
    if entries.len() >= 4 {
        entries.remove(0);
    }
    entries.push((key, Instant::now(), value));
}
fn best() -> String {
    "bestvideo+bestaudio/best".into()
}

pub fn tool(name: &str) -> PathBuf {
    let filename = format!("{name}{}", std::env::consts::EXE_SUFFIX);
    let mut dirs = Vec::new();
    if let Some(dir) = std::env::var_os("DD_MEDIA_TOOLS") {
        dirs.push(PathBuf::from(dir));
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            dirs.push(dir.join("media-tools"));
            dirs.push(dir.join("../Resources/media-tools"));
            dirs.push(dir.join("../lib/dash-download/media-tools"));
        }
    }
    #[cfg(debug_assertions)]
    dirs.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../app/media-tools"));
    for dir in dirs {
        let p = dir.join(&filename);
        if p.is_file() {
            return p;
        }
    }
    PathBuf::from(filename)
}

pub fn validate_format(value: &str) -> Result<()> {
    if value == best()
        || (!value.is_empty()
            && value.len() < 200
            && value
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "_.-+/:".contains(c)))
    {
        return Ok(());
    }
    Err(CoreError::Other("无效的视频清晰度".into()))
}

struct Secrets(PathBuf);
impl Drop for Secrets {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn config(url: &str, ctx: &RequestContext, proxy: &ProxyCfg) -> Result<Secrets> {
    let parsed = url::Url::parse(url).map_err(|e| CoreError::Other(e.to_string()))?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(CoreError::Other("视频仅支持 HTTP/HTTPS".into()));
    }
    let dir = std::env::temp_dir().join(format!(
        "dd-media-{}-{:016x}",
        std::process::id(),
        rand::random::<u64>()
    ));
    std::fs::create_dir(&dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700))?;
    }
    let secrets = Secrets(dir);
    proxy.validate()?;
    let mut conf = String::new();
    let proxy_url = match proxy.kind {
        ProxyKind::Direct => Some(String::new()),
        ProxyKind::NoProxy => None,
        ProxyKind::Socks5 => Some(proxy.socks5_url()),
        ProxyKind::Http => {
            let mut u = url::Url::parse(&format!("http://{}:{}", proxy.host, proxy.port))
                .map_err(|_| CoreError::Other("代理地址无效".into()))?;
            if proxy.auth {
                let _ = u.set_username(&proxy.user);
                let _ = u.set_password(Some(&proxy.pass));
            }
            Some(u.to_string())
        }
    };
    if let Some(proxy_url) = proxy_url {
        conf.push_str(&format!(
            "--proxy {}\n",
            serde_json::to_string(&proxy_url).unwrap()
        ));
    }
    let mut cookies = String::from("# Netscape HTTP Cookie File\n");
    for (key, value) in &ctx.headers {
        if key.contains(['\r', '\n']) || value.contains(['\r', '\n']) {
            return Err(CoreError::Other("非法请求头".into()));
        }
        if key.eq_ignore_ascii_case("cookie") {
            for entry in value.split(';') {
                if let Some((k, v)) = entry.trim().split_once('=') {
                    if !k.contains('\t') && !v.contains('\t') {
                        cookies.push_str(&format!(
                            "{}\tFALSE\t/\t{}\t0\t{}\t{}\n",
                            parsed.host_str().unwrap_or(""),
                            if parsed.scheme() == "https" {
                                "TRUE"
                            } else {
                                "FALSE"
                            },
                            k,
                            v
                        ));
                    }
                }
            }
        } else if ["referer", "user-agent", "origin"]
            .iter()
            .any(|k| key.eq_ignore_ascii_case(k))
        {
            conf.push_str(&format!(
                "--add-headers {}\n",
                serde_json::to_string(&format!("{key}:{value}")).unwrap()
            ));
        }
    }
    std::fs::write(secrets.0.join("cookies.txt"), cookies)?;
    conf.push_str(&format!(
        "--cookies {}\n",
        serde_json::to_string(&secrets.0.join("cookies.txt").to_string_lossy()).unwrap()
    ));
    std::fs::write(secrets.0.join("config"), conf)?;
    Ok(secrets)
}
fn command(secret: &Secrets) -> Command {
    let mut c = Command::new(tool("yt-dlp"));
    c.args([
        "--ignore-config",
        "--no-plugin-dirs",
        "--no-cache-dir",
        "--no-playlist",
        "--no-warnings",
        "--socket-timeout",
        "20",
        "--retries",
        "3",
        "--config-locations",
    ])
    .arg(secret.0.join("config"))
    .stdin(Stdio::null())
    .stdout(Stdio::piped())
    .stderr(Stdio::null())
    .kill_on_drop(true);
    #[cfg(unix)]
    c.process_group(0);
    #[cfg(windows)]
    c.creation_flags(0x08000000);
    c
}
// Also terminate descendants when a future is dropped (e.g. pause during inspection).
struct ProcessGroup(Option<u32>);
impl Drop for ProcessGroup {
    fn drop(&mut self) {
        #[cfg(unix)]
        if let Some(id) = self.0 {
            unsafe {
                libc::kill(-(id as i32), libc::SIGKILL);
            }
        }
        #[cfg(windows)]
        if let Some(id) = self.0 {
            use std::os::windows::process::CommandExt;
            let _ = std::process::Command::new("taskkill")
                .args(["/PID", &id.to_string(), "/T", "/F"])
                .creation_flags(0x08000000)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn();
        }
    }
}
async fn stop(child: &mut tokio::process::Child) {
    #[cfg(unix)]
    if let Some(id) = child.id() {
        unsafe {
            libc::kill(-(id as i32), libc::SIGKILL);
        }
    }
    #[cfg(windows)]
    if let Some(id) = child.id() {
        let _ = Command::new("taskkill")
            .args(["/PID", &id.to_string(), "/T", "/F"])
            .creation_flags(0x08000000)
            .output()
            .await;
    }
    let _ = child.kill().await;
    let _ = child.wait().await;
}
fn unavailable(e: std::io::Error) -> CoreError {
    CoreError::Other(format!(
        "媒体工具启动失败，请重新安装完整应用（开发环境运行 scripts/prepare-media-tools.py）: {e}"
    ))
}

pub async fn inspect(url: &str, ctx: &RequestContext) -> Result<Value> {
    inspect_with_proxy(url, ctx, &ProxyCfg::default()).await
}

pub async fn inspect_with_proxy(
    url: &str,
    ctx: &RequestContext,
    proxy: &ProxyCfg,
) -> Result<Value> {
    let value = probe_metadata(url, ctx, proxy).await?;
    summarize(&value)
}

async fn probe_metadata(url: &str, ctx: &RequestContext, proxy: &ProxyCfg) -> Result<Arc<Value>> {
    let key = probe_key(url, ctx, proxy);
    if let Some(value) = cached_probe(&key) {
        return Ok(value);
    }
    let secret = config(url, ctx, proxy)?;
    let mut child = command(&secret)
        .args(["--dump-single-json", "--skip-download", "--", url])
        .spawn()
        .map_err(unavailable)?;
    let mut group = ProcessGroup(child.id());
    let stdout = child.stdout.take().unwrap();
    let read = async {
        let mut data = Vec::new();
        stdout
            .take(8 * 1024 * 1024 + 1)
            .read_to_end(&mut data)
            .await?;
        if data.len() > 8 * 1024 * 1024 {
            return Err(CoreError::Other("视频清单过大".into()));
        }
        let status = child.wait().await?;
        group.0 = None;
        if !status.success() {
            return Err(CoreError::Other(
                "视频解析失败：资源可能已过期、受保护或需要重新播放后嗅探".into(),
            ));
        }
        let value: Value = serde_json::from_slice(&data)
            .map_err(|_| CoreError::Other("视频解析结果无效".into()))?;
        summarize(&value)?;
        let value = Arc::new(value);
        save_probe(key, value.clone());
        Ok(value)
    };
    match tokio::time::timeout(Duration::from_secs(60), read).await {
        Ok(result) => {
            if result.is_err() {
                stop(&mut child).await;
            }
            result
        }
        Err(_) => {
            stop(&mut child).await;
            Err(CoreError::Other("视频解析超时，请重试".into()))
        }
    }
}
fn summarize(v: &Value) -> Result<Value> {
    if v["is_live"] == true || v["live_status"] == "is_live" || v["live_status"] == "is_upcoming" {
        return Err(CoreError::Other("暂不支持直播，仅支持点播视频".into()));
    }
    if v["has_drm"] == true {
        return Err(CoreError::Other("暂不支持 DRM 保护的视频".into()));
    }
    let mut formats: Vec<Value> = v["formats"].as_array().into_iter().flatten()
        .filter(|f| f["has_drm"] != true && f["vcodec"].as_str().is_some_and(|s| s != "none") && f["format_id"].is_string())
        .map(|f| {
            let id = f["format_id"].as_str().unwrap();
            let format = if f["acodec"] == "none" { format!("{id}+bestaudio/{id}") } else { id.to_string() };
            json!({"format":format,"height":f["height"],"label":f["format_note"],"ext":f["ext"],"bitrate":f["tbr"]})
        }).filter(|f| validate_format(f["format"].as_str().unwrap()).is_ok()).collect();
    formats.sort_by(|a, b| {
        b["height"]
            .as_u64()
            .unwrap_or(0)
            .cmp(&a["height"].as_u64().unwrap_or(0))
            .then_with(|| {
                b["bitrate"]
                    .as_f64()
                    .unwrap_or(0.0)
                    .total_cmp(&a["bitrate"].as_f64().unwrap_or(0.0))
            })
    });
    if formats.is_empty() {
        return Err(CoreError::Other("未找到可下载的无 DRM 视频轨道".into()));
    }
    Ok(json!({"title":v["title"],"formats":formats}))
}

/// Returns false when paused/canceled. yt-dlp keeps native fragment checkpoints.
pub async fn download(
    url: &str,
    ctx: &RequestContext,
    options: &MediaOptions,
    proxy: &ProxyCfg,
    work: &Path,
    done: Arc<AtomicU64>,
    mut cancel: watch::Receiver<bool>,
) -> Result<Option<PathBuf>> {
    if *cancel.borrow() { return Ok(None); }
    std::fs::create_dir_all(work)?;
    // A marker detects removal even if yt-dlp recreates the directory before the next tick.
    let marker = tempfile::Builder::new().prefix(".dd-active-").tempfile_in(work)?;
    let (stop, rx) = watch::channel(false);
    let download = download_inner(url, ctx, options, proxy, work, done, rx);
    tokio::pin!(download);
    let mut tick = tokio::time::interval(Duration::from_millis(200));
    loop {
        tokio::select! {
            biased;
            _ = cancel.changed() => {
                stop.send_replace(true);
                return download.await;
            }
            _ = tick.tick() => {
                if !marker.path().exists() {
                    stop.send_replace(true);
                    // Join the subprocess cleanup before reporting that the task stopped.
                    let _ = download.await;
                    let _ = std::fs::remove_dir_all(work);
                    return Err(CoreError::Other("下载中的临时文件已被外部删除，下载已停止；恢复将重新下载".into()));
                }
            }
            result = &mut download => return result,
        }
    }
}

async fn download_inner(
    url: &str,
    ctx: &RequestContext,
    options: &MediaOptions,
    proxy: &ProxyCfg,
    work: &Path,
    done: Arc<AtomicU64>,
    mut cancel: watch::Receiver<bool>,
) -> Result<Option<PathBuf>> {
    if *cancel.borrow() {
        return Ok(None);
    }
    validate_format(&options.format)?;
    let metadata = tokio::select! {
        r = probe_metadata(url, ctx, proxy) => r?,
        _ = cancel.changed() => return Ok(None),
    };
    let inspected = summarize(&metadata)?;
    if options.format != best()
        && !inspected["formats"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f["format"] == options.format)
    {
        return Err(CoreError::Other("所选清晰度已失效，请重新嗅探".into()));
    }
    std::fs::create_dir_all(work)?;
    let secret = config(url, ctx, proxy)?;
    let ffmpeg = tool("ffmpeg");
    let mut check = Command::new(&ffmpeg);
    check
        .arg("-version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    #[cfg(windows)]
    check.creation_flags(0x08000000);
    if !check.status().await.map_err(unavailable)?.success() {
        return Err(CoreError::Other("FFmpeg 不可用".into()));
    }
    let metadata_path = secret.0.join("info.json");
    std::fs::write(
        &metadata_path,
        serde_json::to_vec(metadata.as_ref()).map_err(|e| CoreError::Other(e.to_string()))?,
    )?;
    let extension = options.container.extension();
    let final_path = work.join(format!("video.{extension}"));
    let output = work.join("video.%(ext)s");
    let mut cmd = command(&secret);
    if options.container == Container::Mp4 {
        cmd.args([
            "--postprocessor-args",
            "Merger+ffmpeg_o:-movflags +faststart",
            "--postprocessor-args",
            "VideoRemuxer+ffmpeg_o:-movflags +faststart",
        ]);
    }
    let mut child = cmd
        .args([
            "--newline",
            "--progress",
            "--no-simulate",
            "--no-overwrites",
            "--continue",
            "--abort-on-unavailable-fragments",
            "--match-filter",
            "!is_live & !has_drm",
            "--format",
            &options.format,
            "--merge-output-format",
            extension,
            "--remux-video",
            extension,
            "--ffmpeg-location",
        ])
        .arg(ffmpeg)
        .args([
            "--progress-template",
            "download:DD:%(info.format_id)s:%(progress.downloaded_bytes)s",
            "--output",
        ])
        .arg(output)
        .arg("--load-info-json")
        .arg(&metadata_path)
        .spawn()
        .map_err(unavailable)?;
    let mut group = ProcessGroup(child.id());
    let mut totals = std::collections::HashMap::new();
    let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
    loop {
        tokio::select! {
            _ = cancel.changed() => { stop(&mut child).await; let _ = std::fs::remove_file(&final_path); return Ok(None); }
            line = lines.next_line() => match line? {
                None => break,
                Some(line) => if let Some((format, bytes)) = line.strip_prefix("DD:").and_then(|s| s.rsplit_once(':')) {
                    if let Ok(n) = bytes.parse::<u64>() { totals.insert(format.to_owned(), n); done.store(totals.values().sum(), Ordering::Relaxed); }
                },
            }
        }
    }
    let status = tokio::select! {
        status = child.wait() => status?,
        _ = cancel.changed() => { stop(&mut child).await; let _ = std::fs::remove_file(&final_path); return Ok(None); }
    };
    group.0 = None;
    if !status.success() {
        return Err(CoreError::Other(
            if options.container == Container::Mp4 {
                "视频下载或 MP4 合并失败；可重新嗅探后重试，编码不兼容时选择 MKV"
            } else {
                "视频下载或合并失败，请重新播放页面刷新资源后重试"
            }
            .into(),
        ));
    }
    let path = final_path;
    if !path.is_file() || std::fs::metadata(&path)?.len() == 0 {
        return Err(CoreError::Other("未生成完整视频文件".into()));
    }
    done.store(std::fs::metadata(&path)?.len(), Ordering::Relaxed);
    Ok(Some(path))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn container_defaults_to_mp4_and_rejects_unknown_values() {
        let options: MediaOptions = serde_json::from_value(json!({})).unwrap();
        assert_eq!(options.container, Container::Mp4);
        assert!(serde_json::from_value::<MediaOptions>(json!({"container":"exe"})).is_err());
    }
    #[test]
    fn probe_cache_is_scoped_to_credentials_and_expires() {
        let url = "https://cache-test.invalid/master.m3u8";
        let proxy = ProxyCfg::default();
        let first = RequestContext {
            headers: vec![("Cookie".into(), "session=a".into())],
        };
        let second = RequestContext {
            headers: vec![("Cookie".into(), "session=b".into())],
        };
        let key = probe_key(url, &first, &proxy);
        save_probe(key.clone(), Arc::new(json!({"cached":true})));
        assert!(cached_probe(&key).is_some());
        assert!(cached_probe(&probe_key(url, &second, &proxy)).is_none());
        {
            let mut entries = PROBES.get().unwrap().lock().unwrap();
            entries.iter_mut().find(|(k, _, _)| k == &key).unwrap().1 =
                Instant::now() - Duration::from_secs(31);
        }
        assert!(cached_probe(&key).is_none());
    }
    #[test]
    fn rejects_live_and_drm() {
        assert!(summarize(&json!({"is_live":true})).is_err());
        assert!(summarize(&json!({"has_drm":true})).is_err());
    }
    #[test]
    fn sorts_quality_and_pairs_audio() {
        let v = summarize(&json!({"formats":[
            {"format_id":"low","vcodec":"h264","acodec":"aac","height":360},
            {"format_id":"high","vcodec":"h264","acodec":"none","height":1080},
            {"format_id":"audio","vcodec":"none"},
            {"format_id":"drm","vcodec":"h264","has_drm":true}
        ]}))
        .unwrap();
        assert_eq!(v["formats"].as_array().unwrap().len(), 2);
        assert_eq!(v["formats"][0]["format"], "high+bestaudio/high");
    }
}
