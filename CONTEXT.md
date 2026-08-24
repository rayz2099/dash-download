# dash-download

跨平台下载管理器, 重写 Neat Download Manager. 形态为 Tauri 桌面 app: Rust 常驻核心 (引擎+调度+localhost API) + webview UI, Chrome 扩展作为 localhost API 客户端. Core 说 http/https 与 BitTorrent (Magnet / .torrent); 网盘 Provider 不在本阶段.

## Language

**Core**:
app 内常驻的 Rust 核心, 承载下载引擎、任务调度与 localhost API, 是系统唯一的状态持有者; 关窗后随托盘进程存活.
_Avoid_: Daemon, Server, 后台程序

**Task**:
一次完整的 HTTP 文件下载单元, 由 URL、目标路径与请求上下文构成, 拥有生命周期状态. 不是 Torrent.
_Avoid_: Download, Job

**Segment**:
Task 内按字节区间切分的并行下载单元, 对应一个 HTTP Range 连接; 设计上支持下载中再切分 (动态分段). 不是 Piece.
_Avoid_: Chunk, Part, Thread

**Queue**:
控制 Task 与 Torrent 并发与执行顺序的调度容器. HTTP 与 BT 下载各一份额度; 做种不占 BT 下载额度. 「全部暂停」含 Active 与 Seeding; 「全部开始」只恢复 Paused, 不动 AwaitingSelection.
_Avoid_: Scheduler

**Remove**:
从表拿掉一行 Task 或 Torrent. 必须二次确认, 确认框带「删除本地文件」, 默认勾选. 勾选则清该行在磁盘上的全部产物: HTTP 的 `.ddown` 与成品, Torrent 的文件. 不勾则只从表移除, 残片/成品都留.
_Avoid_: delete, 卸载, 取消

**Takeover**:
Chrome 扩展把本该由浏览器或系统处理的下载意图转交 Core. HTTP 文件必须携带 Request Context; `magnet:` 点击与 `.torrent` 字节直接交给 Core, 不先走 HTTP Task.
_Avoid_: 拦截, Capture

**Request Context**:
发起下载所需的 HTTP 上下文 (cookies, referer, user-agent 等自定义 header), 随 Task 持久化.
_Avoid_: Headers

**Magnet**:
BitTorrent 磁力链接, 用 Infohash 标识一份 Torrent, 自身不含文件列表与分片哈希, 必须先 Resolve 成 Metainfo.
_Avoid_: 磁力 URL (它不是 HTTP URL)

**Metainfo**:
`.torrent` 字节或 Magnet Resolve 得到的 info 字典: 文件列表, Piece 布局与哈希, tracker. 入队前必须先有它.
_Avoid_: torrent 文件 (那是磁盘上的一种获得方式), 种子

**Resolve**:
把 Magnet 或 `.torrent` 字节变成 Metainfo 的过程 (Magnet 走 BEP-9). 对应 HTTP 的探测, 但探测一词只用于 HTTP.
_Avoid_: Probe, 解析 (太泛)

**Torrent**:
Queue 里与 Task 并列的调度单元, 对应一份 Infohash 的下载与做种, 带 File Selection. 表里同一个 Infohash 只能有一行. 不是 Task, 也不是 Metainfo. 状态自有, 不复用 Task 的状态枚举: Resolving → AwaitingSelection → Queued → Active ⇄ Paused; 选中文件齐了进入 Seeding ⇄ Paused; Failed 为失败态.
_Avoid_: 种子任务, BT Task, 下载

**Resolving**:
Torrent 内部态: 正在拉 Metainfo, 不占下载额度, **不出现在下载列表**. 解析成功才 TorrentAdded; 失败则删掉该行并报错.
_Avoid_: Probing, 解析中 (那是 UI 文案)

**AwaitingSelection**:
Metainfo 已到, 等用户做 File Selection. 不占下载额度. 单文件种子跳过此态, 直接 Queued.
_Avoid_: 待确认

**Seeding**:
File Selection 内 Piece 已齐, 仍在向 Peer 上传. 不是 Completed, 不占 BT 下载额度.
_Avoid_: 已完成 (HTTP Task 的终态), 做种中 (UI 文案)

**Infohash**:
Torrent 的身份, v1 为 info 字典的 SHA-1. Magnet 的 `xt=urn:btih:` 就是它.
_Avoid_: hash, torrent id

**File Selection**:
一份 Torrent 里真正要写到磁盘的文件子集. 未选中的文件不下. Active / Paused / Seeding 期间可改: 新勾上的开始拉, 取消的停请求, 已写下的字节留到 Remove.
_Avoid_: only_files (那是 librqbit 的字段名), 勾选

**Piece**:
Torrent 载荷按固定大小切开并带哈希的校验单元. 一个 Piece 可以跨越两个文件. 不是 Segment.
_Avoid_: Chunk, Block (Block 是 Piece 内 16KiB 请求单元)

**Peer**:
swarm 里的一个远程 BitTorrent 客户端.
_Avoid_: 节点 (DHT node 是路由表条目, 不是下载对端), 连接

**Tracker**:
可选的 Peer 发现服务器, HTTP 或 UDP. 不是下载源.
_Avoid_: 服务器, 加速器
