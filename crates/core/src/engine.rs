use crate::bt::{self, BtCtl};
use crate::error::{CoreError, Result};
use crate::probe::{probe, sanitize};
use crate::runner::{plan_segments, replan_remaining, run_segment, run_stream, SegOutcome};
use crate::settings::{EngineSettings, ProxyCfg, ProxyKind, ProxyProbe, MAX_CONN, MAX_IMPORT_BYTES};
use crate::store::Store;
use crate::torrent::{TorrentEvent, TorrentInfo, TorrentState};
use crate::types::{
    AddTaskOptions, EngineEvent, RequestContext, SegmentInfo, TaskInfo, TaskProgress, TaskState,
};
use crate::writer::TaskFile;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::{broadcast, watch};
use tokio::task::JoinSet;

/// 构造期参数. 运行时偏好只活在 Inner.live, 避免跟 EngineSettings 再抄一份字段.
#[derive(Debug, Clone)]
pub struct EngineConfig {
    pub db_path: PathBuf,
    pub settings: EngineSettings,
    /// 低于此值不再切分
    pub min_segment_size: u64,
    /// Segment 无进度重试上限
    pub retry_limit: u32,
    pub user_agent: String,
}

impl EngineConfig {
    pub fn new(db_path: PathBuf, default_dir: PathBuf) -> Self {
        Self::with_settings(
            db_path,
            EngineSettings {
                default_dir: default_dir.to_string_lossy().into_owned(),
                ..EngineSettings::default()
            },
        )
    }

    pub fn with_settings(db_path: PathBuf, settings: EngineSettings) -> Self {
        EngineConfig {
            db_path,
            settings,
            min_segment_size: 1024 * 1024,
            retry_limit: 5,
            user_agent: format!("dash-download/{}", env!("CARGO_PKG_VERSION")),
        }
    }
}

/// 代理变更必须换 Client: reqwest 把 Proxy 编进连接池, 改字段不会影响已建池.
fn build_client(ua: &str, proxy: &ProxyCfg) -> Result<reqwest::Client> {
    proxy.validate()?;
    let mut b = reqwest::Client::builder()
        .user_agent(ua)
        .connect_timeout(Duration::from_secs(15))
        .cookie_store(true);
    match proxy.kind {
        ProxyKind::Direct => {
            b = b.no_proxy();
        }
        ProxyKind::NoProxy => {}
        ProxyKind::Http | ProxyKind::Socks5 => {
            let scheme = match proxy.kind {
                ProxyKind::Http => "http",
                ProxyKind::Socks5 => "socks5h",
                ProxyKind::Direct | ProxyKind::NoProxy => unreachable!(),
            };
            let url = format!("{}://{}:{}", scheme, proxy.host.trim(), proxy.port);
            let mut p = reqwest::Proxy::all(&url)?;
            if proxy.auth {
                p = p.basic_auth(&proxy.user, &proxy.pass);
            }
            b = b.proxy(p);
        }
    }
    Ok(b.build()?)
}

/// 运行中任务的内存句柄: 段进度用原子量共享给采样器, 避免任何进度锁
struct Running {
    cancel: watch::Sender<bool>,
    /// 区分 pause (保留断点, 状态回 Paused) 与 cancel/remove (直接退出)
    pause_intent: Arc<AtomicBool>,
    /// (idx, 已下载字节) 与 segments 顺序一致
    segs: Vec<(u32, Arc<AtomicU64>)>,
}

pub(crate) struct Inner {
    pub(crate) cfg: EngineConfig,
    pub(crate) live: Mutex<EngineSettings>,
    pub(crate) client: Mutex<reqwest::Client>,
    pub(crate) store: Mutex<Store>,
    running: Mutex<HashMap<i64, Running>>,
    speeds: Mutex<HashMap<i64, u64>>,
    events: broadcast::Sender<EngineEvent>,
    pub(crate) torrent_ev: broadcast::Sender<TorrentEvent>,
    /// Session 后台起来, 避免 UPnP/DHT 挡住窗口
    pub(crate) bt: Mutex<Option<BtCtl>>,
    /// boot 已 spawn 但 Session::new 还没返回, 防止开关连点开出两个 session
    pub(crate) bt_starting: AtomicBool,
    /// Resolve 任务的取消开关, X 掉解析层时置 true
    pub(crate) resolve_abort: Mutex<HashMap<i64, watch::Sender<bool>>>,
    /// XIU2 best 优先, ngosang 补集. 启动时内置, 随后刷新.
    pub(crate) pub_trackers: Mutex<Vec<String>>,
    /// 避免 kick_bt 重入把同一个 queued 加进 swarm 两次.
    pub(crate) bt_busy: AtomicBool,
    /// rebuild 时 +1, 过期的 Session::new 不能写回 bt.
    pub(crate) bt_gen: AtomicU64,
}

