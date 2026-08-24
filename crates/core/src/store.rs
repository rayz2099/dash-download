use crate::error::Result;
use crate::torrent::{TorrentFile, TorrentInfo, TorrentState};
use crate::types::{RequestContext, SegmentInfo, TaskInfo, TaskState};
use rusqlite::{params, Connection, OptionalExtension};
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
                http_status INTEGER NOT NULL DEFAULT 0,
                range_ignored INTEGER NOT NULL DEFAULT 0,
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
            );
            CREATE TABLE IF NOT EXISTS torrent (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                infohash TEXT NOT NULL UNIQUE,
                source TEXT NOT NULL,
                name TEXT NOT NULL DEFAULT '',
                dir TEXT NOT NULL,
                state TEXT NOT NULL,
                done INTEGER NOT NULL DEFAULT 0,
                size INTEGER,
                error TEXT NOT NULL DEFAULT '',
                files_json TEXT NOT NULL DEFAULT '[]',
                selected_json TEXT NOT NULL DEFAULT '[]',
                trackers_json TEXT NOT NULL DEFAULT '[]',
                metainfo BLOB,
                created_at INTEGER NOT NULL,
                completed_at INTEGER
            );",
        )?;
        // 已有库补列: 已存在时 ALTER 失败, 忽略即可
        let _ = conn.execute(
            "ALTER TABLE task ADD COLUMN max_segments INTEGER NOT NULL DEFAULT 8",
            [],
        );
        let _ = conn.execute(
            "ALTER TABLE task ADD COLUMN http_status INTEGER NOT NULL DEFAULT 0",
            [],
        );
        let _ = conn.execute(
            "ALTER TABLE task ADD COLUMN range_ignored INTEGER NOT NULL DEFAULT 0",
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
        // ADR 0009: 崩溃时仍在跑的 Torrent 与 HTTP 一样恢复成 Paused, 不自动续跑.
        self.conn.execute(
            "UPDATE torrent SET state = 'paused' WHERE state IN ('active', 'seeding')",
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
        http_status: u16,
        range_ignored: bool,
    ) -> Result<()> {
        self.conn.execute(
            "UPDATE task SET final_url = ?2, name = ?3, size = ?4, resumable = ?5,
             http_status = ?6, range_ignored = ?7 WHERE id = ?1",
            params![
                id,
                final_url,
                name,
                size.map(|s| s as i64),
                resumable as i64,
                http_status as i64,
                range_ignored as i64
            ],
        )?;
        Ok(())
    }

    /// 探测失败也要留下状态码, 否则 Failed 任务只剩 reqwest 英文长句
    pub fn save_http(&self, id: i64, status: u16, range_ignored: bool) -> Result<()> {
        self.conn.execute(
            "UPDATE task SET http_status = ?2, range_ignored = ?3 WHERE id = ?1",
            params![id, status as i64, range_ignored as i64],
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
             size = NULL, resumable = 0, http_status = 0, range_ignored = 0 WHERE id = ?1",
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
                    headers_json, created_at, completed_at, max_segments,
                    http_status, range_ignored
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
                http_status: row.get::<_, i64>(14)? as u16,
                range_ignored: row.get::<_, i64>(15)? != 0,
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

    pub fn insert_torrent(
        &self,
        infohash: &str,
        source: &str,
        name: &str,
        dir: &str,
        state: TorrentState,
        trackers: &[String],
    ) -> Result<i64> {
        let now = now_ts();
        self.conn.execute(
            "INSERT INTO torrent (infohash, source, name, dir, state, trackers_json, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                infohash,
                source,
                name,
                dir,
                state.as_str(),
                serde_json::to_string(trackers).unwrap_or_else(|_| "[]".into()),
                now
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn torrent_by_hash(&self, infohash: &str) -> Result<Option<TorrentInfo>> {
        let id: Option<i64> = self.conn.query_row(
            "SELECT id FROM torrent WHERE infohash = ?1",
            params![infohash],
            |r| r.get(0),
        ).optional()?;
        match id {
            Some(id) => self.get_torrent(id),
            None => Ok(None),
        }
    }

    pub fn merge_trackers(&self, id: i64, extra: &[String]) -> Result<()> {
        let raw: String = self.conn.query_row(
            "SELECT trackers_json FROM torrent WHERE id = ?1",
            params![id],
            |r| r.get(0),
        )?;
        let mut cur: Vec<String> = serde_json::from_str(&raw).unwrap_or_default();
        for t in extra {
            if !cur.iter().any(|x| x == t) {
                cur.push(t.clone());
            }
        }
        self.conn.execute(
            "UPDATE torrent SET trackers_json = ?2 WHERE id = ?1",
            params![id, serde_json::to_string(&cur).unwrap_or_else(|_| "[]".into())],
        )?;
        Ok(())
    }

    pub fn set_torrent_state(&self, id: i64, state: TorrentState, error: &str) -> Result<()> {
        let completed = if state == TorrentState::Seeding {
            Some(now_ts())
        } else {
            None
        };
        self.conn.execute(
            "UPDATE torrent SET state = ?2, error = ?3,
             completed_at = COALESCE(?4, completed_at) WHERE id = ?1",
            params![id, state.as_str(), error, completed],
        )?;
        Ok(())
    }

    pub fn set_torrent_meta(
        &self,
        id: i64,
        name: &str,
        files: &[TorrentFile],
        selected: &[u32],
        size: Option<u64>,
        metainfo: Option<&[u8]>,
    ) -> Result<()> {
        self.conn.execute(
            "UPDATE torrent SET name = ?2, files_json = ?3, selected_json = ?4, size = ?5, metainfo = ?6
             WHERE id = ?1",
            params![
                id,
                name,
                serde_json::to_string(files).unwrap_or_else(|_| "[]".into()),
                serde_json::to_string(selected).unwrap_or_else(|_| "[]".into()),
                size.map(|s| s as i64),
                metainfo
            ],
        )?;
        Ok(())
    }

    pub fn set_torrent_selected(&self, id: i64, selected: &[u32], size: Option<u64>) -> Result<()> {
        self.conn.execute(
            "UPDATE torrent SET selected_json = ?2, size = ?3 WHERE id = ?1",
            params![
                id,
                serde_json::to_string(selected).unwrap_or_else(|_| "[]".into()),
                size.map(|s| s as i64)
            ],
        )?;
        Ok(())
    }

    pub fn checkpoint_torrent(&self, id: i64, done: u64) -> Result<()> {
        self.conn.execute(
            "UPDATE torrent SET done = ?2 WHERE id = ?1",
            params![id, done as i64],
        )?;
        Ok(())
    }

    pub fn torrent_metainfo(&self, id: i64) -> Result<Option<Vec<u8>>> {
        let blob: Option<Vec<u8>> = self.conn.query_row(
            "SELECT metainfo FROM torrent WHERE id = ?1",
            params![id],
            |r| r.get(0),
        ).optional()?.flatten();
        Ok(blob)
    }

    pub fn replace_trackers(&self, id: i64, trackers: &[String]) -> Result<()> {
        self.conn.execute(
            "UPDATE torrent SET trackers_json = ?2 WHERE id = ?1",
            params![id, serde_json::to_string(trackers).unwrap_or_else(|_| "[]".into())],
        )?;
        Ok(())
    }

    pub fn torrent_trackers(&self, id: i64) -> Result<Vec<String>> {
        let raw: String = self.conn.query_row(
            "SELECT trackers_json FROM torrent WHERE id = ?1",
            params![id],
            |r| r.get(0),
        )?;
        Ok(serde_json::from_str(&raw).unwrap_or_default())
    }

    pub fn next_queued_torrent(&self) -> Result<Option<i64>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id FROM torrent WHERE state = 'queued' ORDER BY id ASC LIMIT 1")?;
        let id = stmt.query_row([], |r| r.get::<_, i64>(0));
        match id {
            Ok(id) => Ok(Some(id)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    pub fn count_torrent_state(&self, state: TorrentState) -> Result<usize> {
        let n: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM torrent WHERE state = ?1",
            params![state.as_str()],
            |r| r.get(0),
        )?;
        Ok(n as usize)
    }

    pub fn list_torrent_ids(&self, state: TorrentState) -> Result<Vec<i64>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id FROM torrent WHERE state = ?1 ORDER BY id ASC")?;
        let rows = stmt.query_map(params![state.as_str()], |r| r.get::<_, i64>(0))?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    pub fn list_resolving(&self) -> Result<Vec<i64>> {
        self.list_torrent_ids(TorrentState::Resolving)
    }

    /// 恢复解析必须一次查出 source: 持着 store 锁再 get_torrent 会自锁.
    pub fn list_resolving_sources(&self) -> Result<Vec<(i64, String)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, source FROM torrent WHERE state = 'resolving' ORDER BY id ASC")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    pub fn delete_torrent(&self, id: i64) -> Result<()> {
        self.conn.execute("DELETE FROM torrent WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn get_torrent(&self, id: i64) -> Result<Option<TorrentInfo>> {
        let mut rows = self.query_torrents(Some(id))?;
        Ok(rows.pop())
    }

    pub fn list_torrents(&self) -> Result<Vec<TorrentInfo>> {
        self.query_torrents(None)
    }

    fn query_torrents(&self, id: Option<i64>) -> Result<Vec<TorrentInfo>> {
        let sql = format!(
            "SELECT id, infohash, source, name, dir, state, done, size, error,
                    files_json, selected_json, created_at, completed_at
             FROM torrent {} ORDER BY id DESC",
            if id.is_some() { "WHERE id = ?1" } else { "" }
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let map_row = |row: &rusqlite::Row| -> rusqlite::Result<TorrentInfo> {
            let files: Vec<TorrentFile> =
                serde_json::from_str(&row.get::<_, String>(9)?).unwrap_or_default();
            let selected: Vec<u32> =
                serde_json::from_str(&row.get::<_, String>(10)?).unwrap_or_default();
            let files = files
                .into_iter()
                .map(|mut f| {
                    f.selected = selected.contains(&f.idx);
                    f
                })
                .collect();
            Ok(TorrentInfo {
                id: row.get(0)?,
                infohash: row.get(1)?,
                source: row.get(2)?,
                name: row.get(3)?,
                dir: row.get(4)?,
                state: TorrentState::parse(&row.get::<_, String>(5)?),
                done: row.get::<_, i64>(6)? as u64,
                size: row.get::<_, Option<i64>>(7)?.map(|v| v as u64),
                speed: 0,
                up_speed: 0,
                error: row.get(8)?,
                files,
                peers: 0,
                seen: 0,
                connecting: 0,
                peer_list: Vec::new(),
                phase: String::new(),
                bt_direct: false,
                created_at: row.get(11)?,
                completed_at: row.get(12)?,
            })
        };
        let rows: Vec<TorrentInfo> = if let Some(id) = id {
            stmt.query_map(params![id], map_row)?.collect::<rusqlite::Result<_>>()?
        } else {
            stmt.query_map([], map_row)?.collect::<rusqlite::Result<_>>()?
        };
        Ok(rows)
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn recover_interrupted_pauses_bt_active() {
        let n = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("dd-store-{}-{n}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = Store::open(&dir.join("t.db")).unwrap();
        let dest = dir.to_string_lossy();
        let id = store
            .insert_torrent(
                "aa".repeat(20).as_str(),
                "magnet:?xt=urn:btih:aa",
                "n",
                &dest,
                TorrentState::Active,
                &[],
            )
            .unwrap();
        store
            .insert_torrent(
                "bb".repeat(20).as_str(),
                "magnet:?xt=urn:btih:bb",
                "s",
                &dest,
                TorrentState::Seeding,
                &[],
            )
            .unwrap();
        store
            .insert_torrent(
                "cc".repeat(20).as_str(),
                "magnet:?xt=urn:btih:cc",
                "q",
                &dest,
                TorrentState::Queued,
                &[],
            )
            .unwrap();
        store.recover_interrupted().unwrap();
        assert_eq!(store.get_torrent(id).unwrap().unwrap().state, TorrentState::Paused);
        let rows = store.list_torrents().unwrap();
        let by_name = |n: &str| rows.iter().find(|t| t.name == n).unwrap().state;
        assert_eq!(by_name("n"), TorrentState::Paused);
        assert_eq!(by_name("s"), TorrentState::Paused);
        assert_eq!(by_name("q"), TorrentState::Queued);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
