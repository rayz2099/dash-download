use crate::error::Result;
use crate::types::RequestContext;
use reqwest::header;

#[derive(Debug, Clone)]
pub struct ProbeResult {
    pub final_url: String,
    /// None = 无 Content-Length (chunked 等), 只能单连接流式
    pub size: Option<u64>,
    /// 服务器是否接受 Range (决定能否分段与续传)
    pub resumable: bool,
    pub filename: String,
}

/// 用带 `Range: bytes=0-` 的 GET 探测服务器能力.
/// 不用 HEAD: 部分服务器对 HEAD 撒谎 (不回 Content-Length 或直接 405),
/// 而对 GET Range 的响应码 (206/200) 是最可靠的能力信号. 响应体直接丢弃.
pub async fn probe(
    client: &reqwest::Client,
    url: &str,
    ctx: &RequestContext,
) -> Result<ProbeResult> {
    let mut req = client.get(url).header(header::RANGE, "bytes=0-");
    for (k, v) in &ctx.headers {
        req = req.header(k.as_str(), v.as_str());
    }
    let resp = req.send().await?.error_for_status()?;

    let status = resp.status();
    let headers = resp.headers().clone();
    let final_url = resp.url().to_string();
    // 立即 drop resp 中断响应体传输, 探测只要头
    drop(resp);

    let resumable = status == reqwest::StatusCode::PARTIAL_CONTENT;
    // 206 时总大小以 Content-Range 的 "bytes 0-x/total" 为准,
    // Content-Length 只是本次响应的长度, 二者在 Range 请求下含义不同
    let size = if resumable {
        headers
            .get(header::CONTENT_RANGE)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.rsplit('/').next())
            .and_then(|v| v.parse::<u64>().ok())
    } else {
        headers
            .get(header::CONTENT_LENGTH)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<u64>().ok())
    };

    let filename = filename_from_disposition(&headers)
        .or_else(|| filename_from_url(&final_url))
        .unwrap_or_else(|| "download".to_string());

    Ok(ProbeResult { final_url, size, resumable, filename: sanitize(&filename) })
}

/// Content-Disposition 里的 filename*= (RFC 5987) 优先于 filename=
fn filename_from_disposition(headers: &header::HeaderMap) -> Option<String> {
    let cd = headers.get(header::CONTENT_DISPOSITION)?.to_str().ok()?;
    for part in cd.split(';') {
        let part = part.trim();
        if let Some(v) = part.strip_prefix("filename*=") {
            let v = v.trim_matches('"');
            // 形如 UTF-8''name.ext
            let enc = v.splitn(2, "''").nth(1).unwrap_or(v);
            if let Some(d) = percent_decode(enc) {
                if !d.is_empty() {
                    return Some(d);
                }
            }
        }
    }
    for part in cd.split(';') {
        let part = part.trim();
        if let Some(v) = part.strip_prefix("filename=") {
            let v = v.trim_matches('"').trim();
            if !v.is_empty() {
                return Some(v.to_string());
            }
        }
    }
    None
}

fn filename_from_url(u: &str) -> Option<String> {
    let parsed = url::Url::parse(u).ok()?;
    let seg = parsed.path_segments()?.filter(|s| !s.is_empty()).last()?;
    let name = percent_decode(seg).unwrap_or_else(|| seg.to_string());
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

fn percent_decode(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok()?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// 去掉路径分隔符等危险字符, 防止服务器指定的文件名逃出下载目录
fn sanitize(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| if matches!(c, '/' | '\\' | '\0' | ':') { '_' } else { c })
        .collect();
    let trimmed = cleaned.trim().trim_start_matches('.').to_string();
    if trimmed.is_empty() {
        "download".to_string()
    } else {
        trimmed
    }
}