/// 引擎门面: 所有客户端 (Tauri UI / 扩展 API / CLI) 的唯一入口.
/// 必须在 tokio runtime 内创建 (内部 spawn 采样与调度协程).
#[derive(Clone)]
pub struct Engine {
    inner: Arc<Inner>,
}

impl Engine {
    pub async fn new(cfg: EngineConfig) -> Result<Engine> {
        let store = Store::open(&cfg.db_path)?;
        store.recover_interrupted()?;
        let live = cfg.settings.clone();
        live.validate()?;
        let client = build_client(&cfg.user_agent, &live.proxy)?;
        let (events, _) = broadcast::channel(512);
        let (torrent_ev, _) = broadcast::channel(512);
        let inner = Arc::new(Inner {
            cfg,
            live: Mutex::new(live.clone()),
            client: Mutex::new(client),
            store: Mutex::new(store),
            running: Mutex::new(HashMap::new()),
            speeds: Mutex::new(HashMap::new()),
            events,
            torrent_ev,
            bt: Mutex::new(None),
            bt_starting: AtomicBool::new(false),
            resolve_abort: Mutex::new(HashMap::new()),
            pub_trackers: Mutex::new(crate::trackers::baked()),
            bt_busy: AtomicBool::new(false),
            bt_gen: AtomicU64::new(0),
        });
        tracing::info!("engine http ready");
        tokio::spawn(sampler_loop(inner.clone()));
        // HTTP 缓存解析不依赖 P2P session, 重启后把半截 Resolving 捞回来
        inner.clone().resume_pending_resolve();
        bt::boot(inner.clone());
        Ok(Engine { inner })
    }

    pub fn subscribe(&self) -> broadcast::Receiver<EngineEvent> {
        self.inner.events.subscribe()
    }

    pub fn subscribe_torrents(&self) -> broadcast::Receiver<TorrentEvent> {
        self.inner.torrent_ev.subscribe()
    }

    pub fn list_torrents(&self) -> Result<Vec<TorrentInfo>> {
        let mut rows = self.inner.store.lock().unwrap().list_torrents()?;
        // Resolving 还没 Metainfo, 不进下载列表
        rows.retain(|t| t.state != TorrentState::Resolving);
        for t in &mut rows {
            self.inner.overlay_torrent(t);
        }
        Ok(rows)
    }

    pub fn torrent(&self, id: i64) -> Result<TorrentInfo> {
        self.inner.torrent_info(id)
    }

    pub fn add_magnet(&self, magnet: &str, dir: Option<String>) -> Result<TorrentInfo> {
        // 解析走 HTTP 缓存, 不要求开 P2P. DHT 回退才要 session.
        let (info, dup) = self.inner.add_magnet_row(magnet, dir)?;
        if info.state == TorrentState::Resolving {
            let live = self.inner.resolve_abort.lock().unwrap().contains_key(&info.id);
            if !dup || !live {
                self.inner
                    .clone()
                    .spawn_resolve(info.id, magnet.trim().to_string());
            }
        } else if dup {
            // 重复 Infohash 也走 TorrentAdded: UI 才能聚焦已有行 / 重开选文件.
            self.inner
                .emit_torrent(TorrentEvent::TorrentAdded { torrent: info.clone() });
        }
        Ok(info)
    }

    pub fn add_torrent_bytes(
        &self,
        bytes: &[u8],
        source: &str,
        dir: Option<String>,
    ) -> Result<TorrentInfo> {
        // .torrent 字节已是 Metainfo, 列文件不打 DHT.
        let (info, dup) = self.inner.add_bytes_row(bytes, source, dir)?;
        if dup {
            self.inner
                .emit_torrent(TorrentEvent::TorrentAdded { torrent: info.clone() });
        }
        if !dup && info.state == TorrentState::Queued {
            Inner::kick_bt(self.inner.clone());
        }
        Ok(info)
    }

    pub fn select_torrent_files(&self, id: i64, selected: Vec<u32>) -> Result<TorrentInfo> {
        let info = self.inner.select_files(id, selected)?;
        Inner::kick_bt(self.inner.clone());
        Ok(info)
    }

    pub fn pause_torrent(&self, id: i64) -> Result<()> {
        self.inner.pause_torrent(id, true)
    }

    pub fn resume_torrent(&self, id: i64) -> Result<()> {
        self.inner.require_p2p()?;
        self.inner.resume_torrent(id)?;
        Inner::kick_bt(self.inner.clone());
        Ok(())
    }

