//! BT 进度采样. 从 bt.rs 拆出, 避免单文件过千行.

use crate::bt::BtSample;
use crate::engine::Inner;
use crate::torrent::{TorrentPeer, TorrentProgress, TorrentState};
use librqbit::{ManagedTorrent, PeerStatsFilter, PeerStatsFilterState};
use std::sync::Arc;

impl Inner {
    fn peer_list_of(h: &ManagedTorrent) -> Vec<TorrentPeer> {
        let Some(live) = h.live() else {
            return Vec::new();
        };
        let snap = live.per_peer_stats_snapshot(PeerStatsFilter {
            state: PeerStatsFilterState::All,
        });
        let mut out: Vec<TorrentPeer> = snap
            .peers
            .into_iter()
            .map(|(addr, st)| TorrentPeer {
                addr,
                client: st.client_name.unwrap_or_default(),
                state: st.state.to_string(),
                down: st.counters.fetched_bytes,
                up: st.counters.uploaded_bytes,
                kind: st.conn_kind.map(|k| k.to_string()).unwrap_or_default(),
                chunks: st.counters.fetched_chunks,
                pieces: st.counters.downloaded_and_checked_pieces,
                piece_ms: st.counters.total_piece_download_ms,
                conn_ms: st.counters.total_time_connecting_ms,
                attempts: st.counters.connection_attempts,
                errors: st.counters.errors,
                incoming: st.counters.incoming_connections > 0,
            })
            .collect();
        out.sort_by(|a, b| b.down.cmp(&a.down));
        out.truncate(80);
        out
    }

    /// 第二项 true 表示 Active 腾出了额度, 调用方必须 kick, 否则 queued 会卡住.
    pub(crate) fn sample_torrents(&self) -> (Vec<TorrentProgress>, bool) {
        let handles: Vec<(i64, Arc<ManagedTorrent>)> = {
            let g = self.bt.lock().unwrap();
            match g.as_ref() {
                Some(bt) => bt
                    .handles
                    .lock()
                    .unwrap()
                    .iter()
                    .map(|(k, v)| (*k, v.clone()))
                    .collect(),
                None => Vec::new(),
            }
        };
        let mut out = Vec::new();
        let mut kick = false;
        for (id, h) in handles {
            let st = h.stats();
            let down = st
                .live
                .as_ref()
                .map(|l| l.download_speed.as_bytes())
                .unwrap_or(0);
            let up = st
                .live
                .as_ref()
                .map(|l| l.upload_speed.as_bytes())
                .unwrap_or(0);
            let ps = st.live.as_ref().map(|l| &l.snapshot.peer_stats);
            let peers = ps.map(|p| p.live as u32).unwrap_or(0);
            let seen = ps.map(|p| p.seen as u32).unwrap_or(0);
            let connecting = ps.map(|p| p.connecting as u32).unwrap_or(0);
            let phase = st.state.to_string();
            let peer_list = Self::peer_list_of(&h);
            if let Some(bt) = self.bt.lock().unwrap().as_ref() {
                bt.speeds.lock().unwrap().insert(
                    id,
                    BtSample {
                        down,
                        up,
                        peers,
                        seen,
                        connecting,
                        done: st.progress_bytes,
                        phase: phase.clone(),
                        peer_list: peer_list.clone(),
                    },
                );
            }
            // 校验中的 progress_bytes 是已扫字节, 不能当下载进度落盘
            if phase != "initializing" {
                let _ = self.store.lock().unwrap().checkpoint_torrent(id, st.progress_bytes);
            }
            if st.finished {
                if let Ok(info) = self.torrent_info(id) {
                    if info.state == TorrentState::Active {
                        let _ = self.set_tstate(id, TorrentState::Seeding, "");
                        kick = true;
                    }
                }
            }
            if let Some(err) = st.error {
                if let Ok(info) = self.torrent_info(id) {
                    if info.state == TorrentState::Active {
                        let _ = self.set_tstate(id, TorrentState::Failed, &err);
                        kick = true;
                    }
                }
            }
            out.push(TorrentProgress {
                id,
                done: st.progress_bytes,
                speed: down,
                up_speed: up,
                peers,
                seen,
                connecting,
                phase,
                peer_list,
            });
        }
        (out, kick)
    }
}
