//! Discover signed updater packages via GitHub API, with a public release-page fallback.

use serde::Deserialize;
use serde_json::{json, Map, Value};
use std::time::Duration;

pub const GH_LATEST: &str =
    "https://api.github.com/repos/rayz2099/dash-download/releases/latest";


#[derive(Debug, Deserialize)]
pub struct GhAsset {
    pub name: String,
    pub browser_download_url: String,
}

#[derive(Debug, Deserialize)]
pub struct GhRelease {
    pub tag_name: String,
    pub body: Option<String>,
    pub published_at: Option<String>,
    pub assets: Vec<GhAsset>,
}

fn is_darwin_pkg(name: &str) -> bool {
    name.ends_with(".app.tar.gz")
}

fn is_linux_pkg(name: &str) -> bool {
    name.ends_with(".AppImage")
}

fn is_windows_pkg(name: &str) -> bool {
    name.ends_with("x64-setup.exe") || name.ends_with("win-x64.exe")
}

/// (tauri target, 安装包, 对应 .sig)
pub fn match_platforms(assets: &[GhAsset]) -> Vec<(&'static str, &GhAsset, &GhAsset)> {
    let rules: &[(&str, fn(&str) -> bool)] = &[
        ("darwin-aarch64", is_darwin_pkg),
        ("linux-x86_64", is_linux_pkg),
        ("windows-x86_64", is_windows_pkg),
    ];
    let mut out = Vec::new();
    for (key, is_pkg) in rules {
        let Some(file) = assets.iter().find(|a| is_pkg(&a.name)) else {
            continue;
        };
        let sig_name = format!("{}.sig", file.name);
        let Some(sig) = assets.iter().find(|a| a.name == sig_name) else {
            continue;
        };
        out.push((*key, file, sig));
    }
    out
}

pub fn version_from_tag(tag: &str) -> String {
    tag.trim().trim_start_matches('v').to_string()
}

fn http_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .user_agent(format!("dash-download/{}", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|e| e.to_string())
}

async fn fetch_latest(client: &reqwest::Client) -> Result<GhRelease, String> {
    fetch_latest_from(client, GH_LATEST, "https://github.com/rayz2099/dash-download/releases/latest").await
}

async fn fetch_latest_from(client: &reqwest::Client, api_url: &str, web_latest: &str) -> Result<GhRelease, String> {
    match fetch_latest_api(client, api_url).await {
        Ok(release) => Ok(release),
        Err(api_error) => fetch_public_release(client, web_latest).await.map_err(|web_error| {
            format!("{api_error}; GitHub Release 备用查询失败: {web_error}。可手动下载: {web_latest}")
        }),
    }
}