    pub async fn remove_torrent(&self, id: i64, delete_file: bool) -> Result<()> {
        self.inner.remove_torrent(id, delete_file).await?;
        Inner::kick_bt(self.inner.clone());
        Ok(())
    }

    pub async fn fetch_torrent_url(&self, url: &str, headers: &[(String, String)]) -> Result<Vec<u8>> {
        let client = self.inner.client.lock().unwrap().clone();
        let mut req = client.get(url);
        for (k, v) in headers {
            req = req.header(k.as_str(), v.as_str());
        }
        let resp = req
            .send()
            .await
            .map_err(|e| CoreError::Other(format!("拉 .torrent: {e}")))?;
        if !resp.status().is_success() {
            return Err(CoreError::Other(format!("拉 .torrent HTTP {}", resp.status())));
        }
        let bytes = resp
            .bytes()
            .await
            .map_err(|e| CoreError::Other(format!("读 .torrent: {e}")))?;
        if bytes.len() > 8 * 1024 * 1024 {
            return Err(CoreError::Other(".torrent 超过 8MB".into()));
        }
        Ok(bytes.to_vec())
    }

    pub fn list(&self) -> Result<Vec<TaskInfo>> {
        let mut tasks = self.inner.store.lock().unwrap().list_tasks()?;
        for t in &mut tasks {
            self.inner.overlay(t);
        }
        Ok(tasks)
    }

    pub fn task(&self, id: i64) -> Result<TaskInfo> {
        self.inner.task_info(id)
    }

    /// 新增任务. 只收 http/https (v1 边界), 其余协议直接拒绝
    pub fn add(&self, url: &str, opts: AddTaskOptions) -> Result<TaskInfo> {
        let parsed = url::Url::parse(url).map_err(|e| CoreError::Other(format!("URL 非法: {e}")))?;
        if !matches!(parsed.scheme(), "http" | "https") {
            return Err(CoreError::Other(format!("暂不支持 {} 协议", parsed.scheme())));
        }
        let live = self.inner.live.lock().unwrap().clone();
        let dir = opts.dir.clone().unwrap_or_else(|| live.default_dir.clone());
        let name = opts.name.clone().unwrap_or_default();
        let max_segments = opts.segments.unwrap_or(live.max_segments).clamp(1, MAX_CONN);
        let id = self.inner.store.lock().unwrap().insert_task(
            url,
            &dir,
            &name,
            TaskState::Queued,
            &opts.ctx,
            max_segments,
        )?;
        let info = self.inner.task_info(id)?;
        self.inner.emit(EngineEvent::TaskAdded { task: info.clone() });
        if !opts.queue_only {
            self.inner.clone().schedule();
        }
        Ok(info)
    }

    /// 扩展把页面 blob/data 读成字节后直写目标文件.
    /// 不能走 add(): blob: 不是 http, 引擎也无法跨进程 fetch 页面 blob URL.
    pub fn import_bytes(
        &self,
        url: &str,
        name: Option<String>,
        mime: Option<String>,
        bytes: &[u8],
    ) -> Result<TaskInfo> {
        if bytes.len() > MAX_IMPORT_BYTES {
            return Err(CoreError::Other(format!(
                "导入内容过大: {} > {MAX_IMPORT_BYTES} 字节",
                bytes.len()
            )));
        }
        let live = self.inner.live.lock().unwrap().clone();
        let dir = live.default_dir.clone();
        std::fs::create_dir_all(&dir)?;
        let name = import_name(name, mime.as_deref());
        let dest = unique_path(Path::new(&dir), &name);
        let final_name = dest
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or(name);
        std::fs::write(&dest, bytes)?;
        let size = bytes.len() as u64;
        let id = {
            let mut store = self.inner.store.lock().unwrap();
            let id = store.insert_task(
                url,
                &dir,
                &final_name,
                TaskState::Completed,
                &RequestContext::default(),
                1,
            )?;
            store.update_probe(id, url, &final_name, Some(size), false, 0, false)?;
            store.checkpoint(id, size, &[])?;
            store.set_state(id, TaskState::Completed, "")?;
            id
        };
        let info = self.inner.task_info(id)?;
        self.inner.emit(EngineEvent::TaskAdded { task: info.clone() });
        Ok(info)
    }

    pub fn pause(&self, id: i64) -> Result<()> {
        let handled = {
            let running = self.inner.running.lock().unwrap();
            if let Some(r) = running.get(&id) {
                r.pause_intent.store(true, Ordering::Relaxed);
                let _ = r.cancel.send(true);
                true
            } else {
                false
            }
        };
        if !handled {
            // 未运行 (排队中) 的任务直接落状态
            let info = self.inner.task_info(id)?;
            if info.state == TaskState::Queued {
                self.inner.set_state_emit(id, TaskState::Paused, "")?;
            }
        }
        Ok(())
    }

