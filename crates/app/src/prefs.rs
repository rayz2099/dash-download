//! 本机偏好: 开机自启 / 自动更新 / 引擎运行时设置.
//! 与 Task sqlite 分开, 避免引擎 schema 被桌面壳字段污染.
//! Store 是唯一写入口, 防止 updater 与设置 API 各自持有一份过期快照互相覆盖.

use dd_core::{EngineSettings, ProxyCfg};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Prefs {
    pub auto_update: bool,
    pub auto_start: bool,
    #[serde(default)]
    pub default_dir: String,
    #[serde(default = "def_conc")]
    pub max_concurrent: u32,
    #[serde(default = "def_seg")]
    pub max_segments: u32,
    #[serde(default)]
    pub proxy: ProxyCfg,
}

fn def_conc() -> u32 {
    3
}
fn def_seg() -> u32 {
    8
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            auto_update: true,
            auto_start: true,
            default_dir: String::new(),
            max_concurrent: 3,
            max_segments: 8,
            proxy: ProxyCfg::default(),
        }
    }
}

impl Prefs {
    pub fn apply_engine(&mut self, s: &EngineSettings) {
        self.default_dir = s.default_dir.clone();
        self.max_concurrent = s.max_concurrent;
        self.max_segments = s.max_segments;
        self.proxy = s.proxy.clone();
    }
}

pub fn path(cfg_dir: &Path) -> PathBuf {
    cfg_dir.join("prefs.json")
}

pub fn load(cfg_dir: &Path) -> Result<Prefs, String> {
    let p = path(cfg_dir);
    match std::fs::read_to_string(&p) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Prefs::default()),
        Err(e) => Err(format!("读 prefs.json 失败: {e}")),
        Ok(raw) => serde_json::from_str(&raw)
            .map_err(|e| format!("prefs.json 损坏, 拒绝用默认值覆盖: {e}")),
    }
}

pub fn save(cfg_dir: &Path, prefs: &Prefs) -> Result<(), String> {
    let p = path(cfg_dir);
    let tmp = cfg_dir.join("prefs.json.tmp");
    let raw = serde_json::to_string_pretty(prefs).map_err(|e| e.to_string())?;
    std::fs::write(&tmp, &raw).map_err(|e| e.to_string())?;
    #[cfg(windows)]
    {
        // Windows rename 不能覆盖已有文件; copy 覆盖可让旧 prefs.json 始终在盘上
        std::fs::copy(&tmp, &p).map_err(|e| e.to_string())?;
        let _ = std::fs::remove_file(&tmp);
        return Ok(());
    }
    #[cfg(not(windows))]
    {
        std::fs::rename(&tmp, &p).map_err(|e| e.to_string())
    }
}

/// 进程内唯一偏好句柄. updater 与 /api/settings 必须走这里写盘.
#[derive(Clone)]
pub struct Store {
    dir: PathBuf,
    inner: Arc<Mutex<Prefs>>,
}

impl Store {
    pub fn load(cfg_dir: &Path) -> Result<Self, String> {
        Ok(Self {
            dir: cfg_dir.to_path_buf(),
            inner: Arc::new(Mutex::new(load(cfg_dir)?)),
        })
    }

    pub fn get(&self) -> Prefs {
        self.inner.lock().expect("prefs mutex").clone()
    }

    pub fn patch<F: FnOnce(&mut Prefs)>(&self, f: F) -> Result<Prefs, String> {
        let mut g = self.inner.lock().expect("prefs mutex");
        let mut next = g.clone();
        f(&mut next);
        save(&self.dir, &next)?;
        *g = next.clone();
        Ok(next)
    }
}
