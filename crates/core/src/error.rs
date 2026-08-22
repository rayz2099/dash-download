#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error("http: {0}")]
    Http(#[from] reqwest::Error),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("db: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("任务不存在: {0}")]
    NotFound(i64),
    /// 探测拿到 4xx/5xx, 状态码要落库才能在失败详情里回看
    #[error("探测 HTTP {0}")]
    ProbeHttp(u16),
    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, CoreError>;
