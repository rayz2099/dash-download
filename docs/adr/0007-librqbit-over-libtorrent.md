# BT 引擎用 librqbit, 不用 libtorrent

Magnet/Torrent 要嵌进现有 Rust Core. libtorrent (Rasterbar) 是吞吐与 BEP 完整度的行业默认, 但把 C++/Boost 打进 Tauri 三端 CI, 等于第二条工具链, 和 ADR 0001 的「Rust 单二进制、MB 级常驻」冲突; 可用的 Rust binding 也没人养. 选择 librqbit: 同栈 (Rust/tokio, Apache-2.0), Magnet/DHT/PEX/LSD/uTP/UPnP/选文件都有. 接受它这一期的缺口: 无 WebSeed (BEP-17/19), Piece picker 只有顺序下载. 加速只做公开机制: 入站监听 + UPnP, DHT+PEX+LSD (`private=1` 强制关), 种子自带 tracker, 完成后继续做种直到暂停/删除; 附加公共 tracker 列表默认关. 冷门种明显慢于 qBittorrent 时再评估换引擎或补 rarest-first, 不在这一期赌 C++.
