//! DHT bootstrap 与 HTTP 种子缓存. 从 bt.rs 拆出, 避免单文件过千行.

use std::time::Duration;

/// 域名 + IP 双写: 国内 DNS 经常把 router.bitcomet.com 一类直接 NXDOMAIN.
/// 不放已失效主机 (silotis.us NXDOMAIN), 否则 librqbit-dht 会 WARN 重试一整天.
pub(crate) const DHT_BOOTSTRAP: &[&str] = &[
    "dht.transmissionbt.com:6881",
    "87.98.162.88:6881",
    "dht.libtorrent.org:25401",
    "192.241.176.51:25401",
    "router.bittorrent.com:6881",
    "67.215.246.10:6881",
    "router.utorrent.com:6881",
    "82.221.103.244:6881",
];

/// HTTP 种子缓存. DHT 在国内经常空转, 命中就能跳过 ut_metadata.
/// itorrents.org 会 301 到 http://itorrents.net, reqwest 默认不跟 HTTPS→HTTP, 所以直接打 .net.
fn meta_cache_urls(hash: &str) -> Vec<String> {
    let up = hash.to_uppercase();
    let low = hash.to_lowercase();
    vec![
        format!("https://itorrents.net/torrent/{up}.torrent"),
        format!("https://itorrents.net/torrent/{low}.torrent"),
        format!("https://itorrents.org/torrent/{up}.torrent"),
    ]
}

/// itorrents 会 302 到 webtor 的另一个种子, 必须对 infohash, 不能只看 HTTP 200.
pub(crate) fn cache_hash_ok(bytes: &[u8], expect: &str) -> bool {
    librqbit::torrent_from_bytes(bytes)
        .ok()
        .map(|p| p.info_hash.as_string().eq_ignore_ascii_case(expect))
        .unwrap_or(false)
}

async fn fetch_one(client: &reqwest::Client, url: String, expect: String) -> Option<Vec<u8>> {
    let send = client
        .get(&url)
        .header("accept", "application/x-bittorrent,*/*")
        .send();
    let resp = match tokio::time::timeout(Duration::from_secs(6), send).await {
        Ok(Ok(r)) if r.status().is_success() => r,
        Ok(Ok(r)) => {
            tracing::debug!(url = %url, status = %r.status(), "缓存未命中");
            return None;
        }
        Ok(Err(e)) => {
            tracing::debug!(url = %url, error = %e, "缓存请求失败");
            return None;
        }
        Err(_) => {
            tracing::debug!(url = %url, "缓存超时");
            return None;
        }
    };
    let bytes = resp.bytes().await.ok()?;
    if bytes.len() < 64 {
        return None;
    }
    if !cache_hash_ok(&bytes, &expect) {
        tracing::debug!(
            url = %url,
            n = bytes.len(),
            expect = %expect,
            "缓存 infohash 对不上, 当未命中"
        );
        return None;
    }
    Some(bytes.to_vec())
}

pub(crate) async fn fetch_meta_http(client: &reqwest::Client, hash: &str) -> Option<Vec<u8>> {
    let mut set = tokio::task::JoinSet::new();
    for url in meta_cache_urls(hash) {
        let c = client.clone();
        let expect = hash.to_string();
        set.spawn(async move { fetch_one(&c, url, expect).await });
    }
    while let Some(joined) = set.join_next().await {
        if let Ok(Some(bytes)) = joined {
            set.abort_all();
            return Some(bytes);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Store;
    use crate::torrent::{files_from_list, TorrentState};
    use crate::engine::{Engine, EngineConfig};
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    #[test]
    fn cache_hash_rejects_garbage() {
        assert!(!cache_hash_ok(b"not a torrent", "F4F4B2530901B9EC81AE9CBEA124A21A82826BFA"));
        assert!(!cache_hash_ok(b"", "aa"));
    }

    /// 用户复现用的磁力. 走外网, 默认 ignore.
    #[tokio::test]
    #[ignore = "network"]
    async fn http_cache_sample_magnet() {
        let client = reqwest::Client::builder()
            .user_agent("dash-download/test")
            .connect_timeout(Duration::from_secs(8))
            .build()
            .expect("client");
        let hash = "185385DBC430A8731A9659F2D52E7EA55766C76D";
        let bytes = fetch_meta_http(&client, hash)
            .await
            .expect("itorrents.net 应命中");
        let parsed = librqbit::torrent_from_bytes(&bytes).expect("torrent");
        let validated = parsed.info.data.clone().validate().expect("meta");
        let files = files_from_list(&validated).expect("paths");
        assert!(
            files.len() >= 2,
            "应有多文件列表, 实际 {} {:?}",
            files.len(),
            files.iter().map(|f| &f.path).collect::<Vec<_>>()
        );
        let name = validated.name().map(|s| s.into_owned()).unwrap_or_default();
        assert!(!name.is_empty(), "name empty");
        eprintln!("ok name={name} files={}", files.len());
        for f in &files {
            eprintln!("  {}  {}", f.size, f.path);
        }
    }

    /// 回归: MutexGuard 活到语句末, filter_map 里再 lock store 会卡住 Engine::new.
    #[tokio::test]
    async fn engine_new_with_resolving_magnet_returns() {
        let n = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("dd-resolve-{}-{n}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let hash = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        {
            let store = Store::open(&dir.join("t.db")).unwrap();
            store
                .insert_torrent(
                    hash,
                    &format!("magnet:?xt=urn:btih:{hash}"),
                    "x",
                    &dir.join("dl").to_string_lossy(),
                    TorrentState::Resolving,
                    &[],
                )
                .unwrap();
        }
        let eng = tokio::time::timeout(
            Duration::from_secs(3),
            Engine::new(EngineConfig::new(dir.join("t.db"), dir.join("dl"))),
        )
        .await
        .expect("Engine::new 死锁: resume_pending_resolve 重入 store 锁")
        .unwrap();
        assert!(eng.list_torrents().unwrap().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
