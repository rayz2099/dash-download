## 1.2.3 - 2026-08-24

### 修复
- macOS `.app` 打 bundle 级 ad-hoc 签 (`signingIdentity: "-"`). 之前只有 linker 签, 下载后 Gatekeeper 报已损坏

## 1.2.2 - 2026-08-24

### 修复
- Release 不再上传 `latest.json`, 1.2.1+ 客户端走 GitHub Releases API 升级
- 资产改名改用 draft 的 `release_id`, 不再打 `/releases/tags/{tag}` 吃 404

## 1.2.1 - 2026-08-24

### 修复
- Linux/Windows 打包不再因 macOS 专用 `RunEvent::Reopen` 失败
- 新版本自动更新改走 GitHub Releases API, 不再依赖 `latest.json`
- Release 有签名包才转正, 避免打到一半抢 latest 导致旧版检查 404

## 1.2.0 - 2026-08-24

### 新功能
- 磁力 / .torrent 下载 (librqbit), 解析后选文件
- 设置增加 P2P 类目, 默认关闭; 公共 tracker 优先 XIU2, ngosang 作补集
- 磁力解析走 itorrents.net HTTP 缓存, 不开 P2P 也能出文件列表
- 多文件种子包一层种子名目录, 单文件直写下到保存目录
- 扩展接管磁力点击和 `.torrent` 下载
- 数值框可手填, 目录可浏览

### 修复
- 选文件 PATCH 被 CORS 拦下, WKWebView 报 TypeError: Load failed
- 点开始下载后种子一直排队: session 未就绪不再丢任务, 下完也会出队
- 重复粘贴同一磁力不再静默结束, 未选文件会再次弹出列表
- itorrents 缓存跳到别的 infohash 当未命中, 改走 DHT
- 崩溃恢复把种子停成暂停, 不再自动续跑
- `private=1` 的种子不再向公共 tracker announce
- 磁力点击只有接管成功才拦截跳转
- 打开已删除的文件提示「文件不存在」, 不再跳到目录
- DHT bootstrap 去掉失效的 bitcomet, 补 IP 节点

## 1.1.0 - 2026-08-22

### 新功能
- 增加应用内设置, 开机启动和 GitHub 更新检查
- 通过 page hook 和扩展策略接管 blob/data 下载

### 修复
- 加固 native host 拉起, takeover 和 probe 诊断
- 拒绝超过 24MB 的 blob/data 导入, 并停止写入页面 takeover 标记
