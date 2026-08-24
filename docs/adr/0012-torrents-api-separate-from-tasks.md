# Torrent 走独立 /api/torrents, 不塞进 /api/tasks

ADR 0008 已经把 Torrent 和 Task 分成两个聚合. HTTP 客户端按 Task 的 `http_status` / `segments` / Request Context 读行; magnet 塞进 `/api/tasks` 会让旧扩展和 UI 吃到空字段. 因此 REST 按聚合切开: `/api/torrents` 负责 Resolve、File Selection、暂停、Remove. 列表 GET 两份, 混排是 UI 投影. WS 仍一条连接, 事件用 `task_*` / `torrent_*` tag 分流; `EngineEvent` 保持 HTTP-only. 扩展 Takeover magnet / `.torrent` 走 `POST /api/torrents`.
