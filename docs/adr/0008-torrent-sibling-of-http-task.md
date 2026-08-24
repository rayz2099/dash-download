# Torrent 与 HTTP Task 是两个聚合, 不共用一张语义

BitTorrent 的暂停、哈希、Peer 集合、File Selection 都是 per-infohash, Piece 还跨文件. 若把 magnet 塞进现有 Task.url、废弃 Segment, HTTP 的 Request Context / http_status / Range 列全部变成空壳, 状态机也会被做种拖脏. 若展开成 N 条文件 Task, 又无法表达跨文件 Piece. 因此 Torrent 是 Queue 里与 Task 并列的调度单元; HTTP Task/Segment/ADR 0004 一个字节都不为 BT 改语义. UI 可以一张表混排, 那是投影. 入队前必须先 Resolve 出 Metainfo 再让用户做 File Selection (单文件跳过).
