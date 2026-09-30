use crate::error::{CoreError, Result};
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
    /// Content-Type 的媒体类型，不含参数。调用方没给文件后缀时用来补上。
    pub content_type: String,
    pub http_status: u16,
    /// 带 Range 探测却拿到 200, 服务器忽略了 Range
    pub range_ignored: bool,
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
    let mut resp = req.send().await?;

    let status = resp.status();
    let headers = resp.headers().clone();
    let final_url = resp.url().to_string();
    // 只留文件头。小红书原图经常不给图片类型，JPEG/HEIC 只能从这里认。
    let mut prefix = [0u8; 32];
    let mut prefix_len = 0;
    if let Ok(Some(chunk)) = resp.chunk().await {
        let n = chunk.len().min(prefix.len());
        prefix[..n].copy_from_slice(&chunk[..n]);
        prefix_len = n;
    }
    drop(resp);

    if !status.is_success() {
        return Err(CoreError::ProbeHttp(status.as_u16()));
    }

    let resumable = status == reqwest::StatusCode::PARTIAL_CONTENT;
    let range_ignored = status == reqwest::StatusCode::OK;
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
    let mut content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    if opaque_type(&content_type) {
        if let Some(mime) = image_mime_from_magic(&prefix[..prefix_len]) {
            content_type = mime.to_string();
        }
    }

    Ok(ProbeResult {
        final_url,
        size,
        resumable,
        filename: sanitize(&filename),
        content_type,
        http_status: status.as_u16(),
        range_ignored,
    })
}

fn opaque_type(t: &str) -> bool {
    matches!(t, "" | "application/octet-stream" | "binary/octet-stream" | "application/binary")
}

/// 文件头对应的图片类型。认不出就返回 None，后缀由调用方决定。
pub(crate) fn image_mime_from_magic(buf: &[u8]) -> Option<&'static str> {
    if buf.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Some("image/jpeg");
    }
    if buf.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Some("image/png");
    }
    if buf.starts_with(b"GIF87a") || buf.starts_with(b"GIF89a") {
        return Some("image/gif");
    }
    if buf.len() >= 12 && buf.starts_with(b"RIFF") && &buf[8..12] == b"WEBP" {
        return Some("image/webp");
    }
    if buf.len() >= 12 && &buf[4..8] == b"ftyp" {
        let brand = &buf[8..12];
        if matches!(brand, b"avif" | b"avis") {
            return Some("image/avif");
        }
        if matches!(brand, b"heic" | b"heix" | b"hevc" | b"hevx" | b"heim" | b"heis" | b"mif1" | b"msf1") {
            return Some("image/heic");
        }
    }
    None
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
pub(crate) fn sanitize(name: &str) -> String {
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