async fn fetch_latest_api(client: &reqwest::Client, api_url: &str) -> Result<GhRelease, String> {
    let resp = client
        .get(api_url)
        .header("accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| format!("GitHub API {api_url}: {e}"))?;
    if !resp.status().is_success() {
        let limited = resp.headers().get("x-ratelimit-remaining").is_some_and(|v| v == "0")
            || resp.status() == reqwest::StatusCode::TOO_MANY_REQUESTS;
        return Err(format!("GitHub API {api_url} HTTP {}{}", resp.status(),
            if limited { " (匿名请求额度已用尽)" } else { "" }));
    }
    let text = resp.text().await.map_err(|e| e.to_string())?;
    serde_json::from_str(&text).map_err(|e| format!("GitHub API JSON: {e}"))
}

/// The public release website has a separate request budget from api.github.com.
/// Resolve its latest redirect, then read only that release's downloadable assets.
/// Installation still requires the matching .sig and the bundled updater public key.
async fn fetch_public_release(client: &reqwest::Client, latest: &str) -> Result<GhRelease, String> {
    let base = url::Url::parse(latest).map_err(|e| e.to_string())?;
    let releases = base.path().strip_suffix("/latest").ok_or("无效的 Release 地址")?;
    let response = client.get(latest).send().await.map_err(|e| e.to_string())?;
    if !response.status().is_success() { return Err(format!("Release 页面 HTTP {}", response.status())); }
    let final_url = response.url();
    if final_url.origin() != base.origin() { return Err("Release 页面重定向到了其他站点".into()); }
    let prefix = format!("{releases}/tag/");
    let tag = final_url.path().strip_prefix(&prefix).ok_or("无法从 Release 页面确定版本")?;
    if tag.is_empty() || !tag.chars().all(|c| c.is_ascii_alphanumeric() || ".-_+".contains(c)) {
        return Err("Release tag 无效".into());
    }
    let tag = tag.to_owned();
    let notes = format!("Release notes: {final_url}");
    let mut assets_url = base.clone();
    assets_url.set_path(&format!("{releases}/expanded_assets/{tag}"));
    drop(response);
    let response = client.get(assets_url).send().await.map_err(|e| e.to_string())?;
    if !response.status().is_success() { return Err(format!("Release 资产列表 HTTP {}", response.status())); }
    let html = response.text().await.map_err(|e| e.to_string())?;
    let assets = public_assets(&base, &tag, &html)?;
    if match_platforms(&assets).is_empty() { return Err("Release 页面没有带 .sig 的安装包".into()); }
    Ok(GhRelease { tag_name: tag, body: Some(notes), published_at: None, assets })
}

fn public_assets(latest: &url::Url, tag: &str, html: &str) -> Result<Vec<GhAsset>, String> {
    let releases = latest.path().strip_suffix("/latest").ok_or("无效的 Release 地址")?;
    let prefix = format!("{releases}/download/{tag}/");
    let mut assets = Vec::<GhAsset>::new();
    // GitHub's expanded_assets fragment uses quoted, repository-relative hrefs.
    for quote in ['"', '\''] {
        let marker = format!("href={quote}");
        for piece in html.split(&marker).skip(1) {
            let Some((href, _)) = piece.split_once(quote) else { continue; };
            let Some(name) = href.strip_prefix(&prefix) else { continue; };
            if name.is_empty() || name.contains('/') || name.contains('?') || name.contains('#') { continue; }
            let asset_url = latest.join(href).map_err(|e| e.to_string())?;
            if asset_url.origin() != latest.origin() || !asset_url.path().starts_with(&prefix) { continue; }
            if assets.iter().any(|a| a.name == name) { continue; }
            assets.push(GhAsset { name: name.into(), browser_download_url: asset_url.to_string() });
        }
    }
    Ok(assets)
}

async fn fetch_sig(client: &reqwest::Client, url: &str) -> Result<String, String> {
    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("拉 .sig: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("拉 .sig HTTP {}", resp.status()));
    }
    let text = resp.text().await.map_err(|e| e.to_string())?;
    let sig = text.trim().to_string();
    if sig.is_empty() {
        return Err(".sig 为空".into());
    }
    Ok(sig)
}

/// Tauri plugin 要的静态清单. 安装包 URL 直接用 GitHub asset, 签名来自同名 .sig.
pub async fn tauri_manifest() -> Result<Value, String> {
    let client = http_client()?;
    let rel = fetch_latest(&client).await?;
    let version = version_from_tag(&rel.tag_name);
    if version.is_empty() {
        return Err("Release tag 空".into());
    }
    let pairs = match_platforms(&rel.assets);
    if pairs.is_empty() {
        return Err("GitHub Release 没有带 .sig 的安装包".into());
    }
    let mut platforms = Map::new();
    for (key, file, sig_asset) in pairs {
        let sig = fetch_sig(&client, &sig_asset.browser_download_url).await?;
        platforms.insert(
            key.to_string(),
            json!({
                "url": file.browser_download_url,
                "signature": sig,
            }),
        );
    }
    Ok(json!({
        "version": version,
        "notes": rel.body.unwrap_or_default(),
        "pub_date": rel.published_at,
        "platforms": platforms,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asset(name: &str) -> GhAsset {
        GhAsset {
            name: name.into(),
            browser_download_url: format!("https://example/{name}"),
        }
    }

    async fn release_server(api_status: u16) -> (String, tokio::task::JoinHandle<()>) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            while let Ok((mut stream, _)) = listener.accept().await {
                let mut buf = [0; 4096];
                let n = stream.read(&mut buf).await.unwrap();
                let request = String::from_utf8_lossy(&buf[..n]);
                let path = request.split_whitespace().nth(1).unwrap();
                let (status, extra, body) = match path {
                    "/api/latest" if api_status == 200 => (200, "", r#"{"tag_name":"v1.3.1","body":null,"published_at":null,"assets":[]}"#),
                    "/api/latest" => (api_status, "X-RateLimit-Remaining: 0\r\n", r#"{"message":"API rate limit exceeded"}"#),
                    "/owner/repo/releases/latest" => (302, "Location: /owner/repo/releases/tag/v1.3.1\r\n", ""),
                    "/owner/repo/releases/tag/v1.3.1" => (200, "", "Release page"),
                    "/owner/repo/releases/expanded_assets/v1.3.1" => (200, "", r#"<a href="/owner/repo/releases/download/v1.3.1/DashDownload-1.3.1-mac-arm64.app.tar.gz">App</a>
<a href="/owner/repo/releases/download/v1.3.1/DashDownload-1.3.1-mac-arm64.app.tar.gz.sig">Signature</a>
<a href="https://untrusted.test/payload.app.tar.gz">foreign asset</a>
<a href="/owner/repo/releases/download/v0.0.1/old.app.tar.gz">old release</a>"#),
                    _ => (404, "", "missing"),
                };
                let response = format!("HTTP/1.1 {status} Status\r\n{extra}Content-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
                stream.write_all(response.as_bytes()).await.unwrap();
            }
        });
        (base, server)
    }

    #[tokio::test]
    async fn rate_limited_api_falls_back_to_public_release_assets() {
        for status in [403, 429] {
            let (base, server) = release_server(status).await;
            let client = reqwest::Client::builder().no_proxy().build().unwrap();
            let release = fetch_latest_from(&client, &format!("{base}/api/latest"), &format!("{base}/owner/repo/releases/latest")).await;
            server.abort();
            let release = release.expect("API rate limit must not prevent update discovery");
            assert_eq!(release.tag_name, "v1.3.1");
            assert_eq!(release.assets.len(), 2);
            assert_eq!(match_platforms(&release.assets).len(), 1);
            assert!(release.assets.iter().all(|a| a.browser_download_url.starts_with(&base)));
        }
    }

    #[tokio::test]
    async fn healthy_api_does_not_need_web_fallback() {
        let (base, server) = release_server(200).await;
        let client = reqwest::Client::builder().no_proxy().build().unwrap();
        let release = fetch_latest_from(&client, &format!("{base}/api/latest"), "http://127.0.0.1:1/unavailable").await.unwrap();
        server.abort();
        assert_eq!(release.tag_name, "v1.3.1");
    }

    #[tokio::test]
    async fn failed_api_and_fallback_report_both_errors_and_manual_link() {
        let (base, server) = release_server(403).await;
        let client = reqwest::Client::builder().no_proxy().build().unwrap();
        let error = fetch_latest_from(&client, &format!("{base}/api/latest"), &format!("{base}/missing/latest")).await.unwrap_err();
        server.abort();
        assert!(error.contains("403") && error.contains("404"), "{error}");
        assert!(error.contains("匿名请求额度已用尽") && error.contains("可手动下载"), "{error}");
    }

    #[test]
    fn tag_strips_v() {
        assert_eq!(version_from_tag("v1.2.0"), "1.2.0");
        assert_eq!(version_from_tag("1.2.0"), "1.2.0");
    }

    #[test]
    fn matches_tauri_default_and_normalized_names() {
        let assets = vec![
            asset("Dash.Download_aarch64.app.tar.gz"),
            asset("Dash.Download_aarch64.app.tar.gz.sig"),
            asset("Dash.Download_1.2.0_amd64.AppImage"),
            asset("Dash.Download_1.2.0_amd64.AppImage.sig"),
            asset("DashDownload-1.2.0-win-x64.exe"),
            asset("DashDownload-1.2.0-win-x64.exe.sig"),
            asset("dash-download-chrome-v1.2.0.zip"),
        ];
        let keys: Vec<_> = match_platforms(&assets).into_iter().map(|(k, _, _)| k).collect();
        assert_eq!(keys, ["darwin-aarch64", "linux-x86_64", "windows-x86_64"]);
    }

    #[test]
    fn skips_platform_without_sig() {
        let assets = vec![
            asset("Dash.Download_aarch64.app.tar.gz"),
            asset("foo.AppImage"),
        ];
        assert!(match_platforms(&assets).is_empty());
    }

    #[tokio::test]
    #[ignore = "network"]
    async fn github_latest_has_signed_packages() {
        let v = tauri_manifest().await.expect("GitHub latest");
        assert!(v["version"].as_str().unwrap().chars().next().unwrap().is_ascii_digit());
        let plats = v["platforms"].as_object().expect("platforms");
        assert!(plats.contains_key("darwin-aarch64"), "{plats:?}");
    }

    #[test]
    fn release_config_allows_insecure_http_for_ephemeral_manifest() {
        let conf: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).expect("tauri.conf.json");
        let up = &conf["plugins"]["updater"];
        assert_eq!(
            up["dangerousInsecureTransportProtocol"].as_bool(),
            Some(true),
            "检查更新时短暂 loopback http 必须显式放行, 否则 release 启动即 panic"
        );
    }

    #[test]
    fn windows_accepts_nsis_setup_name() {
        let assets = vec![
            asset("Dash.Download_1.2.0_x64-setup.exe"),
            asset("Dash.Download_1.2.0_x64-setup.exe.sig"),
        ];
        let hit = match_platforms(&assets);
        assert_eq!(hit.len(), 1);
        assert_eq!(hit[0].0, "windows-x86_64");
    }
}
