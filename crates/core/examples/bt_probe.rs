//! 对照 librqbit 默认 Session 与 dash-download 的 SessionOptions.
//! cargo run -p dd-core --example bt_probe -- /tmp/nezha.torrent

use librqbit::{
    AddTorrent, AddTorrentOptions, ConnectionOptions, DhtSessionConfig, ListenerOptions, Session,
    SessionOptions, TorrentStatsState,
};
use std::collections::HashSet;
use std::net::{Ipv6Addr, SocketAddr};
use std::path::PathBuf;
use std::time::{Duration, Instant};

const DHT_BOOTSTRAP: &[&str] = &[
    "dht.transmissionbt.com:6881",
    "87.98.162.88:6881",
    "dht.libtorrent.org:25401",
    "192.241.176.51:25401",
    "router.bittorrent.com:6881",
    "67.215.246.10:6881",
    "router.utorrent.com:6881",
    "82.221.103.244:6881",
];

fn extra_trackers() -> Vec<String> {
    vec![
        "http://tracker.opentrackr.org:1337/announce".into(),
        "https://tracker.opentrackr.org:443/announce".into(),
        "udp://tracker.opentrackr.org:1337/announce".into(),
        "udp://open.stealth.si:80/announce".into(),
        "udp://exodus.desync.com:6969/announce".into(),
        "udp://open.demonii.com:1337/announce".into(),
        "http://tracker.bt4g.com:2095/announce".into(),
        "http://tracker.renfei.net:8080/announce".into(),
        "https://tracker.nekomi.cn:443/announce".into(),
        "udp://93.158.213.92:1337/announce".into(),
    ]
}

async fn run(label: &str, session: std::sync::Arc<Session>, bytes: Vec<u8>) {
    println!("=== {label} add ===");
    let resp = session
        .add_torrent(
            AddTorrent::from_bytes(bytes),
            Some(AddTorrentOptions {
                paused: false,
                overwrite: true,
                trackers: Some(extra_trackers()),
                ..Default::default()
            }),
        )
        .await
        .expect("add");
    let h = resp.into_handle().expect("handle");
    let t0 = Instant::now();
    if let Err(e) = h.wait_until_initialized().await {
        println!("{label} init fail: {e}");
        return;
    }
    println!("{label} initialized in {:?}", t0.elapsed());
    for i in 0..20 {
        tokio::time::sleep(Duration::from_secs(1)).await;
        let st = h.stats();
        let live = st.live.as_ref();
        let down = live.map(|l| l.download_speed.as_bytes()).unwrap_or(0);
        let peers = live.map(|l| l.snapshot.peer_stats.live).unwrap_or(0);
        let seen = live.map(|l| l.snapshot.peer_stats.seen).unwrap_or(0);
        println!(
            "{label} t={i:02} state={:?} done={} peers_live={peers} seen={seen} speed={down}B/s finished={}",
            st.state,
            st.progress_bytes,
            st.finished
        );
        if st.finished || matches!(st.state, TorrentStatsState::Error) {
            if let Some(e) = st.error {
                println!("{label} error: {e}");
            }
            break;
        }
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                "librqbit=debug,librqbit_dht=info,librqbit_tracker_comms=debug".into()
            }),
        )
        .with_target(true)
        .init();

    let torrent = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "/tmp/nezha.torrent".into());
    let bytes = std::fs::read(&torrent).expect("torrent bytes");
    let mode = std::env::args().nth(2).unwrap_or_else(|| "ours".into());

    let dir = PathBuf::from(format!("/tmp/dd-bt-probe-{mode}"));
    let _ = std::fs::create_dir_all(&dir);

    match mode.as_str() {
        "default" => {
            let session = Session::new(dir).await.expect("session");
            run("default", session, bytes).await;
        }
        _ => {
            let mut listen = ListenerOptions::default();
            listen.listen_addr = SocketAddr::from((Ipv6Addr::UNSPECIFIED, 0));
            listen.enable_upnp_port_forwarding = false;
            let mut session_trackers = HashSet::new();
            for t in extra_trackers() {
                if let Ok(u) = url::Url::parse(&t) {
                    session_trackers.insert(u);
                }
            }
            let opts = SessionOptions {
                listen: Some(listen),
                connect: Some(ConnectionOptions::default()),
                fastresume: true,
                trackers: session_trackers,
                dht: Some(DhtSessionConfig {
                    bootstrap_addrs: Some(DHT_BOOTSTRAP.iter().map(|s| (*s).to_string()).collect()),
                    port: None,
                    persistence: None,
                }),
                disable_local_service_discovery: false,
                ..Default::default()
            };
            let session = Session::new_with_opts(dir, opts).await.expect("session");
            run("ours", session, bytes).await;
        }
    }
}
