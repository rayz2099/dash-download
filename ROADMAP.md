# Roadmap

当前里程碑: **v0.1 可用** (macOS arm, http/https, Chrome 扩展 Takeover). 下面按不可逆程度排序, 不做时间承诺.

## v0.1 — 已交付

- Rust Core: Range 探测, 多连接分段, SQLite checkpoint, 队列, 暂停/续传/重新下载
- Tauri macOS app: NDM 式表格 UI, 原生标题栏, 托盘常驻, localhost REST+WS
- Chrome MV3 扩展: 接管下载并携带 Request Context
- 链接地址随 Task 永久保留

## v0.2 — 引擎补齐

动态分段 (慢 Segment 再切) 是 NDM 的核心算法, 当前尾段拖尾就是它的缺位. 限速与代理同属引擎调度层, 一起做比后补便宜.

- 动态分段: Active 态按速度再切慢段, 段模型已预留
- 全局限速 / 单 Task 限速
- HTTP/SOCKS 代理
- 设置页可改并发数, 连接数, 默认目录 (现在写死 3 / 8)

## v0.3 — 三端分发

tag (`v*`) 会走 `.github/workflows/release.yml`: 分平台打 app 包 + Chrome zip, GitHub Release 只留最新 3 个. justfile 本地配方仍在. Linux/Windows 的 Tauri 壳与 macOS 公证还没在对应主机上跑通.

- Linux x86_64: deb 包, 托盘与文件管理器 (xdg-open)
- Windows x86_64: NSIS 安装包, 托盘, 资源管理器定位
- macOS 公证 (notarization), 否则 Gatekeeper 会拦
- 扩展商店包

## v1.0 — 浏览器闭环

- Native Messaging 最小 host + 开机自启: 扩展可拉起未运行的 app (ADR 0003 拉起面)
- Takeover 过滤规则可配 (域名黑名单, 体积阈值)
- 失败可诊断: 保留探测响应码 / Range 是否被忽略

## 明确不做 (直到有人推翻)

- HLS / DASH / FTP: 新协议, 与 v1 的 http/https 边界冲突
- 视频嗅探: 属于扩展能力膨胀, 不是下载引擎
- Kotlin/JVM 引擎: ADR 0001
- 浏览器内嵌管理页: ADR 0005
