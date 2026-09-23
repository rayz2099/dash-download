//! Download a sniffed HLS/DASH manifest through the same queue as the desktop app.
use dd_core::{media::MediaOptions, AddTaskOptions, Engine, EngineConfig, TaskState};
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let url = args.next().ok_or("usage: media URL OUTPUT_DIRECTORY")?;
    let dir = std::path::PathBuf::from(args.next().ok_or("missing output directory")?);
    std::fs::create_dir_all(&dir)?;
    let engine = Engine::new(EngineConfig::new(dir.join("tasks.sqlite"), dir)).await?;
    let task = engine.add_media(
        &url,
        AddTaskOptions {
            name: Some("video".into()),
            ..Default::default()
        },
        MediaOptions {
            container: Default::default(),
            format: "bestvideo+bestaudio/best".into(),
        },
    )?;
    loop {
        let task = engine.task(task.id)?;
        match task.state {
            TaskState::Completed => {
                println!("{}", task.final_path().display());
                return Ok(());
            }
            TaskState::Failed => return Err(task.error.into()),
            _ => tokio::time::sleep(std::time::Duration::from_millis(250)).await,
        }
    }
}
