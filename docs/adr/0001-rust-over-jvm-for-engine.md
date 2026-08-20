# 引擎与 daemon 使用 Rust 而非 Kotlin/JVM

daemon 是常驻本地的桌面后台进程, 用户 (Kotlin 主场) 明确否决 JVM 方案: 常驻 ~100MB 级内存与携带 JRE 的分发体积对桌面工具不可接受. 选择 Rust: 单二进制分发, 常驻内存 MB 级, tokio 满足多连接分段下载的并发需求. 代价是放弃最熟悉的技术栈, 接受学习成本.
