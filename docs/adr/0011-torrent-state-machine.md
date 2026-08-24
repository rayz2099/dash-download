# Torrent 自有状态机, 做种不是 Completed

HTTP Task 的 Completed 是终态, 不再占网. Torrent 完成后还要做种, 且做种不占下载额度, 所以 Seeding 是独立状态, 禁止复用 TaskState. Magnet 先在内部 Resolving (拉 Metainfo), 成功才进入下载列表 (AwaitingSelection / Queued); 失败删行, 不把半截 infohash 丢进表. 多条 magnet 仍可并行 Resolve, 只是列表里看不见. Paused 恢复时按 bitfield 是否已齐决定回到 Active 还是 Seeding. 同一 Infohash 只允许一行, 重复添加聚焦已有行并把 tracker 并入.
