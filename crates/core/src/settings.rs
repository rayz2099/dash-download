//! 引擎运行时偏好. 与 Task sqlite 分开, 避免把桌面壳字段写进下载库 schema.
//! 持久化由 app 的 prefs.json 负责, 核心只持有一份可热更新的快照.

use crate::error::{CoreError, Result};
use serde::{Deserialize, Serialize};

/// 每任务连接数 / 同时下载数上限. 再高对 TCP 与文件句柄没有收益.
pub const MAX_CONN: u32 = 128;

/// blob/data 直写上限. JSON base64 约 4/3, 对齐 app DefaultBodyLimit 32MB.
pub const MAX_IMPORT_BYTES: usize = 24 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProxyKind {
    /// 强制直连. 旧 prefs 的 off 同义, 显式 no_proxy 忽略 HTTP_PROXY.
    #[serde(alias = "off")]
    Direct,
    /// 不配应用内代理, 跟随环境变量 HTTP_PROXY / NO_PROXY.
    NoProxy,
    Http,
    Socks5,
}

impl Default for ProxyKind {
    fn default() -> Self {
        ProxyKind::Direct
    }
}

impl ProxyKind {
    pub fn is_manual(self) -> bool {
        matches!(self, ProxyKind::Http | ProxyKind::Socks5)
    }
}

/// HTTP CONNECT 或 SOCKS5. Direct 关掉环境代理; NoProxy 跟随环境变量.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProxyCfg {
    pub kind: ProxyKind,
    pub host: String,
    pub port: u16,
    pub auth: bool,
    pub user: String,
    pub pass: String,
}

impl Default for ProxyCfg {
    fn default() -> Self {
        Self {
            kind: ProxyKind::Direct,
            host: String::new(),
            port: 0,
            auth: false,
            user: String::new(),
            pass: String::new(),
        }
    }
}

impl ProxyCfg {
    pub fn validate(&self) -> Result<()> {
        match self.kind {
            ProxyKind::Direct | ProxyKind::NoProxy => Ok(()),
            ProxyKind::Http | ProxyKind::Socks5 => {
                if self.host.trim().is_empty() {
                    return Err(CoreError::Other("代理主机不能为空".into()));
                }
                if self.port == 0 {
                    return Err(CoreError::Other("代理端口无效".into()));
                }
                if self.auth && self.user.trim().is_empty() {
                    return Err(CoreError::Other("开启认证时用户名不能为空".into()));
                }
                Ok(())
            }
        }
    }

    /// librqbit 只认 socks5://, user/pass 必须百分号编码, 否则 `@` `:` 会截断 URL.
    pub fn socks5_url(&self) -> String {
        if !self.auth {
            return format!("socks5://{}:{}", self.host.trim(), self.port);
        }
        format!(
            "socks5://{}:{}@{}:{}",
            pct_enc(&self.user),
            pct_enc(&self.pass),
            self.host.trim(),
            self.port
        )
    }
}

fn pct_enc(s: &str) -> String {
    let mut out = String::new();
    for b in s.as_bytes() {
        if b.is_ascii_alphanumeric() || matches!(*b, b'-' | b'_' | b'.' | b'~') {
            out.push(*b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// 磁力解析墙钟. 再短 HTTP 缓存都跑不完, 再长 DHT 也救不了死 magnet.
const MIN_RESOLVE_SECS: u32 = 5;
const MAX_RESOLVE_SECS: u32 = 300;

/// UI / prefs / 引擎共用的一份设置. 改完立刻作用于新连接, 已跑任务沿用旧 Client.
/// `serde(default)` 吃掉旧 prefs 缺的 BT 字段, 避免 Prefs 再抄一份默认值.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct EngineSettings {
    pub default_dir: String,
    pub max_concurrent: u32,
    pub max_segments: u32,
    pub proxy: ProxyCfg,
    /// 同时拉数据的 Torrent 数. 做种不占这个额度.
    pub max_bt_active: u32,
    /// 做种软顶, 超出暂停上传但不改「已完成/做种」语义以外的状态.
    pub max_bt_seed: u32,
    /// 入站端口. 0 表示首次启动时抽高位端口后由 app 落盘.
    pub listen_port: u16,
    pub upnp: bool,
    /// 下载/做种附加公共 tracker. 磁力解析仍会注入以便拉 Metainfo; private=1 永不附加.
    pub extra_trackers: bool,
    /// 磁力解析总超时 (秒). HTTP 缓存 + DHT 共用这段墙钟.
    pub resolve_secs: u32,
    /// P2P 总开关. 默认关: DHT / Tracker / 入站会对外通信, 要用户主动开.
    pub p2p: bool,
}

impl Default for EngineSettings {
    fn default() -> Self {
        Self {
            default_dir: String::new(),
            max_concurrent: 3,
            max_segments: 8,
            proxy: ProxyCfg::default(),
            max_bt_active: 3,
            max_bt_seed: 10,
            listen_port: 0,
            upnp: true,
            extra_trackers: true,
            resolve_secs: 30,
            p2p: false,
        }
    }
}

impl EngineSettings {
    /// 0 表示还没落盘, 抽高位端口避免跟系统服务撞. 必须写回 prefs, 否则每次启动换口.
    pub fn fill_listen_port(&mut self) {
        if self.listen_port != 0 {
            return;
        }
        let n = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u16)
            .unwrap_or(0);
        self.listen_port = 40000 + n % 20000;
    }

    pub fn validate(&self) -> Result<()> {
        if self.default_dir.trim().is_empty() {
            return Err(CoreError::Other("默认目录不能为空".into()));
        }
        if !(1..=MAX_CONN).contains(&self.max_concurrent) {
            return Err(CoreError::Other("同时下载数必须在 1..=128".into()));
        }
        if !(1..=MAX_CONN).contains(&self.max_segments) {
            return Err(CoreError::Other("每任务连接数必须在 1..=128".into()));
        }
        if !(1..=MAX_CONN).contains(&self.max_bt_active) {
            return Err(CoreError::Other("BT 同时下载数必须在 1..=128".into()));
        }
        if !(1..=MAX_CONN).contains(&self.max_bt_seed) {
            return Err(CoreError::Other("做种上限必须在 1..=128".into()));
        }
        if !(MIN_RESOLVE_SECS..=MAX_RESOLVE_SECS).contains(&self.resolve_secs) {
            return Err(CoreError::Other("磁力解析超时必须在 5..=300 秒".into()));
        }
        self.proxy.validate()
    }

    /// HTTP 代理扛不住 DHT/uTP, BT 直连并让 UI 提示.
    pub fn bt_direct(&self) -> bool {
        matches!(self.proxy.kind, ProxyKind::Http)
    }
}

/// 代理连通性探测结果. 不是下载任务, 只给设置页点「测试」用.
#[derive(Debug, Clone, Serialize)]
pub struct ProxyProbe {
    pub status: u16,
    pub ms: u64,
    pub final_url: String,
}
