use crate::error::{CoreError, Result};
use crate::types::{RequestContext, SegmentInfo};
use crate::writer::TaskFile;
use futures_util::StreamExt;
use reqwest::header;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::watch;

/// 单个 chunk 的读超时: 防止连接假死拖住整段 (总超时不设, 大文件不可预估)
const STALL_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, PartialEq)]
pub(crate) enum SegOutcome {
    Complete,
    Canceled,
}

/// 按大小规划分段: 段数受 max_n 与 min_size 双重约束, 小文件自然退化为单段
pub(crate) fn plan_segments(size: u64, max_n: u32, min_size: u64) -> Vec<SegmentInfo> {
    let n = if size == 0 {
        1
    } else {
        (size / min_size).clamp(1, max_n as u64) as u32
    };
    let per = size / n as u64;
    (0..n)
        .map(|i| SegmentInfo {
            idx: i,
            start: i as u64 * per,
            end: if i == n - 1 { size } else { (i as u64 + 1) * per },
            done: 0,
        })
        .collect()
}

/// 把尚未下完的连续空洞按新连接数切开; 每段必须是文件上的连续区间 (pwrite 不能跨洞).
pub(crate) fn replan_remaining(
    segs: &[SegmentInfo],
    max_n: u32,
    min_size: u64,
) -> Vec<SegmentInfo> {
    let mut holes: Vec<(u64, u64)> = segs
        .iter()
        .filter(|s| s.end > s.start + s.done)
        .map(|s| (s.start + s.done, s.end))
        .collect();
    holes.sort_by_key(|h| h.0);
    let mut merged: Vec<(u64, u64)> = Vec::new();
    for (s, e) in holes {
        match merged.last_mut() {
            Some((_, end)) if s <= *end => *end = (*end).max(e),
            _ => merged.push((s, e)),
        }
    }
    if merged.is_empty() {
        return segs.to_vec();
    }
    let target = max_n.max(1) as usize;
    while merged.len() < target {
        let (i, len) = match merged
            .iter()
            .enumerate()
            .map(|(i, (s, e))| (i, e.saturating_sub(*s)))
            .max_by_key(|(_, l)| *l)
        {
            Some(v) => v,
            None => break,
        };
        if len < min_size.saturating_mul(2) {
            break;
        }
        let mid = merged[i].0 + len / 2;
        let (s, e) = merged[i];
        merged[i] = (s, mid);
        merged.insert(i + 1, (mid, e));
    }
    merged
        .into_iter()
        .enumerate()
        .map(|(i, (start, end))| SegmentInfo { idx: i as u32, start, end, done: 0 })
        .collect()
}

fn apply_ctx(mut req: reqwest::RequestBuilder, ctx: &RequestContext) -> reqwest::RequestBuilder {
    for (k, v) in &ctx.headers {
        req = req.header(k.as_str(), v.as_str());
    }
    req
}

/// 下载一个 Segment (end > 0), 断点从 start+done 续起.
/// 网络错误指数退避重试; 每次有实际进度则重置重试计数,
/// 因为"下了 900MB 后闪断"与"完全连不上"不该共享同一个失败预算.
pub(crate) async fn run_segment(
    client: reqwest::Client,
    url: String,
    ctx: RequestContext,
    seg: SegmentInfo,
    file: Arc<TaskFile>,
    done: Arc<AtomicU64>,
    mut cancel: watch::Receiver<bool>,
    retry_limit: u32,
) -> Result<SegOutcome> {
    let mut attempts: u32 = 0;
    loop {
        if *cancel.borrow() {
            return Ok(SegOutcome::Canceled);
        }
        let cur = done.load(Ordering::Relaxed);
        if seg.start + cur >= seg.end {
            return Ok(SegOutcome::Complete);
        }

        let before = cur;
        match stream_range(&client, &url, &ctx, &seg, &file, &done, &mut cancel).await {
            Ok(outcome) => return Ok(outcome),
            Err(e) => {
                if done.load(Ordering::Relaxed) > before {
                    attempts = 0;
                }
                attempts += 1;
                if attempts > retry_limit {
                    return Err(e);
                }
                let backoff = Duration::from_millis(500 * (1 << attempts.min(5)) as u64);
                tokio::select! {
                    _ = tokio::time::sleep(backoff) => {}
                    _ = cancel.changed() => return Ok(SegOutcome::Canceled),
                }
            }
        }
    }
}

async fn stream_range(
    client: &reqwest::Client,
    url: &str,
    ctx: &RequestContext,
    seg: &SegmentInfo,
    file: &TaskFile,
    done: &AtomicU64,
    cancel: &mut watch::Receiver<bool>,
) -> Result<SegOutcome> {
    let offset = seg.start + done.load(Ordering::Relaxed);
    let range = format!("bytes={}-{}", offset, seg.end - 1);
    let req = apply_ctx(client.get(url).header(header::RANGE, range), ctx);
    let resp = req.send().await?.error_for_status()?;
    // 多段模式必须拿到 206: 返回 200 意味着服务器忽略了 Range,
    // 若照单全收会把整个文件写进本段区间, 直接判错重试
    if resp.status() != reqwest::StatusCode::PARTIAL_CONTENT {
        return Err(CoreError::Other(format!(
            "服务器未按 Range 响应 (HTTP {}), 期望 206",
            resp.status()
        )));
    }

    let mut stream = resp.bytes_stream();
    let mut pos = offset;
    loop {
        let next = tokio::select! {
            biased;
            _ = cancel.changed() => return Ok(SegOutcome::Canceled),
            next = tokio::time::timeout(STALL_TIMEOUT, stream.next()) => next,
        };
        let chunk = match next {
            Err(_) => return Err(CoreError::Other("读取超时 (30s 无数据)".into())),
            Ok(None) => break,
            Ok(Some(c)) => c?,
        };
        // 服务器可能多给 (罕见), 截断到段边界防止越界写入相邻段
        let room = (seg.end - pos) as usize;
        let data = if chunk.len() > room { &chunk[..room] } else { &chunk[..] };
        file.write_at(pos, data)?;
        pos += data.len() as u64;
        done.fetch_add(data.len() as u64, Ordering::Relaxed);
        if pos >= seg.end {
            break;
        }
    }

    if pos >= seg.end {
        Ok(SegOutcome::Complete)
    } else {
        Err(CoreError::Other("连接提前结束".into()))
    }
}

/// 单连接流式下载: 服务器不支持 Range 或大小未知时的降级路径.
/// 无断点能力, 重试/恢复都从 0 开始 (done 会被重置).
pub(crate) async fn run_stream(
    client: reqwest::Client,
    url: String,
    ctx: RequestContext,
    file: Arc<TaskFile>,
    done: Arc<AtomicU64>,
    mut cancel: watch::Receiver<bool>,
) -> Result<SegOutcome> {
    done.store(0, Ordering::Relaxed);
    let req = apply_ctx(client.get(&url), &ctx);
    let resp = req.send().await?.error_for_status()?;
    let mut stream = resp.bytes_stream();
    let mut pos: u64 = 0;
    loop {
        let next = tokio::select! {
            biased;
            _ = cancel.changed() => return Ok(SegOutcome::Canceled),
            next = tokio::time::timeout(STALL_TIMEOUT, stream.next()) => next,
        };
        let chunk = match next {
            Err(_) => return Err(CoreError::Other("读取超时 (30s 无数据)".into())),
            Ok(None) => break,
            Ok(Some(c)) => c?,
        };
        file.write_at(pos, &chunk)?;
        pos += chunk.len() as u64;
        done.store(pos, Ordering::Relaxed);
    }
    Ok(SegOutcome::Complete)
}
