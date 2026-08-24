//! 用 GitHub Releases API 发现版本和带 .sig 的安装包, 再交给 tauri-plugin-updater 验签安装.
//! 有正在跑的 Task 时等到空闲再 install/restart, 避免截断 pwrite.

use crate::gh_update::{GH_LATEST, MANIFEST_URL};
use crate::prefs;
use dd_core::Engine;
use serde::Serialize;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Manager};
use tauri_plugin_updater::UpdaterExt;
use url::Url;

#[derive(Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Idle,
    Checking,
    UpToDate,
    Available,
    Downloading,
    Waiting,
    Installing,
    Error,
}

#[derive(Clone, Serialize)]
pub struct Status {
    pub auto_update: bool,
    pub current: String,
    pub latest: Option<String>,
    pub phase: Phase,
    pub done: u64,
    pub total: Option<u64>,
    pub error: Option<String>,
}

struct Inner {
    status: Status,
    busy: bool,
}

#[derive(Clone)]
pub struct Updater {
    engine: Engine,
    prefs: prefs::Store,
    inner: Arc<Mutex<Inner>>,
}

impl Updater {
    pub fn new(engine: Engine, prefs: prefs::Store) -> Self {
        let current = env!("CARGO_PKG_VERSION").to_string();
        let snapshot = prefs.get();
        Self {
            engine,
            prefs,
            inner: Arc::new(Mutex::new(Inner {
                status: Status {
                    auto_update: snapshot.auto_update,
                    current,
                    latest: None,
                    phase: Phase::Idle,
                    done: 0,
                    total: None,
                    error: None,
                },
                busy: false,
            })),
        }
    }

    pub fn snapshot(&self) -> Status {
        self.inner.lock().expect("updater mutex").status.clone()
    }

    pub fn set_auto_update(&self, on: bool) -> Result<Status, String> {
        let p = self.prefs.patch(|p| p.auto_update = on)?;
        let mut g = self.inner.lock().expect("updater mutex");
        g.status.auto_update = p.auto_update;
        Ok(g.status.clone())
    }

    pub fn prefs(&self) -> prefs::Prefs {
        self.prefs.get()
    }

    pub fn set_auto_start_pref(&self, on: bool) -> Result<(), String> {
        self.prefs.patch(|p| p.auto_start = on).map(|_| ())
    }

    fn set_phase(&self, phase: Phase, error: Option<String>) {
        let mut g = self.inner.lock().expect("updater mutex");
        g.status.phase = phase;
        g.status.error = error;
    }

    /// 设置页手动点检查: 只探测, 即使开了自动安装也不下包, 让 UI 先问一句.
    pub async fn check(&self, app: &AppHandle) -> Result<Status, String> {
        self.execute(app, false, false).await
    }

    pub async fn run(&self, app: &AppHandle, force_install: bool) -> Result<Status, String> {
        self.execute(app, true, force_install).await
    }

    async fn execute(
        &self,
        app: &AppHandle,
        may_install: bool,
        force_install: bool,
    ) -> Result<Status, String> {
        // debug 包没有签名产物, 去打 GitHub 只会在 UI 上留下假失败.
        if cfg!(debug_assertions) {
            if let Some(phase) = debug_manual_phase(may_install) {
                self.set_phase(phase, None);
            }
            return Ok(self.snapshot());
        }
        {
            let mut g = self.inner.lock().expect("updater mutex");
            if g.busy {
                return Ok(g.status.clone());
            }
            g.busy = true;
            g.status.phase = Phase::Checking;
            g.status.error = None;
            g.status.done = 0;
            g.status.total = None;
        }
        let result = self.run_inner(app, may_install, force_install).await;
        self.inner.lock().expect("updater mutex").busy = false;
        match result {
            Ok(()) => Ok(self.snapshot()),
            Err(e) => {
                self.set_phase(Phase::Error, Some(e.clone()));
                Err(e)
            }
        }
    }

