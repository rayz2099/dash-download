## 1.1.0 - 2026-08-22

### 新功能
- 增加应用内设置, 开机启动和 GitHub 更新检查
- 通过 page hook 和扩展策略接管 blob/data 下载

### 修复
- 加固 native host 拉起, takeover 和 probe 诊断
- 拒绝超过 24MB 的 blob/data 导入, 并停止写入页面 takeover 标记
