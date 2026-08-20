# dash-download

跨平台下载管理器, 重写 Neat Download Manager. 形态为 Tauri 桌面 app: Rust 常驻核心 (引擎+调度+localhost API) + webview UI, Chrome 扩展作为 localhost API 客户端, v1 仅支持 http/https.

## Language

**Core**:
app 内常驻的 Rust 核心, 承载下载引擎、任务调度与 localhost API, 是系统唯一的状态持有者; 关窗后随托盘进程存活.
_Avoid_: Daemon, Server, 后台程序

**Task**:
一次完整的文件下载单元, 由 URL、目标路径与请求上下文构成, 拥有生命周期状态.
_Avoid_: Download, Job

**Segment**:
Task 内按字节区间切分的并行下载单元, 对应一个 HTTP Range 连接; 设计上支持下载中再切分 (动态分段).
_Avoid_: Chunk, Part, Thread

**Queue**:
控制 Task 并发数与执行顺序的调度容器.
_Avoid_: Scheduler

**Takeover**:
Chrome 扩展拦截浏览器原生下载并转交 daemon 的行为, 必须携带 cookies/referer/user-agent 等请求上下文.
_Avoid_: 拦截, Capture

**Request Context**:
发起下载所需的 HTTP 上下文 (cookies, referer, user-agent 等自定义 header), 随 Task 持久化.
_Avoid_: Headers