    /// 恢复暂停/失败/取消的任务: 回到队列由调度器按并发额度拉起.
    /// Canceled 与 Paused 一样保留 .ddown, 取消不是删任务.
    pub fn resume(&self, id: i64) -> Result<()> {
        let info = self.inner.task_info(id)?;
        if !matches!(
            info.state,
            TaskState::Paused | TaskState::Failed | TaskState::Canceled
        ) {
            return Ok(());
        }
        self.inner.set_state_emit(id, TaskState::Queued, "")?;
        self.inner.clone().schedule();
        Ok(())
    }

    pub fn pause_all(&self) -> Result<()> {
        for t in self.list()? {
            if t.state.is_running() || t.state == TaskState::Queued {
                self.pause(t.id)?;
            }
        }
        for t in self.list_torrents()? {
            if matches!(
                t.state,
                TorrentState::Active | TorrentState::Seeding | TorrentState::Queued
            ) {
                self.pause_torrent(t.id)?;
            }
        }
        Ok(())
    }

    pub fn resume_all(&self) -> Result<()> {
        for t in self.list()? {
            if matches!(t.state, TaskState::Paused | TaskState::Failed) {
                self.inner.set_state_emit(t.id, TaskState::Queued, "")?;
            }
        }
        if self.inner.live.lock().unwrap().p2p {
            for t in self.list_torrents()? {
                if matches!(t.state, TorrentState::Paused | TorrentState::Failed) {
                    self.inner.resume_torrent(t.id)?;
                }
            }
            Inner::kick_bt(self.inner.clone());
        }
        self.inner.clone().schedule();
        Ok(())
    }

    /// 调整并行度. 下载中只记偏好, 暂停/未开始时立刻按剩余字节重切分段.
    pub fn set_connections(&self, id: i64, n: u32) -> Result<()> {
        let n = n.clamp(1, MAX_CONN);
        let info = self.inner.task_info(id)?;
        self.inner.store.lock().unwrap().set_max_segments(id, n)?;
        if matches!(info.state, TaskState::Paused | TaskState::Queued | TaskState::Failed | TaskState::Canceled)
            && !info.segments.is_empty()
            && info.resumable
        {
            let segs = replan_remaining(
                &info.segments,
                n,
                self.inner.cfg.min_segment_size,
            );
            self.inner.store.lock().unwrap().replace_segments(id, &segs)?;
        }
        if let Ok(t) = self.inner.task_info(id) {
            self.inner.emit(EngineEvent::TaskUpdated { task: t });
        }
        Ok(())
    }

    /// 当前运行时偏好. UI 设置页的读模型.
    pub fn settings(&self) -> EngineSettings {
        self.inner.live.lock().unwrap().clone()
    }

    /// 用草稿代理发一次 GET. 不写 prefs, 避免「还没点应用就把坏代理生效」.
    pub async fn probe_url(&self, proxy: &ProxyCfg, url: &str) -> Result<ProxyProbe> {
        let url = url.trim();
        if url.is_empty() {
            return Err(CoreError::Other("URL 不能为空".into()));
        }
        let parsed = url::Url::parse(url).map_err(|e| CoreError::Other(format!("URL 非法: {e}")))?;
        if !matches!(parsed.scheme(), "http" | "https") {
            return Err(CoreError::Other(format!("暂不支持 {} 协议", parsed.scheme())));
        }
        let client = build_client(&self.inner.cfg.user_agent, proxy)?;
        let t0 = Instant::now();
        // 设置页探测要快失败, 跟下载 Client 的 15s connect 分开.
        let send = client.get(url).send();
        let resp = match tokio::time::timeout(Duration::from_secs(5), send).await {
            Ok(Ok(r)) => r,
            Ok(Err(e)) => return Err(e.into()),
            Err(_) => return Err(CoreError::Other("超时 5s".into())),
        };
        Ok(ProxyProbe {
            status: resp.status().as_u16(),
            ms: t0.elapsed().as_millis() as u64,
            final_url: resp.url().to_string(),
        })
    }

