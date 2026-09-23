//! Real fixture test: generate media, serve it locally, download and decode the result.
//! Run after preparing tools: cargo test -p dd-core --test media_download -- --ignored
use dd_core::{media, AddTaskOptions, Engine, EngineConfig, RequestContext, TaskState};
use std::{
    io::{Read, Write},
    path::Path,
    process::Command,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

fn ffmpeg(dir: &Path, args: &[&str]) {
    let output = Command::new(media::tool("ffmpeg"))
        .current_dir(dir)
        .args(["-hide_banner", "-loglevel", "error", "-y"])
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[tokio::test]
#[ignore = "requires prepared media sidecars"]
async fn hls_and_dash_download_complete_audio_video() {
    let dir = std::env::temp_dir().join(format!("dd-media-test-{}", rand::random::<u64>()));
    std::fs::create_dir_all(&dir).unwrap();
    ffmpeg(
        &dir,
        &[
            "-f",
            "lavfi",
            "-i",
            "testsrc=size=160x90:rate=10",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:sample_rate=44100",
            "-t",
            "3",
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
            "-g",
            "10",
            "-c:a",
            "aac",
            "source.mp4",
        ],
    );
    ffmpeg(
        &dir,
        &[
            "-i",
            "source.mp4",
            "-an",
            "-c:v",
            "copy",
            "-hls_time",
            "1",
            "-hls_playlist_type",
            "vod",
            "video.m3u8",
        ],
    );
    ffmpeg(
        &dir,
        &[
            "-i",
            "source.mp4",
            "-vn",
            "-c:a",
            "copy",
            "-hls_time",
            "1",
            "-hls_playlist_type",
            "vod",
            "audio.m3u8",
        ],
    );
    ffmpeg(
        &dir,
        &[
            "-i",
            "source.mp4",
            "-map",
            "0",
            "-c",
            "copy",
            "-seg_duration",
            "1",
            "-f",
            "dash",
            "manifest.mpd",
        ],
    );
    std::fs::write(dir.join("master.m3u8"), "#EXTM3U\n#EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID=\"audio\",NAME=\"main\",DEFAULT=YES,AUTOSELECT=YES,URI=\"audio.m3u8\"\n#EXT-X-STREAM-INF:BANDWIDTH=200000,RESOLUTION=160x90,CODECS=\"avc1.64000a,mp4a.40.2\",AUDIO=\"audio\"\nvideo.m3u8\n").unwrap();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    listener.set_nonblocking(true).unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let stopped = stop.clone();
    let slow = Arc::new(AtomicBool::new(false));
    let slow_server = slow.clone();
    let root = dir.clone();
    let server = std::thread::spawn(move || {
        while !stopped.load(Ordering::Relaxed) {
            let Ok((mut stream, _)) = listener.accept() else {
                std::thread::sleep(Duration::from_millis(10));
                continue;
            };
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut raw = [0; 8192];
            let n = stream.read(&mut raw).unwrap_or(0);
            let req = String::from_utf8_lossy(&raw[..n]);
            let file = req
                .split_whitespace()
                .nth(1)
                .unwrap_or("/")
                .trim_start_matches('/');
            // Verify browser credentials reach playlist and fragment requests.
            let authorized = req.to_lowercase().contains("cookie: session=fixture");
            let data = std::fs::read(root.join(file)).ok();
            if let Some(data) = data.filter(|_| authorized) {
                if slow_server.load(Ordering::Relaxed) && (file.ends_with(".ts") || file.ends_with(".m4s")) {
                    std::thread::sleep(Duration::from_millis(350));
                }
                let mime = if file.ends_with("m3u8") {
                    "application/vnd.apple.mpegurl"
                } else if file.ends_with("mpd") {
                    "application/dash+xml"
                } else {
                    "application/octet-stream"
                };
                let _ = write!(stream, "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: {mime}\r\nConnection: close\r\n\r\n", data.len());
                let _ = stream.write_all(&data);
            } else {
                let _ = stream.write_all(
                    b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                );
            }
        }
    });
    let eng = Engine::new(EngineConfig::new(
        dir.join("tasks.sqlite"),
        dir.join("downloads"),
    ))
    .await
    .unwrap();
    let ctx = RequestContext {
        headers: vec![
            ("Cookie".into(), "session=fixture".into()),
            ("Referer".into(), format!("http://{address}/page")),
        ],
    };
    for (manifest, container) in [
        ("master.m3u8", media::Container::Mp4),
        ("manifest.mpd", media::Container::Mp4),
        ("master.m3u8", media::Container::Mkv),
        ("manifest.mpd", media::Container::Mkv),
    ] {
        let url = format!("http://{address}/{manifest}");
        let info = media::inspect(&url, &ctx).await.unwrap();
        assert!(!info["formats"].as_array().unwrap().is_empty());
        let task = eng
            .add_media(
                &url,
                AddTaskOptions {
                    name: Some("Fixture video".into()),
                    ctx: ctx.clone(),
                    ..Default::default()
                },
                media::MediaOptions {
                    container,
                    format: info["formats"][0]["format"].as_str().unwrap().into(),
                },
            )
            .unwrap();
        // A pause while probing must not be lost when the worker installs progress handles.
        eng.pause(task.id).unwrap();
        tokio::time::timeout(Duration::from_secs(10), async {
            while eng.task(task.id).unwrap().state != TaskState::Paused {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap();
        eng.resume(task.id).unwrap();
        let completed = tokio::time::timeout(Duration::from_secs(120), async {
            loop {
                let task = eng.task(task.id).unwrap();
                if task.state == TaskState::Completed {
                    break task;
                }
                assert_ne!(task.state, TaskState::Failed, "{}", task.error);
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        })
        .await
        .unwrap();
        assert!(completed.done > 0);
        assert_eq!(
            completed.final_path().extension().unwrap(),
            container.extension()
        );
        let output = Command::new(media::tool("ffmpeg"))
            .args(["-hide_banner", "-i"])
            .arg(completed.final_path())
            .args(["-f", "null", "-"])
            .output()
            .unwrap();
        assert!(output.status.success());
        let log = String::from_utf8_lossy(&output.stderr);
        assert!(log.contains("Video:") && log.contains("Audio:"), "{log}");
    }
    // Exercise an actual yt-dlp process while it is writing fragments.
    slow.store(true, Ordering::Relaxed);
    for external_delete in [true, false] {
        let task = eng.add_media(&format!("http://{address}/master.m3u8"), AddTaskOptions {
            name: Some("Lifecycle video".into()), ctx: ctx.clone(), ..Default::default()
        }, serde_json::from_str("{}").unwrap()).unwrap();
        let work = task.part_path();
        tokio::time::timeout(Duration::from_secs(15), async {
            loop {
                let writing = std::fs::read_dir(&work).ok().into_iter().flatten().flatten()
                    .any(|e| e.file_name().to_string_lossy().ends_with(".part"));
                if writing { break; }
                let state = eng.task(task.id).unwrap();
                assert_ne!(state.state, TaskState::Failed, "{}", state.error);
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        }).await.unwrap();
        if external_delete {
            std::fs::remove_dir_all(&work).unwrap();
            let failed = tokio::time::timeout(Duration::from_secs(10), async {
                loop {
                    let current = eng.task(task.id).unwrap();
                    if current.state == TaskState::Failed { break current; }
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
            }).await.unwrap();
            assert!(failed.error.contains("外部删除"), "{}", failed.error);
            tokio::time::sleep(Duration::from_millis(500)).await;
            assert!(!work.exists(), "subprocess recreated deleted media data");
            eng.resume(task.id).unwrap();
            tokio::time::timeout(Duration::from_secs(20), async {
                loop {
                    let current = eng.task(task.id).unwrap();
                    if current.state == TaskState::Completed { break; }
                    assert_ne!(current.state, TaskState::Failed, "{}", current.error);
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
            }).await.unwrap();
        } else {
            eng.remove(task.id, true).await.unwrap();
            assert!(!work.exists());
            tokio::time::sleep(Duration::from_millis(500)).await;
            assert!(!work.exists(), "worker was still alive after remove returned");
        }
    }
    stop.store(true, Ordering::Relaxed);
    server.join().unwrap();
    let _ = std::fs::remove_dir_all(dir);
}
