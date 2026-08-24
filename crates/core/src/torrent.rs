use crate::error::{CoreError, Result};
use serde::{Deserialize, Serialize};
use std::path::{Component, Path, PathBuf};

/// Torrent 自有状态机. 不复用 TaskState: Seeding 不是 Completed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TorrentState {
    Resolving,
    AwaitingSelection,
    Queued,
    Active,
    Seeding,
    Paused,
    Failed,
}

impl TorrentState {
    pub fn as_str(self) -> &'static str {
        match self {
            TorrentState::Resolving => "resolving",
            TorrentState::AwaitingSelection => "awaiting_selection",
            TorrentState::Queued => "queued",
            TorrentState::Active => "active",
            TorrentState::Seeding => "seeding",
            TorrentState::Paused => "paused",
            TorrentState::Failed => "failed",
        }
    }

    pub fn parse(s: &str) -> TorrentState {
        match s {
            "resolving" => TorrentState::Resolving,
            "awaiting_selection" => TorrentState::AwaitingSelection,
            "queued" => TorrentState::Queued,
            "active" => TorrentState::Active,
            "seeding" => TorrentState::Seeding,
            "paused" => TorrentState::Paused,
            _ => TorrentState::Failed,
        }
    }

    /// 正在拉数据, 占用 BT 下载额度
    pub fn is_downloading(self) -> bool {
        matches!(self, TorrentState::Active)
    }

    pub fn is_seeding(self) -> bool {
        matches!(self, TorrentState::Seeding)
    }
}

/// 当前连上或正在连的 Peer. 不落库, 只跟进度一起推 UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TorrentPeer {
    pub addr: String,
    pub client: String,
    pub state: String,
    pub down: u64,
    pub up: u64,
    pub kind: String,
    #[serde(default)]
    pub chunks: u32,
    #[serde(default)]
    pub pieces: u32,
    #[serde(default)]
    pub piece_ms: u64,
    #[serde(default)]
    pub conn_ms: u64,
    #[serde(default)]
    pub attempts: u32,
    #[serde(default)]
    pub errors: u32,
    #[serde(default)]
    pub incoming: bool,
}

