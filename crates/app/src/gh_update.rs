//! 用 GitHub Releases API 拼 Tauri updater 清单, 不再依赖 CI 上传 latest.json.

use serde::Deserialize;
use serde_json::{json, Map, Value};
use std::time::Duration;

pub const GH_LATEST: &str =
    "https://api.github.com/repos/rayz2099/dash-download/releases/latest";
pub const MANIFEST_URL: &str = "http://127.0.0.1:41320/api/updater-manifest";

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
    let resp = client
        .get(GH_LATEST)
        .header("accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| format!("GitHub API {GH_LATEST}: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("GitHub API {GH_LATEST} HTTP {}", resp.status()));
    }
    let text = resp.text().await.map_err(|e| e.to_string())?;
    serde_json::from_str(&text).map_err(|e| format!("GitHub API JSON: {e}"))
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
