# 预分配单文件 + 定位写, 而非分段 .part 合并

下载开始时按 Content-Length 预分配目标文件 (下载中命名 `*.ddown`, 完成后 rename), 各 Segment 通过 pwrite 定位写入自己的字节区间. 拒绝"每段独立 .part 最后合并"方案: 大文件收尾时会产生一次全量拷贝 IO, 没有对应收益. Segment 已确认偏移定期 checkpoint 到 SQLite, crash 后据此续传.