/// Torrent 内一个文件. selected 表示 File Selection 成员.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TorrentFile {
    pub idx: u32,
    pub path: String,
    pub size: u64,
    pub selected: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TorrentInfo {
    pub id: i64,
    pub infohash: String,
    /// magnet 或 .torrent 来源 URL/路径
    pub source: String,
    pub name: String,
    pub dir: String,
    pub state: TorrentState,
    pub done: u64,
    /// File Selection 合计大小
    pub size: Option<u64>,
    pub speed: u64,
    pub up_speed: u64,
    pub error: String,
    pub files: Vec<TorrentFile>,
    pub peers: u32,
    /// tracker/DHT 见过的 Peer, 含还没握手成功的
    #[serde(default)]
    pub seen: u32,
    #[serde(default)]
    pub connecting: u32,
    #[serde(default)]
    pub peer_list: Vec<TorrentPeer>,
    /// librqbit 内部态: initializing / live / paused. 校验中的 progress 不是已下载.
    #[serde(default)]
    pub phase: String,
    /// 当前 HTTP 代理套不上 BT 时提示直连
    #[serde(default)]
    pub bt_direct: bool,
    pub created_at: i64,
    pub completed_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TorrentProgress {
    pub id: i64,
    pub done: u64,
    pub speed: u64,
    pub up_speed: u64,
    pub peers: u32,
    #[serde(default)]
    pub seen: u32,
    #[serde(default)]
    pub connecting: u32,
    pub phase: String,
    #[serde(default)]
    pub peer_list: Vec<TorrentPeer>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TorrentEvent {
    /// Metainfo 还在拉, 弹出解析层, 不进下载列表.
    Resolving { torrent: TorrentInfo },
    TorrentAdded { torrent: TorrentInfo },
    TorrentUpdated { torrent: TorrentInfo },
    TorrentRemoved { id: i64 },
    TorrentProgress { torrents: Vec<TorrentProgress> },
    /// Metainfo 没拉到. 解析层显示失败, 可叉掉.
    ResolveFailed { id: i64, source: String, error: String },
}

/// 从 magnet 抽出 v1 Infohash. 没有 btih 则 Resolve 前无法判重.
pub fn infohash_from_magnet(s: &str) -> Option<String> {
    librqbit::Magnet::parse(s.trim())
        .ok()
        .and_then(|m| m.as_id20())
        .map(|id| id.as_string())
}

/// 相对路径只能有 Normal 分量, 否则会写出/删到下载目录外.
pub(crate) fn rel_path_ok(path: &str) -> bool {
    let p = Path::new(path);
    !path.is_empty() && p.components().all(|c| matches!(c, Component::Normal(_)))
}

pub(crate) fn files_from_list<B: AsRef<[u8]>>(
    info: &librqbit::ValidatedTorrentMetaV1Info<B>,
) -> Result<Vec<TorrentFile>> {
    let mut files = Vec::new();
    for (i, d) in info.iter_file_details().enumerate() {
        let path = d.filename.to_string();
        if !rel_path_ok(&path) {
            return Err(CoreError::Other(format!("种子文件路径非法: {path}")));
        }
        files.push(TorrentFile {
            idx: i as u32,
            path,
            size: d.len,
            selected: true,
        });
    }
    Ok(files)
}

/// 种子名做目录名: 去掉分隔符, 避免 `Downloads/../x`.
pub(crate) fn safe_dir_name(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| if matches!(c, '/' | '\\' | ':' | '\0') { '_' } else { c })
        .collect();
    let s = s.trim().trim_matches('.');
    if s.is_empty() || s == "." || s == ".." {
        "torrent".into()
    } else {
        s.to_string()
    }
}

/// 单文件直写用户目录; 2 个及以上包一层种子名, 避免一堆磁力摊在 Downloads.
pub(crate) fn bt_out_dir(dir: &str, name: &str, file_n: usize) -> PathBuf {
    let base = PathBuf::from(dir);
    if file_n < 2 {
        base
    } else {
        base.join(safe_dir_name(name))
    }
}

pub(crate) fn safe_join(dir: &Path, rel: &str) -> Option<PathBuf> {
    rel_path_ok(rel).then(|| dir.join(rel))
}

pub(crate) fn sel_size(files: &[TorrentFile], selected: &[u32]) -> Option<u64> {
    Some(
        files
            .iter()
            .filter(|f| selected.contains(&f.idx))
            .map(|f| f.size)
            .sum(),
    )
}

pub(crate) struct MetaSnap {
    pub hash: String,
    pub name: String,
    pub files: Vec<TorrentFile>,
    pub trackers: Vec<String>,
    pub private: bool,
}

pub(crate) fn meta_from_bytes(bytes: &[u8]) -> Result<MetaSnap> {
    let parsed = librqbit::torrent_from_bytes(bytes)
        .map_err(|e| CoreError::Other(format!("非法 .torrent: {e}")))?;
    let validated = parsed
        .info
        .data
        .clone()
        .validate()
        .map_err(|e| CoreError::Other(format!("metainfo: {e}")))?;
    let hash = parsed.info_hash.as_string();
    let files = files_from_list(&validated)?;
    let name = validated
        .name()
        .map(|s| s.into_owned())
        .unwrap_or_else(|| hash[..8.min(hash.len())].to_string());
    let trackers = parsed
        .iter_announce()
        .filter_map(|b| std::str::from_utf8(b.as_ref()).ok().map(str::to_string))
        .collect();
    Ok(MetaSnap {
        hash,
        name,
        files,
        trackers,
        private: parsed.info.data.private,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rel_path_rejects_traversal() {
        assert!(rel_path_ok("a/b.mkv"));
        assert!(rel_path_ok("foo.mkv"));
        assert!(!rel_path_ok("../etc/passwd"));
        assert!(!rel_path_ok("/tmp/x"));
        assert!(!rel_path_ok(""));
        assert!(!rel_path_ok("a/../b"));
    }

    #[test]
    fn single_file_stays_in_dest() {
        let p = bt_out_dir("/Downloads", "movie.mkv", 1);
        assert_eq!(p, PathBuf::from("/Downloads"));
    }

    #[test]
    fn multi_file_wraps_name() {
        let p = bt_out_dir("/Downloads", "哪吒之魔童降世", 2);
        assert_eq!(p, PathBuf::from("/Downloads/哪吒之魔童降世"));
        let p = bt_out_dir("/Downloads", "foo/bar", 3);
        assert_eq!(p, PathBuf::from("/Downloads/foo_bar"));
    }
}