    /// 热更新并发 / 目录 / 代理. 已在跑的 Task 仍持有旧 Client, 暂停再续才走新代理.
    /// 不在这里 schedule: 调用方必须先落盘, 否则写盘失败无法收回已启动的任务.
    pub fn apply_settings(&self, next: EngineSettings) -> Result<EngineSettings> {
        next.validate()?;
        let client = build_client(&self.inner.cfg.user_agent, &next.proxy)?;
        let prev = self.inner.live.lock().unwrap().clone();
        *self.inner.client.lock().unwrap() = client;
        *self.inner.live.lock().unwrap() = next.clone();
        if next.p2p && !prev.p2p {
            bt::boot(self.inner.clone());
        } else if !next.p2p && prev.p2p {
            bt::shutdown(self.inner.clone());
        } else if next.p2p {
            let rebuild = prev.listen_port != next.listen_port
                || prev.upnp != next.upnp
                || prev.proxy != next.proxy;
            if rebuild {
                bt::rebuild(self.inner.clone());
            }
            Inner::kick_bt(self.inner.clone());
        }
        Ok(next)
    }

    pub fn pump_queue(&self) {
        self.inner.clone().schedule();
    }

    /// 用原链接重新下载 (NDM 的 Redownload): 清进度重新排队.
    /// 注意: 运行中的管理协程收到 cancel 后可能补写一次过期 checkpoint,
    /// 只影响短暂的显示值, 下次采样即被覆盖, 不做加锁串行化
    pub fn redownload(&self, id: i64) -> Result<()> {
        {
            let mut running = self.inner.running.lock().unwrap();
            if let Some(r) = running.remove(&id) {
                r.pause_intent.store(false, Ordering::Relaxed);
                let _ = r.cancel.send(true);
            }
        }
        let info = self.inner.task_info(id)?;
        let _ = std::fs::remove_file(info.part_path());
        self.inner.store.lock().unwrap().reset_task(id)?;
        self.inner.set_state_emit(id, TaskState::Queued, "")?;
        self.inner.clone().schedule();
        Ok(())
    }

    /// 取消下载: 停跑, 任务留在列表, .ddown 断点保留, 以后可 Resume.
    /// 与 pause 的差别只是状态落 Canceled; 清文件只走 remove.
    pub fn cancel(&self, id: i64) -> Result<()> {
        let handled = {
            let running = self.inner.running.lock().unwrap();
            if let Some(r) = running.get(&id) {
                r.pause_intent.store(false, Ordering::Relaxed);
                let _ = r.cancel.send(true);
                true
            } else {
                false
            }
        };
        if !handled {
            let info = self.inner.task_info(id)?;
            if matches!(
                info.state,
                TaskState::Queued | TaskState::Paused | TaskState::Failed
            ) {
                self.inner.set_state_emit(id, TaskState::Canceled, "")?;
            }
        }
        Ok(())
    }

    /// 删除任务. delete_file 控制是否清磁盘 (含 .ddown 与成品); 不勾选则只从表移除.
    pub fn remove(&self, id: i64, delete_file: bool) -> Result<()> {
        let info = self.inner.task_info(id)?;
        {
            let mut running = self.inner.running.lock().unwrap();
            if let Some(r) = running.remove(&id) {
                r.pause_intent.store(false, Ordering::Relaxed);
                let _ = r.cancel.send(true);
            }
        }
        self.inner.store.lock().unwrap().delete_task(id)?;
        if delete_file {
            let _ = std::fs::remove_file(info.part_path());
            let _ = std::fs::remove_file(info.final_path());
        }
        self.inner.speeds.lock().unwrap().remove(&id);
        self.inner.emit(EngineEvent::TaskRemoved { id });
        self.inner.clone().schedule();
        Ok(())
    }
}

impl Inner {
    fn emit(&self, ev: EngineEvent) {
        let _ = self.events.send(ev);
    }

    /// 内存进度覆盖 db 快照: db 里的 done 只在 checkpoint 时落盘, 实时值在原子量里
    fn overlay(&self, t: &mut TaskInfo) {
        let running = self.running.lock().unwrap();
        if let Some(r) = running.get(&t.id) {
            let mut total = 0u64;
            for (idx, done) in &r.segs {
                let d = done.load(Ordering::Relaxed);
                total += d;
                if let Some(seg) = t.segments.iter_mut().find(|s| s.idx == *idx) {
                    seg.done = d;
                }
            }
            t.done = total;
            t.speed = self.speeds.lock().unwrap().get(&t.id).copied().unwrap_or(0);
        }
    }

    fn task_info(&self, id: i64) -> Result<TaskInfo> {
        let mut t = self
            .store
            .lock()
            .unwrap()
            .get_task(id)?
            .ok_or(CoreError::NotFound(id))?;
        self.overlay(&mut t);
        Ok(t)
    }

    fn set_state_emit(&self, id: i64, state: TaskState, error: &str) -> Result<()> {
        self.store.lock().unwrap().set_state(id, state, error)?;
        if let Ok(t) = self.task_info(id) {
            self.emit(EngineEvent::TaskUpdated { task: t });
        }
        Ok(())
    }