    async fn run_inner(
        &self,
        app: &AppHandle,
        may_install: bool,
        force_install: bool,
    ) -> Result<(), String> {
        let endpoint = Url::parse(MANIFEST_URL).map_err(|e| e.to_string())?;
        let update = app
            .updater_builder()
            .endpoints(vec![endpoint])
            .map_err(|e| e.to_string())?
            .build()
            .map_err(|e| e.to_string())?
            .check()
            .await
            .map_err(|e| explain_check(e.to_string()))?;
        let Some(update) = update else {
            self.set_phase(Phase::UpToDate, None);
            return Ok(());
        };
        {
            let mut g = self.inner.lock().expect("updater mutex");
            g.status.latest = Some(update.version.clone());
            g.status.phase = Phase::Available;
        }
        if !should_install(may_install, force_install, self.prefs().auto_update) {
            return Ok(());
        }
        self.set_phase(Phase::Downloading, None);
        let me = self.clone();
        let bytes = update
            .download(
                move |chunk, total| {
                    let mut g = me.inner.lock().expect("updater mutex");
                    g.status.done += chunk as u64;
                    g.status.total = total;
                    g.status.phase = Phase::Downloading;
                },
                || {},
            )
            .await
            .map_err(|e| e.to_string())?;

        self.set_phase(Phase::Waiting, None);
        self.wait_idle().await?;
        self.set_phase(Phase::Installing, None);
        update.install(bytes).map_err(|e| e.to_string())?;
        app.restart();
    }

    async fn wait_idle(&self) -> Result<(), String> {
        loop {
            let http = self.engine.list().map_err(|e| e.to_string())?;
            let bt = self.engine.list_torrents().map_err(|e| e.to_string())?;
            let http_busy = http.iter().any(|t| t.state.is_running());
            let bt_busy = bt.iter().any(|t| t.state.is_downloading());
            if !http_busy && !bt_busy {
                return Ok(());
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    }
}

/// 手动检查 (may_install=false) 必须停在 Available, 否则设置页确认框来不及出现.
fn should_install(may_install: bool, force_install: bool, auto_update: bool) -> bool {
    may_install && (auto_update || force_install)
}

/// debug 包不能打 GitHub, 手动检查用 UpToDate 让设置页能闪「已经是最新版了」.
fn debug_manual_phase(may_install: bool) -> Option<Phase> {
    if may_install {
        None
    } else {
        Some(Phase::UpToDate)
    }
}

/// 清单由本机 /api/updater-manifest 现查 GitHub API 拼出; 原文留给设置页复制.
fn explain_check(raw: String) -> String {
    format!("GitHub Releases API ({GH_LATEST}): {raw}")
}

pub fn spawn_loop(app: AppHandle) {

    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(8)).await;
        loop {
            if cfg!(debug_assertions) {
                return;
            }
            let Some(up) = app.try_state::<Updater>() else {
                return;
            };
            if up.prefs().auto_update {
                let _ = up.run(&app, false).await;
            }
            tokio::time::sleep(Duration::from_secs(6 * 3600)).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manual_check_never_installs() {
        // 自动安装开着也不能在「检查更新」里直接下包.
        assert!(!should_install(false, false, true));
        assert!(!should_install(false, false, false));
        assert!(!should_install(false, true, true));
    }

    #[test]
    fn auto_loop_follows_pref() {
        assert!(should_install(true, false, true));
        assert!(!should_install(true, false, false));
    }

    #[test]
    fn confirmed_upgrade_uses_force_install() {
        // 设置页确认 / 顶栏「立即更新」走同一条 force 路径.
        assert!(should_install(true, true, false));
        assert!(should_install(true, true, true));
    }

    #[test]
    fn debug_manual_check_fakes_up_to_date() {
        assert!(matches!(debug_manual_phase(false), Some(Phase::UpToDate)));
        assert!(debug_manual_phase(true).is_none());
    }

    #[test]
    fn ui_reads_snake_case_phases() {
        let tag = |p: Phase| serde_json::to_string(&p).unwrap();
        assert_eq!(tag(Phase::UpToDate), "\"up_to_date\"");
        assert_eq!(tag(Phase::Available), "\"available\"");
        assert_eq!(tag(Phase::Checking), "\"checking\"");
        assert_eq!(tag(Phase::Downloading), "\"downloading\"");
        assert_eq!(tag(Phase::Waiting), "\"waiting\"");
        assert_eq!(tag(Phase::Installing), "\"installing\"");
    }
}
