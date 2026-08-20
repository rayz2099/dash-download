//! dd-core: 下载引擎纯库.
//! 不绑定任何 UI/RPC 框架, 上层 (Tauri app / CLI) 通过 [`Engine`] 与事件流交互.

mod engine;
mod error;
mod probe;
mod runner;
mod settings;
mod store;
mod types;
mod writer;

pub use engine::{Engine, EngineConfig};
pub use error::{CoreError, Result};
pub use probe::ProbeResult;
pub use settings::{EngineSettings, ProxyCfg, ProxyKind, ProxyProbe, MAX_CONN};
pub use types::{
    AddTaskOptions, EngineEvent, RequestContext, SegmentInfo, TaskInfo, TaskProgress, TaskState,
};
