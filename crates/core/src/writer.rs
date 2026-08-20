use crate::error::Result;
use std::fs::{File, OpenOptions};
use std::path::Path;

/// 预分配单文件 + 定位写 (见 ADR 0004).
/// 各 Segment 并发写各自的字节区间, 无需锁: pwrite 本身是位置无关的原子系统调用.
pub struct TaskFile {
    file: File,
}

impl TaskFile {
    /// 打开 (不存在则创建) 下载临时文件; size 已知时预分配, 避免稀疏文件碎片
    pub fn open(path: &Path, size: Option<u64>) -> Result<TaskFile> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let file = OpenOptions::new().read(true).write(true).create(true).open(path)?;
        if let Some(size) = size {
            if file.metadata()?.len() != size {
                file.set_len(size)?;
            }
        }
        Ok(TaskFile { file })
    }

    #[cfg(unix)]
    pub fn write_at(&self, offset: u64, buf: &[u8]) -> Result<()> {
        use std::os::unix::fs::FileExt;
        self.file.write_all_at(buf, offset)?;
        Ok(())
    }

    #[cfg(windows)]
    pub fn write_at(&self, offset: u64, buf: &[u8]) -> Result<()> {
        use std::os::windows::fs::FileExt;
        let mut written = 0usize;
        while written < buf.len() {
            let n = self.file.seek_write(&buf[written..], offset + written as u64)?;
            written += n;
        }
        Ok(())
    }

    pub fn sync(&self) -> Result<()> {
        self.file.sync_data()?;
        Ok(())
    }
}
