//! 公共 tracker: 国内优先 XIU2/TrackersListCollection, ngosang 作补集.
//! 磁力经常没有 announce, 不补 tracker 只能干等 DHT.

use std::time::Duration;

/// XIU2 国内镜像优先. cf.trackerslist.com 走 Cloudflare.
const XIU2_URLS: &[&str] = &[
    "https://cf.trackerslist.com/best.txt",
    "https://bitbucket.org/xiu2/trackerslistcollection/raw/master/best.txt",
    "https://jsd.onmicrosoft.cn/gh/XIU2/TrackersListCollection/best.txt",
    "https://cdn.jsdelivr.net/gh/XIU2/TrackersListCollection/best.txt",
];

const NGOSANG_URLS: &[&str] = &[
    "https://cdn.jsdelivr.net/gh/ngosang/trackerslist@master/trackers_best.txt",
    "https://ngosang.github.io/trackerslist/trackers_best.txt",
];

/// HTTP 热路径. UDP tracker 在国内经常通, 但 magnet 无 announce 时 HTTP 更稳.
/// 实测 opentrackr HTTP 对该 infohash 能立刻回几十个 peer.
fn hot_http() -> Vec<String> {
    parse(
        "\
http://tracker.opentrackr.org:1337/announce
https://tracker.opentrackr.org:443/announce
http://tracker.bt4g.com:2095/announce
http://tracker.renfei.net:8080/announce
https://tracker.nekomi.cn:443/announce
https://tracker.zhuqiy.com:443/announce
http://ipv4announce.sktorrent.eu:6969/announce
http://nyaa.tracker.wf:7777/announce
",
    )
}

/// 编译期快照: HTTP 热路径在前, XIU2 best, ngosang 域名 + IP 补漏.
pub fn baked() -> Vec<String> {
    compose(
        parse(include_str!("trackers_best.txt")),
        parse(include_str!("trackers_ngosang.txt")),
    )
}

fn compose(primary: Vec<String>, extra: Vec<String>) -> Vec<String> {
    let mut list = hot_http();
    merge(&mut list, &primary);
    merge(&mut list, &extra);
    merge(&mut list, &parse(include_str!("trackers_best_ip.txt")));
    list
}

/// librqbit 的 magnet 路径忽略 AddTorrentOptions.trackers, 只能写进 magnet URL.
pub fn magnet_with_trackers(magnet: &str, trackers: &[String]) -> String {
    let mut s = magnet.trim().to_string();
    for t in trackers {
        if t.is_empty() {
            continue;
        }
        s.push_str("&tr=");
        s.extend(url::form_urlencoded::byte_serialize(t.as_bytes()));
    }
    s
}

pub fn parse(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in text.lines() {
        let t = line.trim();
        if t.is_empty() || t.starts_with('#') {
            continue;
        }
        // ws/i2p/ygg 这客户端不接
        if !(t.starts_with("udp://") || t.starts_with("http://") || t.starts_with("https://")) {
            continue;
        }
        if out.iter().any(|x| x == t) {
            continue;
        }
        out.push(t.to_string());
    }
    out
}

pub fn merge(dst: &mut Vec<String>, extra: &[String]) {
    for t in extra {
        if !dst.iter().any(|x| x == t) {
            dst.push(t.clone());
        }
    }
}

pub async fn refresh(client: &reqwest::Client) -> Option<Vec<String>> {
    if let Some(list) = pull(client, XIU2_URLS).await {
        tracing::info!(n = list.len(), src = "XIU2", "已刷新公共 tracker");
        return Some(compose(list, parse(include_str!("trackers_ngosang.txt"))));
    }
    if let Some(list) = pull(client, NGOSANG_URLS).await {
        tracing::info!(n = list.len(), src = "ngosang", "XIU2 不可达, 改用 ngosang");
        return Some(compose(parse(include_str!("trackers_best.txt")), list));
    }
    tracing::warn!("公共 tracker 刷新失败, 继续用内置 XIU2/ngosang");
    None
}

async fn pull(client: &reqwest::Client, urls: &[&str]) -> Option<Vec<String>> {
    let mut set = tokio::task::JoinSet::new();
    for url in urls {
        let c = client.clone();
        let url = (*url).to_string();
        set.spawn(async move { fetch_list(&c, url).await });
    }
    while let Some(joined) = set.join_next().await {
        if let Ok(Some(list)) = joined {
            set.abort_all();
            return Some(list);
        }
    }
    None
}

async fn fetch_list(client: &reqwest::Client, url: String) -> Option<Vec<String>> {
    let send = client.get(&url).header("accept", "text/plain,*/*").send();
    let resp = match tokio::time::timeout(Duration::from_secs(6), send).await {
        Ok(Ok(r)) if r.status().is_success() => r,
        Ok(Ok(r)) => {
            tracing::debug!(url = %url, status = %r.status(), "tracker 列表未拉到");
            return None;
        }
        Ok(Err(e)) => {
            tracing::debug!(url = %url, error = %e, "tracker 列表请求失败");
            return None;
        }
        Err(_) => {
            tracing::debug!(url = %url, "tracker 列表超时");
            return None;
        }
    };
    let text = resp.text().await.ok()?;
    let list = parse(&text);
    // 当日副本至少要有一批, 避免把 HTML 错误页写进内存
    if list.len() < 8 {
        tracing::debug!(url = %url, n = list.len(), "tracker 列表过短, 丢弃");
        return None;
    }
    Some(list)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn baked_starts_with_http_opentrackr() {
        let list = baked();
        assert!(
            list.iter().any(|t| t == "http://tracker.opentrackr.org:1337/announce"),
            "HTTP opentrackr 必须在内置列表"
        );
        assert!(list.iter().any(|t| t.starts_with("udp://")));
        assert!(list.len() >= 40);
    }

    #[tokio::test]
    async fn mse_loopback_exchange() {
        librqbit::mse_self_check()
            .await
            .expect("MSE initiate/receive 本机往返");
    }

    #[test]
    fn magnet_appends_tr() {
        let m = magnet_with_trackers(
            "magnet:?xt=urn:btih:8C9F4DB08497563EF6EB01CF81199F645DA0954B",
            &["http://tracker.opentrackr.org:1337/announce".into()],
        );
        assert!(m.contains("xt=urn:btih:8C9F4DB08497563EF6EB01CF81199F645DA0954B"));
        assert!(m.contains("&tr=http%3A%2F%2Ftracker.opentrackr.org%3A1337%2Fannounce"));
    }
}
