# 扩展通信采用 localhost HTTP/WS 而非 Native Messaging

daemon 暴露 localhost REST + WebSocket (token 鉴权), 扩展与 Web UI 复用同一套 API. 拒绝 Chrome Native Messaging: 它要求为每个浏览器注册 native host manifest, 且是 stdio 单连接模型, 无法与其他客户端共用. 放弃的能力是"扩展自动拉起 daemon 进程", v1 由 popup 提示手动启动, 后续如需要可单独补一个最小 native host 只做拉起.
