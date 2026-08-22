use serde::{Deserialize, Serialize};

/// Task 生命周期状态. 转移见 docs/adr 与 runner:
/// Queued → Probing → Active ⇄ Paused; Completed/Failed 为终态.
/// Canceled 停跑但保留断点, Resume 回到 Queued.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskState {
    Queued,
    Probing,
    Active,
    Paused,
    Completed,
    Failed,
    Canceled,
}

impl TaskState {
    pub fn as_str(&self) -> &'static str {
        match self {
            TaskState::Queued => "queued",
            TaskState::Probing => "probing",
            TaskState::Active => "active",
            TaskState::Paused => "paused",
            TaskState::Completed => "completed",
            TaskState::Failed => "failed",
            TaskState::Canceled => "canceled",
        }
    }

    pub fn parse(s: &str) -> TaskState {
        match s {
            "queued" => TaskState::Queued,
            "probing" => TaskState::Probing,
            "active" => TaskState::Active,
            "paused" => TaskState::Paused,
            "completed" => TaskState::Completed,
            "failed" => TaskState::Failed,
            _ => TaskState::Canceled,
        }
    }

    /// 是否占用并发额度 (调度器只统计这两态)
    pub fn is_running(&self) -> bool {
        matches!(self, TaskState::Probing | TaskState::Active)
    }
}

/// 发起下载所需的 HTTP 上下文, 由扩展接管时携带 (cookies/referer/UA 均为普通 header).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RequestContext {
    pub headers: Vec<(String, String)>,
}

/// Segment: 按字节区间切分的并行下载单元. end 为开区间;
/// end == 0 表示大小未知的单连接流式下载 (不可分段).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SegmentInfo {
    pub idx: u32,
    pub start: u64,
    pub end: u64,
    pub done: u64,
}

impl SegmentInfo {
    pub fn remaining(&self) -> u64 {
        if self.end == 0 {
            u64::MAX
        } else {
            (self.end - self.start).saturating_sub(self.done)
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskInfo {
    pub id: i64,
    pub url: String,
    pub final_url: String,
    pub name: String,
    pub dir: String,
    /// None = 服务器未给 Content-Length
    pub size: Option<u64>,
    pub resumable: bool,
    /// 最近一次探测的 HTTP 状态码, 0 表示还没探测
    #[serde(default)]
    pub http_status: u16,
    /// 发了 Range 却拿到 200: 服务器忽略 Range
    #[serde(default)]
    pub range_ignored: bool,
    pub state: TaskState,
    pub done: u64,
    /// 瞬时速度 bytes/s, 仅内存值不落库
    pub speed: u64,
    pub error: String,
    pub segments: Vec<SegmentInfo>,
    /// 该 Task 的最大连接数, 规划分段时使用
    pub max_segments: u32,
    pub created_at: i64,
    pub completed_at: Option<i64>,
}

impl TaskInfo {
    /// 下载中的临时文件路径 (完成后 rename 去掉 .ddown 后缀)
    pub fn part_path(&self) -> std::path::PathBuf {
        std::path::Path::new(&self.dir).join(format!("{}.ddown", self.name))
    }

    pub fn final_path(&self) -> std::path::PathBuf {
        std::path::Path::new(&self.dir).join(&self.name)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AddTaskOptions {
    /// 保存目录, 空则用引擎默认目录
    pub dir: Option<String>,
    /// 文件名, 空则探测时从 Content-Disposition/URL 推断
    pub name: Option<String>,
    /// 最大分段数, 空则用引擎默认值
    pub segments: Option<u32>,
    /// true 则只入队不立即开始
    pub queue_only: bool,
    #[serde(default)]
    pub ctx: RequestContext,
}

/// 进度快照, 高频广播用 (状态变更走 TaskUpdated)
#[derive(Debug, Clone, Serialize)]
pub struct TaskProgress {
    pub id: i64,
    pub done: u64,
    pub speed: u64,
    pub seg_done: Vec<u64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EngineEvent {
    TaskAdded { task: TaskInfo },
    TaskUpdated { task: TaskInfo },
    TaskRemoved { id: i64 },
    Progress { tasks: Vec<TaskProgress> },
}
