# daemon + 薄客户端架构

引擎与调度收敛在常驻本地的 daemon 中, Chrome 扩展/Web UI/CLI 一律作为其客户端 (aria2 模型), 而非 NDM 式单体 GUI 内嵌引擎. 原因: Chrome 扩展本质上必须跨进程与引擎通信, daemon 化让扩展通信、UI 崩溃不中断下载、后续多客户端 (托盘 GUI/远程控制) 都不需要改动引擎侧.
