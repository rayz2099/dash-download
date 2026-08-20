# UI 采用 Tauri 桌面 app, 而非 daemon 内嵌 Web UI

产品定位是 NDM 式桌面下载器, 用户明确否决"浏览器里开管理页"的形态. 改为 Tauri 桌面 app: Rust 核心 (引擎 + 调度 + localhost API) 与 webview UI 同进程, 关窗缩到系统托盘, 下载不中断. ADR 0002 的"独立 daemon 进程"随之收敛为 app 内的常驻核心, 但"扩展作为 localhost API 客户端"(ADR 0003) 不变; 若未来需要无 GUI 运行, 核心 crate 仍可单独封装 headless binary.