    /// 队列调度: 只要有空闲额度就拉起最早的 Queued 任务
    fn schedule(self: Arc<Self>) {
        loop {
            let slots = {
                let running = self.running.lock().unwrap();
                let max = self.live.lock().unwrap().max_concurrent as usize;
                max.saturating_sub(running.len())
            };
            if slots == 0 {
                return;
            }
            let next = match self.store.lock().unwrap().next_queued() {
                Ok(Some(id)) => id,
                _ => return,
            };
            // 先占坑再 spawn, 防止 schedule 并发重入把同一任务拉起两次
            let (cancel_tx, _) = watch::channel(false);
            let placeholder = Running {
                cancel: cancel_tx,
                pause_intent: Arc::new(AtomicBool::new(false)),
                segs: Vec::new(),
            };
            self.running.lock().unwrap().insert(next, placeholder);
            let _ = self.store.lock().unwrap().set_state(next, TaskState::Probing, "");
            tokio::spawn(self.clone().run_task(next));
        }
    }

    /// 任务管理协程: 探测 → 规划分段 → 并发下载 → 收尾
    async fn run_task(self: Arc<Self>, id: i64) {
        if let Ok(t) = self.task_info(id) {
            self.emit(EngineEvent::TaskUpdated { task: t });
        }
        let outcome = self.clone().drive_task(id).await;
        // 统一收尾: 无论成功失败都释放句柄并推进队列
        self.running.lock().unwrap().remove(&id);
        self.speeds.lock().unwrap().remove(&id);
        if let Err(e) = outcome {
            // 只把探测失败的状态码写入诊断字段. 分段/单流 HTTP 错误不能盖掉探测结果.
            if let CoreError::ProbeHttp(st) = &e {
                let _ = self.store.lock().unwrap().save_http(id, *st, false);
            }
            let _ = self.set_state_emit(id, TaskState::Failed, &e.to_string());
        }
        self.clone().schedule();
    }

