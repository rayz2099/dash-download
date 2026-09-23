//! Filesystem operations must not detach a live writer or mix two tasks' checkpoints.
use dd_core::{AddTaskOptions, Engine, EngineConfig, TaskInfo, TaskState};
use std::{
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};

struct Fixture {
    url: String,
    engine: Engine,
    _dir: tempfile::TempDir,
    server: tokio::task::JoinHandle<()>,
    active: Arc<AtomicUsize>,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.server.abort();
    }
}
impl Fixture {
    async fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let active = Arc::new(AtomicUsize::new(0));
        let connections = active.clone();
        let server = tokio::spawn(async move {
            while let Ok((mut stream, _)) = listener.accept().await {
                let connections = connections.clone();
                tokio::spawn(async move {
                    let mut raw = Vec::new();
                    loop {
                        let mut buf = [0; 1024];
                        let n = stream.read(&mut buf).await.unwrap_or(0);
                        if n == 0 {
                            return;
                        }
                        raw.extend_from_slice(&buf[..n]);
                        if raw.ends_with(b"\r\n\r\n") {
                            break;
                        }
                    }
                    let request = String::from_utf8_lossy(&raw).to_lowercase();
                    let size = 512 * 1024;
                    let range = request
                        .lines()
                        .find_map(|line| line.strip_prefix("range: bytes="));
                    let (start, end) = range
                        .map(|v| {
                            let (a, b) = v.split_once('-').unwrap();
                            (
                                a.parse::<usize>().unwrap(),
                                b.parse::<usize>().unwrap_or(size - 1),
                            )
                        })
                        .unwrap_or((0, size - 1));
                    let status = if range.is_some() {
                        "206 Partial Content"
                    } else {
                        "200 OK"
                    };
                    let headers = format!("HTTP/1.1 {status}\r\nContent-Length: {}\r\nAccept-Ranges: bytes\r\nContent-Range: bytes {start}-{end}/{size}\r\nConnection: close\r\n\r\n", end-start+1);
                    if stream.write_all(headers.as_bytes()).await.is_err()
                        || request.starts_with("head ")
                    {
                        return;
                    }
                    connections.fetch_add(1, Ordering::SeqCst);
                    let byte = if request.starts_with("get /a ") {
                        b'a'
                    } else {
                        b'b'
                    };
                    for offset in (start..=end).step_by(4096) {
                        if stream
                            .write_all(&vec![byte; 4096.min(end + 1 - offset)])
                            .await
                            .is_err()
                        {
                            break;
                        }
                        tokio::time::sleep(Duration::from_millis(8)).await;
                    }
                    connections.fetch_sub(1, Ordering::SeqCst);
                });
            }
        });
        let engine = Engine::new(EngineConfig::new(
            dir.path().join("db"),
            dir.path().join("downloads"),
        ))
        .await
        .unwrap();
        Self {
            url,
            engine,
            _dir: dir,
            server,
            active,
        }
    }
    fn add(&self, path: &str) -> TaskInfo {
        self.engine
            .add(
                &format!("{}/{path}", self.url),
                AddTaskOptions {
                    name: Some("same.bin".into()),
                    segments: Some(1),
                    ..Default::default()
                },
            )
            .unwrap()
    }
    async fn wait(&self, id: i64, predicate: impl Fn(&TaskInfo) -> bool) -> TaskInfo {
        tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                let task = self.engine.task(id).unwrap();
                if predicate(&task) {
                    return task;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("task transition timed out")
    }
}

#[tokio::test]
async fn finder_delete_stops_writer_and_resume_restarts_missing_partial() {
    let f = Fixture::new().await;
    let task = f.add("a");
    let active = f.wait(task.id, |t| t.done >= 16384).await;
    std::fs::remove_file(active.part_path()).unwrap();
    let failed = f.wait(task.id, |t| t.state == TaskState::Failed).await;
    assert!(failed.error.contains("外部删除"), "{}", failed.error);
    assert!(!failed.final_path().exists());
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(f.active.load(Ordering::SeqCst), 0);
    f.engine.resume(task.id).unwrap();
    let completed = f.wait(task.id, |t| t.state == TaskState::Completed).await;
    assert_eq!(
        std::fs::read(completed.final_path()).unwrap(),
        vec![b'a'; 512 * 1024]
    );
}

#[tokio::test]
async fn same_name_pause_resume_and_redownload_do_not_overwrite() {
    let f = Fixture::new().await;
    let a = f.add("a");
    let b = f.add("b");
    f.wait(a.id, |t| t.done >= 16384).await;
    f.engine.pause(a.id).unwrap();
    let paused = f.wait(a.id, |t| t.state == TaskState::Paused).await;
    assert!(paused.part_path().exists());
    f.engine.resume(a.id).unwrap();
    let a = f.wait(a.id, |t| t.state == TaskState::Completed).await;
    let b = f.wait(b.id, |t| t.state == TaskState::Completed).await;
    assert_ne!(a.final_path(), b.final_path());
    assert_eq!(
        std::fs::read(a.final_path()).unwrap(),
        vec![b'a'; 512 * 1024]
    );
    assert_eq!(
        std::fs::read(b.final_path()).unwrap(),
        vec![b'b'; 512 * 1024]
    );
    f.engine.redownload(a.id).await.unwrap();
    let again = f.wait(a.id, |t| t.state == TaskState::Completed).await;
    assert_ne!(again.final_path(), a.final_path());
    assert_eq!(
        std::fs::read(a.final_path()).unwrap(),
        vec![b'a'; 512 * 1024]
    );
    assert_eq!(
        std::fs::read(again.final_path()).unwrap(),
        vec![b'a'; 512 * 1024]
    );
}

#[tokio::test]
async fn removing_active_task_joins_writer_before_cleaning_files() {
    let f = Fixture::new().await;
    let task = f.add("a");
    let active = f.wait(task.id, |t| t.done >= 16384).await;
    f.engine.remove(task.id, true).await.unwrap();
    assert!(f.engine.task(task.id).is_err());
    assert!(!active.part_path().exists());
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(!active.part_path().exists());
    assert!(!active.final_path().exists());
    assert_eq!(f.active.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn removing_during_a_stalled_probe_does_not_wait_for_the_server() {
    let f = Fixture::new().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/stall", listener.local_addr().unwrap());
    let (accepted, ready) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut buf = [0; 4096];
        socket.read(&mut buf).await.unwrap();
        accepted.send(()).unwrap();
        socket.read(&mut buf).await.unwrap()
    });
    let task = f.engine.add(&url, AddTaskOptions::default()).unwrap();
    ready.await.unwrap();
    tokio::time::timeout(Duration::from_secs(1), f.engine.remove(task.id, true))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(1), server)
            .await
            .unwrap()
            .unwrap(),
        0
    );
}

#[tokio::test]
async fn redownload_joins_active_writer_before_reusing_task_partial() {
    let f = Fixture::new().await;
    let task = f.add("a");
    f.wait(task.id, |t| t.done >= 16384).await;
    f.engine.redownload(task.id).await.unwrap();
    let completed = f.wait(task.id, |t| t.state == TaskState::Completed).await;
    assert_eq!(
        std::fs::read(completed.final_path()).unwrap(),
        vec![b'a'; 512 * 1024]
    );
    assert!(!completed.part_path().exists());
}
