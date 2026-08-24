# 磁力/种子走本地 BitTorrent, 不做厂商协议逆向

本阶段要支持 Magnet 与 `.torrent`. 拒绝两条捷径: (1) 迅雷/百度等云盘离线把磁力转成 HTTP 再走现有引擎; (2) 逆向迅雷私有加速协议或复用其账号/登录态. 理由: 用户明确禁止逆向; 非官方 API 易挂、会封号, 且与公开 Apache-2.0 仓库不匹配. Core 对 BT 说标准 BEP; 现有 HTTP Task/Segment/Range 路径不动. 加速只考虑公开机制 (DHT, PEX, LSD, tracker, web seed, 入站监听/UPnP), 不接任何未文档化的厂商 overlay. 网盘 Provider 整枝推迟.