    async fn drive_task(self: Arc<Self>, id: i64) -> Result<()> {
        let (info, ctx) = {
            let store = self.store.lock().unwrap();
            let info = store.get_task(id)?.ok_or(CoreError::NotFound(id))?;
            let ctx = store.load_ctx(id)?;
            (info, ctx)
        };

        // 首跑探测; 续传 (已有分段) 跳过, 复用上次的 final_url 与分段布局
        let info = if info.segments.is_empty() {
            let http = self.client.lock().unwrap().clone();
            let p = probe(&http, &info.url, &ctx).await?;
            let name = if info.name.is_empty() { p.filename.clone() } else { info.name.clone() };
            let segs = match (p.size, p.resumable) {
                (Some(size), true) if size > 0 => {
                    plan_segments(size, info.max_segments, self.cfg.min_segment_size)
                }
                (Some(size), false) => vec![SegmentInfo { idx: 0, start: 0, end: size, done: 0 }],
                _ => vec![SegmentInfo { idx: 0, start: 0, end: 0, done: 0 }],
            };
            {
                let store = self.store.lock().unwrap();
                store.update_probe(
                    id,
                    &p.final_url,
                    &name,
                    p.size,
                    p.resumable,
                    p.http_status,
                    p.range_ignored,
                )?;
                store.replace_segments(id, &segs)?;
            }
            self.store.lock().unwrap().get_task(id)?.ok_or(CoreError::NotFound(id))?
        } else {
            info
        };

        let file = Arc::new(TaskFile::open(&info.part_path(), info.size)?);
        let (cancel_tx, cancel_rx) = watch::channel(false);
        let pause_intent = Arc::new(AtomicBool::new(false));
        let seg_handles: Vec<(u32, Arc<AtomicU64>)> = info
            .segments
            .iter()
            .map(|s| (s.idx, Arc::new(AtomicU64::new(s.done))))
            .collect();

        // 替换 schedule() 里的占坑句柄; 若期间已被 remove, 说明任务没了, 直接退出
        {
            let mut running = self.running.lock().unwrap();
            if !running.contains_key(&id) {
                return Ok(());
            }
            running.insert(
                id,
                Running {
                    cancel: cancel_tx,
                    pause_intent: pause_intent.clone(),
                    segs: seg_handles.clone(),
                },
            );
        }
        self.set_state_emit(id, TaskState::Active, "")?;

        let multi = info.resumable && info.segments.len() > 1
            || (info.resumable && info.segments.first().map(|s| s.end > 0).unwrap_or(false));
        let mut set: JoinSet<Result<SegOutcome>> = JoinSet::new();
        for (seg, (_, done)) in info.segments.iter().zip(seg_handles.iter()) {
            let client = self.client.lock().unwrap().clone();
            let url = info.final_url.clone();
            let ctx = ctx.clone();
            let file = file.clone();
            let done = done.clone();
            let rx = cancel_rx.clone();
            let retry = self.cfg.retry_limit;
            let seg = seg.clone();
            if multi {
                set.spawn(run_segment(client, url, ctx, seg, file, done, rx, retry));
            } else {
                set.spawn(run_stream(client, url, ctx, file, done, rx));
            }
        }

        let mut canceled = false;
        let mut first_err: Option<CoreError> = None;
        while let Some(res) = set.join_next().await {
            match res {
                Ok(Ok(SegOutcome::Canceled)) => canceled = true,
                Ok(Ok(SegOutcome::Complete)) => {}
                Ok(Err(e)) => {
                    // 一段失败即整体失败, 立刻叫停其余段, 保留断点
                    if first_err.is_none() {
                        first_err = Some(e);
                        let running = self.running.lock().unwrap();
                        if let Some(r) = running.get(&id) {
                            let _ = r.cancel.send(true);
                        }
                    }
                }
                Err(e) => {
                    if first_err.is_none() {
                        first_err = Some(CoreError::Other(format!("segment panic: {e}")));
                    }
                }
            }
        }

        // 落最终 checkpoint (remove 场景 db 行已删, UPDATE 为无害空操作)
        let total: u64 = seg_handles.iter().map(|(_, d)| d.load(Ordering::Relaxed)).sum();
        let seg_done: Vec<(u32, u64)> =
            seg_handles.iter().map(|(i, d)| (*i, d.load(Ordering::Relaxed))).collect();
        let _ = self.store.lock().unwrap().checkpoint(id, total, &seg_done);

        if let Some(e) = first_err {
            return Err(e);
        }
        if canceled {
            let ours = {
                let running = self.running.lock().unwrap();
                running
                    .get(&id)
                    .map(|r| Arc::ptr_eq(&r.pause_intent, &pause_intent))
                    .unwrap_or(false)
            };
            // 句柄已被 remove/redownload 换掉则不要覆盖新状态
            if ours {
                let next = if pause_intent.load(Ordering::Relaxed) {
                    TaskState::Paused
                } else {
                    TaskState::Canceled
                };
                self.set_state_emit(id, next, "")?;
            }
            return Ok(());
        }

        // 全段完成: 校验尺寸 → fsync → 去掉 .ddown 后缀 (重名自动加序号)
        if let Some(size) = info.size {
            if total < size {
                return Err(CoreError::Other(format!(
                    "数据不完整: 预期 {size} 字节, 实收 {total}"
                )));
            }
        }
        file.sync()?;
        let final_path = unique_path(Path::new(&info.dir), &info.name);
        let final_name = final_path
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| info.name.clone());
        std::fs::rename(info.part_path(), &final_path)?;
        {
            let store = self.store.lock().unwrap();
            if final_name != info.name {
                store.set_name(id, &final_name)?;
            }
        }
        self.set_state_emit(id, TaskState::Completed, "")?;
        Ok(())
    }
}

/// 进度采样循环: 500ms 广播一次快照, 2s 落一次 checkpoint.
/// 丢帧无所谓 (UI 只要最新值), 但 checkpoint 决定 crash 后的续传点
async fn sampler_loop(inner: Arc<Inner>) {
    let mut last: HashMap<i64, (u64, Instant)> = HashMap::new();
    let mut tick: u64 = 0;
    let mut interval = tokio::time::interval(Duration::from_millis(500));
    loop {
        interval.tick().await;
        tick += 1;
        let snapshots: Vec<(i64, u64, Vec<(u32, u64)>)> = {
            let running = inner.running.lock().unwrap();
            running
                .iter()
                .map(|(id, r)| {
                    let seg: Vec<(u32, u64)> =
                        r.segs.iter().map(|(i, d)| (*i, d.load(Ordering::Relaxed))).collect();
                    let total = seg.iter().map(|(_, d)| d).sum();
                    (*id, total, seg)
                })
                .collect()
        };
        let (tprog, kick_bt) = inner.sample_torrents();
        if !tprog.is_empty() {
            inner.emit_torrent(crate::torrent::TorrentEvent::TorrentProgress { torrents: tprog });
        }
        if kick_bt {
            Inner::kick_bt(inner.clone());
        }
        if snapshots.is_empty() {
            last.clear();
            continue;
        }

        let now = Instant::now();
        let mut progress = Vec::with_capacity(snapshots.len());
        for (id, total, seg) in &snapshots {
            let speed = match last.get(id) {
                Some((prev, t)) => {
                    let dt = now.duration_since(*t).as_secs_f64();
                    if dt > 0.0 && total >= prev {
                        ((*total - *prev) as f64 / dt) as u64
                    } else {
                        0
                    }
                }
                None => 0,
            };
            last.insert(*id, (*total, now));
            inner.speeds.lock().unwrap().insert(*id, speed);
            progress.push(TaskProgress {
                id: *id,
                done: *total,
                speed,
                seg_done: seg.iter().map(|(_, d)| *d).collect(),
            });
        }
        last.retain(|id, _| snapshots.iter().any(|(sid, ..)| sid == id));
        inner.emit(EngineEvent::Progress { tasks: progress });

        if tick % 4 == 0 {
            let mut store = inner.store.lock().unwrap();
            for (id, total, seg) in &snapshots {
                let _ = store.checkpoint(*id, *total, seg);
            }
        }
    }
}

