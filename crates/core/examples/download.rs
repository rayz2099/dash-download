//! 引擎冒烟验证: cargo run --example download -- <url> [dir]
//! 用国内测速地址验证多连接分段下载与进度采样.

use dd_core::{AddTaskOptions, Engine, EngineConfig, EngineEvent, TaskState};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let url = std::env::args().nth(1).expect("用法: download <url> [dir]");
    let dir = std::env::args().nth(2).unwrap_or_else(|| "/tmp/dd-test".to_string());

    let cfg = EngineConfig::new("/tmp/dd-test/dd.sqlite".into(), dir.clone().into());
    let engine = Engine::new(cfg)?;
    let mut events = engine.subscribe();

    let task = engine.add(&url, AddTaskOptions::default())?;
    println!("任务 #{} 已创建: {}", task.id, task.url);

    loop {
        match events.recv().await? {
            EngineEvent::TaskUpdated { task: t } if t.id == task.id => {
                println!(
                    "[{}] {} size={:?} resumable={} segs={} err={}",
                    t.state.as_str(),
                    t.name,
                    t.size,
                    t.resumable,
                    t.segments.len(),
                    t.error
                );
                match t.state {
                    TaskState::Completed => {
                        println!("完成 → {}/{}", t.dir, t.name);
                        return Ok(());
                    }
                    TaskState::Failed => {
                        eprintln!("失败: {}", t.error);
                        std::process::exit(1);
                    }
                    _ => {}
                }
            }
            EngineEvent::Progress { tasks } => {
                if let Some(p) = tasks.iter().find(|p| p.id == task.id) {
                    let mb = p.done as f64 / 1048576.0;
                    let spd = p.speed as f64 / 1048576.0;
                    println!(
                        "进度 {:.1} MB @ {:.1} MB/s  segs={:?}",
                        mb,
                        spd,
                        p.seg_done.iter().map(|d| d / 1048576).collect::<Vec<_>>()
                    );
                }
            }
            _ => {}
        }
    }
}
