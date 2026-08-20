use crate::error::Result;
use crate::types::{RequestContext, SegmentInfo, TaskInfo, TaskState};
use rusqlite::{params, Connection};
use std::path::Path;

/// SQLite 持久化: task 元数据 + segment checkpoint.
/// 写频率低 (状态变更 + 2s 一次进度 checkpoint), 同步接口足够, 上层短临界区调用.
pub struct Store {
    conn: Connection,
}

impl Store {
    pub fn open(path: &Path) -> Result<Store> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        // WAL: 崩溃安全且读写不互斥, 桌面单进程场景的默认最优解
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS task (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                url TEXT NOT NULL,
                final_url TEXT NOT NULL DEFAULT '',
                name TEXT NOT NULL DEFAULT '',
                dir TEXT NOT NULL,
                size INTEGER,
                resumable INTEGER NOT NULL DEFAULT 0,
                state TEXT NOT NULL,
                done INTEGER NOT NULL DEFAULT 0,
                error TEXT NOT NULL DEFAULT '',
                headers_json TEXT NOT NULL DEFAULT '[]',
                created_at INTEGER NOT NULL,
                completed_at INTEGER
            );
            CREATE TABLE IF NOT EXISTS segment (
                task_id INTEGER NOT NULL,
                idx INTEGER NOT NULL,
                start INTEGER NOT NULL,
                end INTEGER NOT NULL,
                done INTEGER NOT NULL DEFAULT 0,
                PRIMARY KEY (task_id, idx)
            );",
        )?;
        // 已有库补列: 已存在时 ALTER 失败, 忽略即可
        let _ = conn.execute(
            "ALTER TABLE task ADD COLUMN max_segments INTEGER NOT NULL DEFAULT 8",
            [],
        );
        Ok(Store { conn })
    }

    /// app 启动恢复: 上次进程退出时仍在跑的任务一律置为 Paused (ADR: 不自动续跑)
    pub fn recover_interrupted(&self) -> Result<()> {
        self.conn.execute(
            "UPDATE task SET state = 'paused' WHERE state IN ('active', 'probing')",
            [],
        )?;
        Ok(())
    }

    pub fn insert_task(
        &self,
        url: &str,
        dir: &str,
        name: &str,
        state: TaskState,
        ctx: &RequestContext,
        max_segments: u32,
    ) -> Result<i64> {
        let now = now_ts();
        self.conn.execute(
            "INSERT INTO task (url, final_url, name, dir, state, headers_json, created_at, max_segments)
             VALUES (?1, ?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                url,
                name,
                dir,
                state.as_str(),
                serde_json::to_string(&ctx.headers).unwrap_or_else(|_| "[]".into()),
                now,
                max_segments
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn set_max_segments(&self, id: i64, n: u32) -> Result<()> {
        self.conn.execute(
            "UPDATE task SET max_segments = ?2 WHERE id = ?1",
            params![id, n],
        )?;
        Ok(())
    }

    pub fn update_probe(
        &self,
        id: i64,
        final_url: &str,
        name: &str,
        size: Option<u64>,
        resumable: bool,
    ) -> Result<()> {
        self.conn.execute(
            "UPDATE task SET final_url = ?2, name = ?3, size = ?4, resumable = ?5 WHERE id = ?1",
            params![id, final_url, name, size.map(|s| s as i64), resumable as i64],
        )?;
        Ok(())
    }

    pub fn set_state(&self, id: i64, state: TaskState, error: &str) -> Result<()> {
        let completed = if state == TaskState::Completed { Some(now_ts()) } else { None };
        self.conn.execute(
            "UPDATE task SET state = ?2, error = ?3,
             completed_at = COALESCE(?4, completed_at) WHERE id = ?1",
            params![id, state.as_str(), error, completed],
        )?;
        Ok(())
    }

    pub fn set_name(&self, id: i64, name: &str) -> Result<()> {
        self.conn.execute("UPDATE task SET name = ?2 WHERE id = ?1", params![id, name])?;
        Ok(())
    }

    pub fn replace_segments(&self, id: i64, segs: &[SegmentInfo]) -> Result<()> {
        self.conn.execute("DELETE FROM segment WHERE task_id = ?1", params![id])?;
        for s in segs {
            self.conn.execute(
                "INSERT INTO segment (task_id, idx, start, end, done) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![id, s.idx, s.start as i64, s.end as i64, s.done as i64],
            )?;
        }
        Ok(())
    }

    /// 进度 checkpoint: 一个事务批量刷所有段偏移, crash 后从这里续传
    pub fn checkpoint(&mut self, id: i64, done: u64, seg_done: &[(u32, u64)]) -> Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute("UPDATE task SET done = ?2 WHERE id = ?1", params![id, done as i64])?;
        for (idx, d) in seg_done {
            tx.execute(
                "UPDATE segment SET done = ?3 WHERE task_id = ?1 AND idx = ?2",
                params![id, idx, *d as i64],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// 重新下载: 清空进度与分段, 保留 url/headers/dir/name (链接地址永久保留是产品要求).
    /// size/resumable 一并清掉, 由下次探测重新填 (源文件可能已更新)
    pub fn reset_task(&self, id: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE task SET done = 0, error = '', completed_at = NULL,
             size = NULL, resumable = 0 WHERE id = ?1",
            params![id],
        )?;
        self.conn.execute("DELETE FROM segment WHERE task_id = ?1", params![id])?;
        Ok(())
    }

    /// 调度器取号: 最早入队的 Queued 任务
    pub fn next_queued(&self) -> Result<Option<i64>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id FROM task WHERE state = 'queued' ORDER BY id ASC LIMIT 1")?;
        let id = stmt.query_row([], |r| r.get::<_, i64>(0));
        match id {
            Ok(id) => Ok(Some(id)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    pub fn delete_task(&self, id: i64) -> Result<()> {
        self.conn.execute("DELETE FROM segment WHERE task_id = ?1", params![id])?;
        self.conn.execute("DELETE FROM task WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn get_task(&self, id: i64) -> Result<Option<TaskInfo>> {
        let mut tasks = self.query_tasks(Some(id))?;
        Ok(tasks.pop())
    }

    pub fn list_tasks(&self) -> Result<Vec<TaskInfo>> {
        self.query_tasks(None)
    }

    fn query_tasks(&self, id: Option<i64>) -> Result<Vec<TaskInfo>> {
        let sql = format!(
            "SELECT id, url, final_url, name, dir, size, resumable, state, done, error,
                    headers_json, created_at, completed_at, max_segments
             FROM task {} ORDER BY id DESC",
            if id.is_some() { "WHERE id = ?1" } else { "" }
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let map_row = |row: &rusqlite::Row| -> rusqlite::Result<TaskInfo> {
            Ok(TaskInfo {
                id: row.get(0)?,
                url: row.get(1)?,
                final_url: row.get(2)?,
                name: row.get(3)?,
                dir: row.get(4)?,
                size: row.get::<_, Option<i64>>(5)?.map(|v| v as u64),
                resumable: row.get::<_, i64>(6)? != 0,
                state: TaskState::parse(&row.get::<_, String>(7)?),
                done: row.get::<_, i64>(8)? as u64,
                speed: 0,
                error: row.get(9)?,
                segments: Vec::new(),
                created_at: row.get(11)?,
                completed_at: row.get(12)?,
                max_segments: row.get::<_, i64>(13).unwrap_or(8) as u32,
            })
        };
        let mut tasks: Vec<TaskInfo> = if let Some(id) = id {
            stmt.query_map(params![id], map_row)?.collect::<rusqlite::Result<_>>()?
        } else {
            stmt.query_map([], map_row)?.collect::<rusqlite::Result<_>>()?
        };
        for t in &mut tasks {
            t.segments = self.load_segments(t.id)?;
        }
        Ok(tasks)
    }

    pub fn load_segments(&self, id: i64) -> Result<Vec<SegmentInfo>> {
        let mut stmt = self.conn.prepare(
            "SELECT idx, start, end, done FROM segment WHERE task_id = ?1 ORDER BY idx",
        )?;
        let segs = stmt
            .query_map(params![id], |row| {
                Ok(SegmentInfo {
                    idx: row.get(0)?,
                    start: row.get::<_, i64>(1)? as u64,
                    end: row.get::<_, i64>(2)? as u64,
                    done: row.get::<_, i64>(3)? as u64,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(segs)
    }

    pub fn load_ctx(&self, id: i64) -> Result<RequestContext> {
        let json: String = self.conn.query_row(
            "SELECT headers_json FROM task WHERE id = ?1",
            params![id],
            |r| r.get(0),
        )?;
        Ok(RequestContext { headers: serde_json::from_str(&json).unwrap_or_default() })
    }
}

pub fn now_ts() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
