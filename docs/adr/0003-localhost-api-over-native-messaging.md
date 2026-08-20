# 扩展通信采用 localhost HTTP/WS 而非 Native Messaging

daemon 暴露 localhost REST + WebSocket, 扩展与 Web UI 复用同一套 API. 拒绝 Chrome Native Messaging: 它要求为每个浏览器注册 native host manifest, 且是 stdio 单连接模型, 无法与其他客户端共用. 放弃的能力是"扩展自动拉起 daemon 进程", v1 由 popup 提示手动启动, 后续如需要可单独补一个最小 native host 只做拉起.

## 鉴权 (2026-08: 去掉配对 token)

NDM 没有 pairing token; app 在跑, 扩展就能接管. 我们对齐这个交互.

绑定仍是 `127.0.0.1` only. 浏览器 CSRF 靠:

1. CORS Origin 白名单 (`chrome-extension://*`, Tauri / vite localhost)
2. 控制面强制自定义头 `x-dd-client`, 让跨站无法发 simple request
3. WS 握手校验 `Origin`; 无 Origin (curl) 放行

任意本机进程仍可打回环口, 与 NDM 同一威胁模型. 不做跨设备, 不做公网暴露.
