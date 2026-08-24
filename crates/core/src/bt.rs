//! librqbit Session 适配: SQLite 是 Torrent 身份, Session 只跑 Piece/Peer.
//! 不把 HTTP Task 语义塞进这里 (ADR 0008).

use crate::bt_meta::{fetch_meta_http, DHT_BOOTSTRAP};
use crate::engine::Inner;
use crate::error::{CoreError, Result};
use crate::settings::{EngineSettings, ProxyKind};
use crate::torrent::{
    bt_out_dir, files_from_list, infohash_from_magnet, meta_from_bytes, rel_path_ok, safe_join,
    sel_size, TorrentEvent, TorrentFile, TorrentInfo, TorrentPeer, TorrentState,
};
use crate::trackers;
use librqbit::{
    AddTorrent, AddTorrentOptions, AddTorrentResponse, ConnectionOptions, DhtSessionConfig,
    ListenerMode, ListenerOptions, ManagedTorrent, Session, SessionOptions, TorrentStatsState,
};
use std::collections::{HashMap, HashSet};
use std::net::{Ipv6Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::watch;


#[derive(Clone)]
pub(crate) struct BtSample {
    pub(crate) down: u64,
    pub(crate) up: u64,
    pub(crate) peers: u32,
    pub(crate) seen: u32,
    pub(crate) connecting: u32,
    pub(crate) done: u64,
    pub(crate) phase: String,
    pub(crate) peer_list: Vec<TorrentPeer>,
}

pub(crate) struct BtCtl {
    pub session: Arc<Session>,
    pub handles: Mutex<HashMap<i64, Arc<ManagedTorrent>>>,
    pub speeds: Mutex<HashMap<i64, BtSample>>,
}

pub(crate) async fn start_session(
    cfg: &EngineSettings,
    dht_file: Option<PathBuf>,
) -> Result<Arc<Session>> {
    let mut listen = ListenerOptions::default();
    listen.listen_addr = SocketAddr::from((Ipv6Addr::UNSPECIFIED, cfg.listen_port));
    listen.enable_upnp_port_forwarding = cfg.upnp;
    // TCP + uTP: 国内 BitComet 很多只肯走 uTP / 加密, 多一条出站
    listen.mode = ListenerMode::TcpAndUtp;
    listen.announce_port = if cfg.listen_port == 0 {
        None
    } else {
        Some(cfg.listen_port)
    };

    let mut connect = ConnectionOptions::default();
    if cfg.proxy.kind == ProxyKind::Socks5 && !cfg.proxy.host.trim().is_empty() {
        connect.proxy_url = Some(cfg.proxy.socks5_url());
    }

    // 公共 tracker 按种子附加, 不进 Session: 否则 private=1 / 关掉 extra_trackers 仍会 announce.

    let dht_persist = dht_file.map(|config_filename| librqbit::dht::DhtPersistenceConfig {
        dump_interval: None,
        config_filename: Some(config_filename),
    });
    let opts = SessionOptions {
        listen: Some(listen),
        connect: Some(connect),
        fastresume: true,
        trackers: HashSet::new(),
        dht: Some(DhtSessionConfig {
            bootstrap_addrs: Some(DHT_BOOTSTRAP.iter().map(|s| (*s).to_string()).collect()),
            port: None,
            persistence: dht_persist,
        }),
        client_name_and_version: Some(format!("dash-download/{}", env!("CARGO_PKG_VERSION"))),
        disable_local_service_discovery: false,
        ..Default::default()
    };
    tracing::debug!(
        port = cfg.listen_port,
        upnp = cfg.upnp,
        socks = cfg.proxy.kind == ProxyKind::Socks5,
        "启动 BT session"
    );
    Session::new_with_opts(PathBuf::from(&cfg.default_dir), opts)
        .await
        .map_err(|e| CoreError::Other(format!("BT session: {e:#}")))
}

/// 用户打开 P2P 才启 DHT / 监听. 关着时不准对外打流量.
pub(crate) fn boot(inner: Arc<Inner>) {
    if !inner.live.lock().unwrap().p2p {
        tracing::info!("P2P 关闭, 不启 BT session");
        return;
    }
    if inner.bt.lock().unwrap().is_some() {
        Inner::kick_bt(inner);
        return;
    }
    if inner.bt_starting.swap(true, Ordering::SeqCst) {
        return;
    }
    let gen = inner.bt_gen.load(Ordering::SeqCst);
    tokio::spawn(async move {
        // 刷新跟 session 并行: 内置列表已经能 announce, 不必空等 CDN
        {
            let inner = inner.clone();
            tokio::spawn(async move {
                let client = inner.client.lock().unwrap().clone();
                if let Some(list) = crate::trackers::refresh(&client).await {
                    *inner.pub_trackers.lock().unwrap() = list;
                }
            });
        }
        loop {
            if inner.bt_gen.load(Ordering::SeqCst) != gen {
                inner.bt_starting.store(false, Ordering::SeqCst);
                return;
            }
            if !inner.live.lock().unwrap().p2p {
                inner.bt_starting.store(false, Ordering::SeqCst);
                return;
            }
            let cfg = inner.live.lock().unwrap().clone();
            let dht_file = inner.cfg.db_path.parent().map(|p| p.join("dht.json"));
            match start_session(&cfg, dht_file).await {
                Ok(session) => {
                    if inner.bt_gen.load(Ordering::SeqCst) != gen {
                        tracing::info!("丢弃过期 BT session");
                        inner.bt_starting.store(false, Ordering::SeqCst);
                        return;
                    }
                    if !inner.live.lock().unwrap().p2p {
                        tracing::info!("P2P 已关闭, 丢弃刚起来的 session");
                        inner.bt_starting.store(false, Ordering::SeqCst);
                        return;
                    }
                    if let Some(addr) = session.listen_addr() {
                        inner.live.lock().unwrap().listen_port = addr.port();
                    }
                    *inner.bt.lock().unwrap() = Some(BtCtl {
                        session,
                        handles: Mutex::new(HashMap::new()),
                        speeds: Mutex::new(HashMap::new()),
                    });
                    inner.bt_starting.store(false, Ordering::SeqCst);
                    tracing::info!(
                        listen = inner.live.lock().unwrap().listen_port,
                        "BT session ready"
                    );
                    Inner::kick_bt(inner);
                    return;
                }
                Err(e) => {
                    tracing::error!("BT session 启动失败: {e}");
                    tokio::time::sleep(Duration::from_secs(3)).await;
                }
            }
        }
    });
}

/// 关掉 P2P: 停解析, Active/Seeding 改暂停, 丢掉 session 让 DHT/入站立刻停.
pub(crate) fn shutdown(inner: Arc<Inner>) {
    tokio::spawn(async move {
        let resolving: Vec<i64> = inner
            .store
            .lock()
            .unwrap()
            .list_resolving()
            .unwrap_or_default();
        for id in resolving {
            inner.abort_resolve(id);
        }
        let live: Vec<i64> = inner
            .store
            .lock()
            .unwrap()
            .list_torrents()
            .unwrap_or_default()
            .into_iter()
            .filter(|t| matches!(t.state, TorrentState::Active | TorrentState::Seeding))
            .map(|t| t.id)
            .collect();
        for id in live {
            let _ = inner.set_tstate(id, TorrentState::Paused, "");
        }
        let dropped = inner.bt.lock().unwrap().take();
        inner.bt_starting.store(false, Ordering::SeqCst);
        if dropped.is_some() {
            tracing::info!("P2P 已关闭, BT session 停止");
        }
    });
}

/// listen/代理热改: 丢掉旧 session 再 boot. gen 让还在 `Session::new` 的旧任务自动作废.
pub(crate) fn rebuild(inner: Arc<Inner>) {
    inner.bt_gen.fetch_add(1, Ordering::SeqCst);
    inner.bt_starting.store(false, Ordering::SeqCst);
    let dropped = inner.bt.lock().unwrap().take();
    drop(dropped);
    boot(inner);
}

impl Inner {
    fn bt_session(&self) -> Option<Arc<Session>> {
        self.bt.lock().unwrap().as_ref().map(|b| b.session.clone())
    }

    /// DHT resolve / 出队都要 session. 起 UPnP 期间不能立刻删行.
    async fn wait_bt_session(&self, deadline: Instant) -> Result<Arc<Session>> {
        loop {
            if let Some(s) = self.bt_session() {
                return Ok(s);
            }
            if !self.live.lock().unwrap().p2p {
                return Err(CoreError::Other("P2P 已关闭".into()));
            }
            if Instant::now() >= deadline {
                return Err(CoreError::Other("BT session 启动超时".into()));
            }
            tokio::time::sleep(Duration::from_millis(150)).await;
        }
    }

    fn handle_of(&self, id: i64) -> Option<(Arc<Session>, Arc<ManagedTorrent>)> {
        let g = self.bt.lock().unwrap();
        let bt = g.as_ref()?;
        let h = bt.handles.lock().unwrap().get(&id).cloned()?;
        Some((bt.session.clone(), h))
    }

    fn existing_hash(&self, hash: &str, extra: &[String]) -> Result<Option<TorrentInfo>> {
        let id = {
            let store = self.store.lock().unwrap();
            match store.torrent_by_hash(hash)? {
                Some(exist) => {
                    store.merge_trackers(exist.id, extra)?;
                    Some(exist.id)
                }
                None => None,
            }
        };
        id.map(|id| self.torrent_info(id)).transpose()
    }

    pub(crate) fn require_p2p(&self) -> Result<()> {
        if self.live.lock().unwrap().p2p {
            Ok(())
        } else {
            Err(CoreError::Other("P2P 未开启, 请在设置中打开".into()))
        }
    }

    fn merge_pub_trackers(&self, dst: &mut Vec<String>) {
        trackers::merge(dst, &self.pub_trackers.lock().unwrap());
    }

    pub(crate) fn emit_torrent(&self, ev: TorrentEvent) {
        let _ = self.torrent_ev.send(ev);
    }

    pub(crate) fn torrent_info(&self, id: i64) -> Result<TorrentInfo> {
        let mut t = self
            .store
            .lock()
            .unwrap()
            .get_torrent(id)?
            .ok_or(CoreError::NotFound(id))?;
        self.overlay_torrent(&mut t);
        Ok(t)
    }

    pub(crate) fn overlay_torrent(&self, t: &mut TorrentInfo) {
        t.bt_direct = self.live.lock().unwrap().bt_direct();
        if let Some(s) = self.bt.lock().unwrap().as_ref().and_then(|b| {
            b.speeds.lock().unwrap().get(&t.id).cloned()
        }) {
            t.speed = s.down;
            t.up_speed = s.up;
            t.peers = s.peers;
            t.seen = s.seen;
            t.connecting = s.connecting;
            t.done = s.done;
            t.phase = s.phase;
            t.peer_list = s.peer_list;
        }
    }

    pub(crate) fn set_tstate(&self, id: i64, state: TorrentState, error: &str) -> Result<()> {
        self.store
            .lock()
            .unwrap()
            .set_torrent_state(id, state, error)?;
        if let Ok(t) = self.torrent_info(id) {
            self.emit_torrent(TorrentEvent::TorrentUpdated { torrent: t });
        }
        Ok(())
    }

    /// magnet / 已有 infohash 入表. 重复 Infohash 聚焦已有行并把 tracker 并入.
    pub(crate) fn add_magnet_row(
        &self,
        magnet: &str,
        dir: Option<String>,
    ) -> Result<(TorrentInfo, bool)> {
        let magnet = magnet.trim();
        let live = self.live.lock().unwrap().clone();
        let dir = dir.unwrap_or_else(|| live.default_dir.clone());
        let hash = infohash_from_magnet(magnet)
            .ok_or_else(|| CoreError::Other("磁力链接没有 v1 Infohash".into()))?;
        let trackers = librqbit::Magnet::parse(magnet)
            .ok()
            .map(|m| m.trackers)
            .unwrap_or_default();
        if let Some(info) = self.existing_hash(&hash, &trackers)? {
            if info.state == TorrentState::Resolving {
                self.emit_torrent(TorrentEvent::Resolving { torrent: info.clone() });
            }
            return Ok((info, true));
        }
        let name = librqbit::Magnet::parse(magnet)
            .ok()
            .and_then(|m| m.name)
            .unwrap_or_else(|| hash[..8.min(hash.len())].to_string());
        let id = self.store.lock().unwrap().insert_torrent(
            &hash,
            magnet,
            &name,
            &dir,
            TorrentState::Resolving,
            &trackers,
        )?;
        tracing::debug!(id, hash = %hash, "magnet 入队 Resolve, 列表暂不展示");
        let info = self.torrent_info(id)?;
        self.emit_torrent(TorrentEvent::Resolving {
            torrent: info.clone(),
        });
        Ok((info, false))
    }

    pub(crate) fn add_bytes_row(
        &self,
        bytes: &[u8],
        source: &str,
        dir: Option<String>,
    ) -> Result<(TorrentInfo, bool)> {
        let live = self.live.lock().unwrap().clone();
        let dir = dir.unwrap_or_else(|| live.default_dir.clone());
        let meta = meta_from_bytes(bytes)?;
        let mut trackers = meta.trackers;
        if live.extra_trackers && !meta.private {
            self.merge_pub_trackers(&mut trackers);
        }
        if let Some(info) = self.existing_hash(&meta.hash, &trackers)? {
            return Ok((info, true));
        }
        let files = meta.files;
        let name = meta.name;
        let selected: Vec<u32> = files.iter().map(|f| f.idx).collect();
        let size = sel_size(&files, &selected);
        if files.iter().any(|f| !rel_path_ok(&f.path)) {
            return Err(CoreError::Other("种子文件路径非法".into()));
        }
        let state = if files.len() <= 1 {
            TorrentState::Queued
        } else {
            TorrentState::AwaitingSelection
        };
        let id = self.store.lock().unwrap().insert_torrent(
            &meta.hash,
            source,
            &name,
            &dir,
            state,
            &trackers,
        )?;
        self.store.lock().unwrap().set_torrent_meta(
            id,
            &name,
            &files,
            &selected,
            size,
            Some(bytes),
        )?;
        let info = self.torrent_info(id)?;
        self.emit_torrent(TorrentEvent::TorrentAdded {
            torrent: info.clone(),
        });
        Ok((info, false))
    }

    pub(crate) fn resume_pending_resolve(self: Arc<Self>) {
        // MutexGuard 活到整句结束. 同一语句里再 lock store 会自锁, 窗口永远起不来.
        let pending = {
            let store = self.store.lock().unwrap();
            store.list_resolving_sources().unwrap_or_default()
        };
        for (id, src) in pending {
            if src.starts_with("magnet:") {
                tracing::info!(id, "恢复未完成的磁力解析");
                self.clone().spawn_resolve(id, src);
            }
        }
    }

    pub(crate) fn abort_resolve(&self, id: i64) {
        if let Some(tx) = self.resolve_abort.lock().unwrap().remove(&id) {
            let _ = tx.send(true);
        }
    }

    pub(crate) fn spawn_resolve(self: Arc<Self>, id: i64, magnet: String) {
        let (tx, mut rx) = watch::channel(false);
        self.resolve_abort.lock().unwrap().insert(id, tx);
        tokio::spawn(async move {
            let run = self.resolve_magnet(id, &magnet);
            let aborted = async {
                loop {
                    if *rx.borrow() {
                        return;
                    }
                    if rx.changed().await.is_err() {
                        return;
                    }
                }
            };
            tokio::select! {
                r = run => {
                    self.resolve_abort.lock().unwrap().remove(&id);
                    if let Err(e) = r {
                        tracing::warn!(id, magnet = %magnet, error = %e, "Resolve 失败, 不进下载列表");
                        let _ = self.store.lock().unwrap().delete_torrent(id);
                        self.emit_torrent(TorrentEvent::ResolveFailed {
                            id,
                            source: magnet,
                            error: e.to_string(),
                        });
                    }
                }
                _ = aborted => {
                    tracing::debug!(id, "Resolve 被取消");
                    let _ = self.store.lock().unwrap().delete_torrent(id);
                }
            }
            let _ = self.schedule_bt_inner().await;
        });
    }

    async fn resolve_magnet(&self, id: i64, magnet: &str) -> Result<()> {
        tracing::debug!(id, magnet = %magnet, "开始 Resolve Metainfo");
        let secs = self.live.lock().unwrap().resolve_secs as u64;
        let budget = Duration::from_secs(secs);
        let t0 = Instant::now();
        let hash = infohash_from_magnet(magnet).unwrap_or_default();
        let client = self.client.lock().unwrap().clone();
        // HTTP 缓存通常秒回, 不依赖 P2P; 未命中 / 校验失败才走 DHT
        if !hash.is_empty() {
            let http = tokio::time::timeout(budget, fetch_meta_http(&client, &hash)).await;
            match http {
                Ok(Some(bytes)) => {
                    match self.commit_torrent_bytes(id, &bytes, &hash) {
                        Ok(()) => {
                            tracing::info!(id, hash = %hash, n = bytes.len(), "HTTP 种子缓存命中");
                            return Ok(());
                        }
                        Err(e) => {
                            tracing::warn!(id, hash = %hash, error = %e, "HTTP 缓存不可用, 改 DHT");
                        }
                    }
                }
                _ => tracing::debug!(id, hash = %hash, "HTTP 缓存未命中, 改 DHT/Tracker"),
            }
        }
        if !self.live.lock().unwrap().p2p {
            return Err(CoreError::Other(
                "HTTP 缓存未命中. 打开 P2P 后可用 DHT 解析".into(),
            ));
        }
        let left = budget.saturating_sub(t0.elapsed());
        if left.is_zero() {
            return Err(CoreError::Other(format!("解析超时 ({secs}s)")));
        }
        let session = self.wait_bt_session(t0 + left).await?;
        let left = budget.saturating_sub(t0.elapsed());
        if left.is_zero() {
            return Err(CoreError::Other(format!("解析超时 ({secs}s)")));
        }
        let mut trackers = self.store.lock().unwrap().torrent_trackers(id)?;
        // 解析阶段必须带公共 tracker: 纯 infohash 没有 announce. 落盘时再按 private/开关裁.
        self.merge_pub_trackers(&mut trackers);
        // magnet 路径丢弃 opts.trackers, 必须写进 URL
        let magnet_url = trackers::magnet_with_trackers(magnet, &trackers);
        let add = session.add_torrent(
            AddTorrent::from_url(magnet_url),
            Some(AddTorrentOptions {
                list_only: true,
                trackers: Some(trackers),
                ..Default::default()
            }),
        );
        let resp = match tokio::time::timeout(left, add).await {
            Ok(Ok(r)) => r,
            Ok(Err(e)) => return Err(CoreError::Other(format!("Resolve 失败: {e:#}"))),
            Err(_) => {
                return Err(CoreError::Other(format!(
                    "解析超时 ({secs}s), DHT/Tracker 无响应"
                )))
            }
        };
        let list = match resp {
            AddTorrentResponse::ListOnly(l) => l,
            other => {
                if let Some(h) = other.into_handle() {
                    let _ = session.delete(librqbit::api::TorrentIdOrHash::Id(h.id()), false).await;
                }
                return Err(CoreError::Other("Resolve 未返回文件列表".into()));
            }
        };
        if list.torrent_bytes.len() > 64 {
            return self.commit_torrent_bytes(id, list.torrent_bytes.as_ref(), &hash);
        }
        if !hash.is_empty() && !list.info_hash.as_string().eq_ignore_ascii_case(&hash) {
            return Err(CoreError::Other("Metainfo infohash 与磁力不一致".into()));
        }
        self.commit_metainfo(
            id,
            list.info
                .name()
                .map(|s| s.into_owned())
                .unwrap_or_else(|| list.info_hash.as_string()),
            files_from_list(&list.info)?,
            list.torrent_bytes.as_ref(),
            list.info.info().private,
            &[],
        )
    }

    fn commit_torrent_bytes(&self, id: i64, bytes: &[u8], expect_hash: &str) -> Result<()> {
        let meta = meta_from_bytes(bytes)
            .map_err(|e| CoreError::Other(format!("缓存返回非法 torrent: {e}")))?;
        if !expect_hash.is_empty() && !meta.hash.eq_ignore_ascii_case(expect_hash) {
            return Err(CoreError::Other("Metainfo infohash 与磁力不一致".into()));
        }
        self.commit_metainfo(
            id,
            meta.name,
            meta.files,
            bytes,
            meta.private,
            &meta.trackers,
        )
    }

    fn commit_metainfo(
        &self,
        id: i64,
        name: String,
        files: Vec<TorrentFile>,
        bytes: &[u8],
        private: bool,
        meta_trackers: &[String],
    ) -> Result<()> {
        if files.iter().any(|f| !rel_path_ok(&f.path)) {
            return Err(CoreError::Other("种子文件路径非法".into()));
        }
        let selected: Vec<u32> = files.iter().map(|f| f.idx).collect();
        let size = sel_size(&files, &selected);
        let mut trackers = if meta_trackers.is_empty() {
            self.store
                .lock()
                .unwrap()
                .torrent_trackers(id)
                .unwrap_or_default()
        } else {
            meta_trackers.to_vec()
        };
        if !private && self.live.lock().unwrap().extra_trackers {
            self.merge_pub_trackers(&mut trackers);
        }
        if private {
            // private=1 只留种子自带 tracker, 去掉解析阶段注入的公共列表.
            trackers = meta_trackers.to_vec();
            if trackers.is_empty() {
                trackers = self
                    .store
                    .lock()
                    .unwrap()
                    .torrent_trackers(id)
                    .unwrap_or_default();
            }
        }
        {
            let store = self.store.lock().unwrap();
            store.set_torrent_meta(id, &name, &files, &selected, size, Some(bytes))?;
            store.replace_trackers(id, &trackers)?;
            let next = if files.len() <= 1 {
                TorrentState::Queued
            } else {
                TorrentState::AwaitingSelection
            };
            store.set_torrent_state(id, next, "")?;
        }
        let info = self.torrent_info(id)?;
        tracing::info!(
            id,
            name = %info.name,
            files = info.files.len(),
            "Resolve 成功, 加入下载列表"
        );
        self.emit_torrent(TorrentEvent::TorrentAdded { torrent: info });
        Ok(())
    }

    fn torrent_private(&self, id: i64) -> bool {
        self.store
            .lock()
            .unwrap()
            .torrent_metainfo(id)
            .ok()
            .flatten()
            .and_then(|b| meta_from_bytes(&b).ok())
            .map(|m| m.private)
            .unwrap_or(false)
    }

    pub(crate) fn select_files(&self, id: i64, selected: Vec<u32>) -> Result<TorrentInfo> {
        let info = self.torrent_info(id)?;
        if selected.is_empty() {
            return Err(CoreError::Other("至少选择一个文件".into()));
        }
        let prev: HashSet<u32> = info
            .files
            .iter()
            .filter(|f| f.selected)
            .map(|f| f.idx)
            .collect();
        let added = selected.iter().any(|i| !prev.contains(i));
        let size = sel_size(&info.files, &selected);
        self.store
            .lock()
            .unwrap()
            .set_torrent_selected(id, &selected, size)?;
        if let Some((session, h)) = self.handle_of(id) {
            let set: HashSet<usize> = selected.iter().map(|i| *i as usize).collect();
            tokio::spawn(async move {
                let _ = session.update_only_files(&h, &set).await;
            });
        }
        match info.state {
            TorrentState::AwaitingSelection => {
                self.set_tstate(id, TorrentState::Queued, "")?;
            }
            TorrentState::Seeding if added => {
                // 给已完成种加文件必须重新占下载额度, 不能留在 Seeding.
                self.set_tstate(id, TorrentState::Queued, "")?;
            }
            TorrentState::Active | TorrentState::Paused | TorrentState::Seeding
            | TorrentState::Queued | TorrentState::Failed => {
                if let Ok(t) = self.torrent_info(id) {
                    self.emit_torrent(TorrentEvent::TorrentUpdated { torrent: t });
                }
            }
            _ => {}
        }
        self.torrent_info(id)
    }

    fn add_opts(&self, info: &TorrentInfo) -> AddTorrentOptions {
        let selected: Vec<usize> = info
            .files
            .iter()
            .filter(|f| f.selected)
            .map(|f| f.idx as usize)
            .collect();
        let mut trackers = self.store.lock().unwrap().torrent_trackers(info.id).unwrap_or_default();
        if !self.torrent_private(info.id) && self.live.lock().unwrap().extra_trackers {
            self.merge_pub_trackers(&mut trackers);
        }
        let out = bt_out_dir(&info.dir, &info.name, info.files.len());
        // paused:false: 必须在 add 时带上 peer_rx. 先 paused 再立刻 unpause
        // 会撞 Initializing, try_start_check 失败直接 return, 校验完仍停在 Paused, 永远 0 速.
        AddTorrentOptions {
            paused: false,
            overwrite: true,
            only_files: if selected.is_empty() {
                None
            } else {
                Some(selected)
            },
            output_folder: Some(out.to_string_lossy().into_owned()),
            trackers: if trackers.is_empty() { None } else { Some(trackers) },
            ..Default::default()
        }
    }

    async fn sync_only_files(
        &self,
        session: &Arc<Session>,
        h: &Arc<ManagedTorrent>,
        id: i64,
    ) -> Result<()> {
        let info = self.torrent_info(id)?;
        let set: HashSet<usize> = info
            .files
            .iter()
            .filter(|f| f.selected)
            .map(|f| f.idx as usize)
            .collect();
        session
            .update_only_files(h, &set)
            .await
            .map_err(|e| CoreError::Other(format!("更新 File Selection: {e:#}")))
    }

    async fn ensure_handle(&self, id: i64) -> Result<Arc<ManagedTorrent>> {
        if let Some((session, h)) = self.handle_of(id) {
            // pause 后再改勾选, handle 还在, 必须跟 sqlite 对齐, 否则新文件永不拉.
            let _ = self.sync_only_files(&session, &h, id).await;
            return Ok(h);
        }
        let info = self.torrent_info(id)?;
        let bytes = self
            .store
            .lock()
            .unwrap()
            .torrent_metainfo(id)?
            .ok_or_else(|| CoreError::Other("没有 Metainfo, 无法启动".into()))?;
        let session = self
            .bt_session()
            .ok_or_else(|| CoreError::Other("BT session 未就绪".into()))?;
        let resp = session
            .add_torrent(AddTorrent::from_bytes(bytes), Some(self.add_opts(&info)))
            .await
            .map_err(|e| CoreError::Other(format!("加入 swarm: {e:#}")))?;
        let handle = resp
            .into_handle()
            .ok_or_else(|| CoreError::Other("加入 swarm 未返回 handle".into()))?;
        if let Some(bt) = self.bt.lock().unwrap().as_ref() {
            bt.handles.lock().unwrap().insert(id, handle.clone());
        }
        Ok(handle)
    }

    pub(crate) async fn schedule_bt_inner(&self) -> Result<()> {
        if !self.live.lock().unwrap().p2p {
            return Ok(());
        }
        if self.bt_session().is_none() {
            tracing::debug!("BT session 未就绪, 保持排队");
            return Ok(());
        }
        if self.bt_busy.swap(true, Ordering::SeqCst) {
            return Ok(());
        }
        let out = self.schedule_bt_run().await;
        self.bt_busy.store(false, Ordering::SeqCst);
        out
    }

    async fn schedule_bt_run(&self) -> Result<()> {
        let live = self.live.lock().unwrap().clone();
        loop {
            let active = self
                .store
                .lock()
                .unwrap()
                .count_torrent_state(TorrentState::Active)?;
            if active >= live.max_bt_active as usize {
                break;
            }
            let next = self.store.lock().unwrap().next_queued_torrent()?;
            let Some(id) = next else { break };
            tracing::info!(id, "BT 出队, 加入 swarm");
            self.set_tstate(id, TorrentState::Active, "")?;
            match self.ensure_handle(id).await {
                Ok(h) => {
                    if let Err(e) = h.wait_until_initialized().await {
                        tracing::warn!(id, error = %e, "校验失败");
                        self.set_tstate(id, TorrentState::Failed, &e.to_string())?;
                        continue;
                    }
                    match h.stats().state {
                        TorrentStatsState::Live => {}
                        TorrentStatsState::Error => {
                            let err = h.stats().error.unwrap_or_else(|| "BT 错误".into());
                            self.set_tstate(id, TorrentState::Failed, &err)?;
                        }
                        _ => {
                            let Some(session) = self.bt_session() else {
                                self.set_tstate(id, TorrentState::Queued, "")?;
                                break;
                            };
                            if let Err(e) = session.unpause(&h).await {
                                tracing::warn!(id, error = %e, "unpause 失败");
                                self.set_tstate(id, TorrentState::Failed, &format!("{e:#}"))?;
                            }
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!(id, error = %e, "加入 swarm 失败");
                    self.set_tstate(id, TorrentState::Failed, &e.to_string())?;
                }
            }
        }
        self.apply_seed_cap(live.max_bt_seed as usize).await
    }

    /// sqlite 保持 Seeding; 超出的只 pause handle. 额度回来再 unpause, 避免单向停死.
    async fn apply_seed_cap(&self, cap: usize) -> Result<()> {
        let seeds = self
            .store
            .lock()
            .unwrap()
            .list_torrent_ids(TorrentState::Seeding)?;
        for (i, id) in seeds.iter().enumerate() {
            if i < cap {
                match self.ensure_handle(*id).await {
                    Ok(h) => {
                        if matches!(h.stats().state, TorrentStatsState::Paused) {
                            if let Some(session) = self.bt_session() {
                                if let Err(e) = session.unpause(&h).await {
                                    tracing::warn!(id, error = %e, "做种 unpause 失败");
                                }
                            }
                        }
                    }
                    Err(e) => tracing::warn!(id, error = %e, "做种加入 swarm 失败"),
                }
            } else if let Some((session, h)) = self.handle_of(*id) {
                if matches!(h.stats().state, TorrentStatsState::Live) {
                    let _ = session.pause(&h).await;
                }
            }
        }
        Ok(())
    }

    pub(crate) fn pause_torrent(&self, id: i64, emit_paused: bool) -> Result<()> {
        let info = self.torrent_info(id)?;
        if let Some((session, h)) = self.handle_of(id) {
            tokio::spawn(async move {
                let _ = session.pause(&h).await;
            });
        }
        if emit_paused
            && matches!(
                info.state,
                TorrentState::Active | TorrentState::Seeding | TorrentState::Queued
            )
        {
            self.set_tstate(id, TorrentState::Paused, "")?;
        }
        Ok(())
    }

    pub(crate) fn resume_torrent(&self, id: i64) -> Result<()> {
        let info = self.torrent_info(id)?;
        if !matches!(info.state, TorrentState::Paused | TorrentState::Failed) {
            return Ok(());
        }
        let complete = info.size.unwrap_or(0) > 0 && info.done >= info.size.unwrap_or(0);
        let next = if complete {
            TorrentState::Seeding
        } else {
            TorrentState::Queued
        };
        self.set_tstate(id, next, "")?;
        Ok(())
    }

    pub(crate) fn kick_bt(inner: Arc<Inner>) {
        tokio::spawn(async move {
            let _ = inner.schedule_bt_inner().await;
        });
    }

    pub(crate) async fn remove_torrent(&self, id: i64, delete_file: bool) -> Result<()> {
        self.abort_resolve(id);
        let info = self.torrent_info(id)?;
        let (session, handle) = {
            let g = self.bt.lock().unwrap();
            match g.as_ref() {
                Some(bt) => (
                    Some(bt.session.clone()),
                    bt.handles.lock().unwrap().remove(&id),
                ),
                None => (None, None),
            }
        };
        match (session, handle) {
            (Some(session), Some(h)) => {
                let _ = session
                    .delete(librqbit::api::TorrentIdOrHash::Id(h.id()), delete_file)
                    .await;
            }
            (_, None) if delete_file => {
                let out = bt_out_dir(&info.dir, &info.name, info.files.len());
                for f in &info.files {
                    if f.selected {
                        if let Some(p) = safe_join(&out, &f.path) {
                            let _ = std::fs::remove_file(&p);
                        }
                    }
                }
                if info.files.len() >= 2 {
                    let _ = std::fs::remove_dir(&out);
                }
            }
            _ => {}
        }
        self.store.lock().unwrap().delete_torrent(id)?;
        self.emit_torrent(TorrentEvent::TorrentRemoved { id });
        Ok(())
    }

}
