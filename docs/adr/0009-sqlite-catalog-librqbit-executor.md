# Torrent 身份在 SQLite, librqbit 只当执行器

Core 是唯一状态持有者. 若 Torrent 列表只活在 librqbit session 目录, 我们的 list/pause/恢复会和 HTTP Task 分叉, 重启对账也没有单一权威. SQLite 存 Torrent 目录 (id, Infohash, magnet/.torrent 来源, 目录, File Selection, 状态); librqbit 只持有 Piece bitfield / fastresume. 启动时按 SQLite 再 add 进 Session. 崩溃时仍在跑的 Torrent 与 HTTP 一样恢复成 Paused, 不自动续跑.
