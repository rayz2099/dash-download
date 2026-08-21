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
}

/// UI / prefs / 引擎共用的一份设置. 改完立刻作用于新连接, 已跑任务沿用旧 Client.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineSettings {
    pub default_dir: String,
    pub max_concurrent: u32,
    pub max_segments: u32,
    pub proxy: ProxyCfg,
}

impl EngineSettings {
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
        self.proxy.validate()
    }
}

/// 代理连通性探测结果. 不是下载任务, 只给设置页点「测试」用.
#[derive(Debug, Clone, Serialize)]
pub struct ProxyProbe {
    pub status: u16,
    pub ms: u64,
    pub final_url: String,
}
