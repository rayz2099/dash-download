# BT 未完成文件用最终名, 不套 .ddown

ADR 0004 的 `*.ddown` + 预分配 pwrite 是 HTTP Range 模型. Torrent 多文件且 Piece 跨文件, 硬套 .ddown 等于重写 librqbit 磁盘层. 未完成的 BT 文件直接用最终文件名, 续传靠 bitfield, 完成态靠我们的状态机而不是扩展名. `.ddown` 只属于 HTTP Task.