fn import_name(name: Option<String>, mime: Option<&str>) -> String {
    let raw = sanitize(&name.unwrap_or_default());
    let ext = mime_ext(mime.unwrap_or(""));
    if ext.is_empty() || raw.to_ascii_lowercase().ends_with(ext) {
        return raw;
    }
    format!("{raw}{ext}")
}

fn mime_ext(mime: &str) -> &'static str {
    let main = mime.split(';').next().unwrap_or("").trim().to_ascii_lowercase();
    match main.as_str() {
        "image/png" => ".png",
        "image/jpeg" | "image/jpg" => ".jpg",
        "image/webp" => ".webp",
        "image/gif" => ".gif",
        "image/svg+xml" => ".svg",
        "image/bmp" => ".bmp",
        "application/pdf" => ".pdf",
        "application/zip" => ".zip",
        "text/plain" => ".txt",
        _ => "",
    }
}

/// 目标文件已存在时追加 " (n)" 序号, 不覆盖用户既有文件
fn unique_path(dir: &Path, name: &str) -> PathBuf {
    let candidate = dir.join(name);
    if !candidate.exists() {
        return candidate;
    }
    let (stem, ext) = match name.rfind('.') {
        Some(i) if i > 0 => (&name[..i], &name[i..]),
        _ => (name, ""),
    };
    for n in 1..1000 {
        let p = dir.join(format!("{stem} ({n}){ext}"));
        if !p.exists() {
            return p;
        }
    }
    dir.join(format!("{stem} ({}){ext}", std::process::id()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    async fn tmp_engine() -> Engine {
        let n = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let seq = std::sync::atomic::AtomicU64::new(0);
        // 并行单测不能共享同一个 sqlite 文件, pid+ns 仍可能撞, 再加地址
        let dir = std::env::temp_dir().join(format!(
            "dd-import-{}-{}-{:p}",
            std::process::id(),
            n,
            &seq as *const _
        ));
        std::fs::create_dir_all(&dir).unwrap();
        Engine::new(EngineConfig::new(dir.join("t.db"), dir.join("dl")))
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn import_bytes_writes_completed_file() {
        let eng = tmp_engine().await;
        let task = eng
            .import_bytes(
                "blob:https://gemini.google.com/abc",
                Some("logo.png".into()),
                Some("image/png".into()),
                b"\x89PNG",
            )
            .unwrap();
        assert_eq!(task.state, TaskState::Completed);
        assert_eq!(task.size, Some(4));
        assert_eq!(task.done, 4);
        let path = Path::new(&task.dir).join(&task.name);
        assert_eq!(std::fs::read(path).unwrap(), b"\x89PNG");
        assert_eq!(task.name, "logo.png");
    }

    #[tokio::test]
    async fn import_bytes_fills_ext_from_mime() {
        let eng = tmp_engine().await;
        let task = eng
            .import_bytes(
                "blob:https://example/x",
                None,
                Some("image/webp".into()),
                b"RIFF",
            )
            .unwrap();
        assert_eq!(task.name, "download.webp");
        assert_eq!(task.state, TaskState::Completed);
    }

    #[tokio::test]
    async fn import_bytes_rejects_over_max() {
        let eng = tmp_engine().await;
        let bytes = vec![0u8; MAX_IMPORT_BYTES + 1];
        let err = eng
            .import_bytes("blob:https://example/x", None, None, &bytes)
            .unwrap_err();
        assert!(err.to_string().contains("过大"));
    }

    #[tokio::test]
    async fn should_not_add_blob_url() {
        let eng = tmp_engine().await;
        let err = eng.add("blob:https://x/y", AddTaskOptions::default()).unwrap_err();
        assert!(err.to_string().contains("blob"));
    }
}
