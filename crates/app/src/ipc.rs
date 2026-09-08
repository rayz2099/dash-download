//! GUI 侧私有 IPC: Unix socket / Windows named pipe, 权限收在用户配置目录.
//! native host 是 Chrome 拉起的另一进程, 只能当客户端把帧转进来.

use crate::api::{dispatch, ApiCtx};
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::path::Path;
use std::sync::Arc;

const MAX_FRAME: usize = 64 * 1024 * 1024;

pub fn sock_path(cfg_dir: &Path) -> std::path::PathBuf {
    cfg_dir.join("ipc.sock")
}

#[cfg(windows)]
pub fn pipe_name() -> &'static str {
    r"\\.\pipe\dash-download-ipc"
}

fn read_frame<R: Read>(r: &mut R) -> Result<Value, String> {
    let mut len_buf = [0u8; 4];
    r.read_exact(&mut len_buf).map_err(|e| e.to_string())?;
    let len = u32::from_le_bytes(len_buf) as usize;
    if len == 0 || len > MAX_FRAME {
        return Err("ipc 帧长度非法".into());
    }
    let mut buf = vec![0u8; len];
    r.read_exact(&mut buf).map_err(|e| e.to_string())?;
    serde_json::from_slice(&buf).map_err(|e| e.to_string())
}

fn write_frame<W: Write>(w: &mut W, v: &Value) -> Result<(), String> {
    let bytes = serde_json::to_vec(v).map_err(|e| e.to_string())?;
    w.write_all(&(bytes.len() as u32).to_le_bytes())
        .map_err(|e| e.to_string())?;
    w.write_all(&bytes).map_err(|e| e.to_string())?;
    w.flush().map_err(|e| e.to_string())?;
    Ok(())
}

/// native host 同步客户端. 连不上说明 GUI 没起来.
pub fn call(cfg_dir: &Path, req: &Value) -> Result<Value, String> {
    #[cfg(unix)]
    {
        let mut s = std::os::unix::net::UnixStream::connect(sock_path(cfg_dir))
            .map_err(|e| e.to_string())?;
        write_frame(&mut s, req)?;
        read_frame(&mut s)
    }
    #[cfg(windows)]
    {
        let mut s = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(pipe_name())
            .map_err(|e| e.to_string())?;
        write_frame(&mut s, req)?;
        read_frame(&mut s)
    }
}

pub fn up(cfg_dir: &Path) -> bool {
    call(cfg_dir, &json!({ "op": "ping" })).is_ok()
}

async fn handle_req(ctx: Arc<ApiCtx>, req: Value) -> Value {
    match dispatch(&ctx, req).await {
        Ok(v) => v,
        Err(e) => json!({ "ok": false, "error": e }),
    }
}

/// GUI tokio 线程里接 native host.
pub async fn serve(ctx: Arc<ApiCtx>) {
    #[cfg(unix)]
    if let Err(e) = serve_unix(ctx).await {
        eprintln!("ipc server 退出: {e}");
    }
    #[cfg(windows)]
    if let Err(e) = serve_win(ctx).await {
        eprintln!("ipc server 退出: {e}");
    }
}

#[cfg(unix)]
async fn serve_unix(ctx: Arc<ApiCtx>) -> Result<(), String> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::UnixListener;
    let path = sock_path(&ctx.cfg_dir);
    if path.exists() {
        if std::os::unix::net::UnixStream::connect(&path).is_ok() {
            return Err("ipc 已被占用".into());
        }
        let _ = std::fs::remove_file(&path);
    }
    let listener = UnixListener::bind(&path).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut p = std::fs::metadata(&path).map_err(|e| e.to_string())?.permissions();
        p.set_mode(0o600);
        std::fs::set_permissions(&path, p).map_err(|e| e.to_string())?;
    }
    loop {
        let (mut stream, _) = listener.accept().await.map_err(|e| e.to_string())?;
        let ctx = ctx.clone();
        tokio::spawn(async move {
            let mut len_buf = [0u8; 4];
            if stream.read_exact(&mut len_buf).await.is_err() {
                return;
            }
            let len = u32::from_le_bytes(len_buf) as usize;
            if len == 0 || len > MAX_FRAME {
                return;
            }
            let mut buf = vec![0u8; len];
            if stream.read_exact(&mut buf).await.is_err() {
                return;
            }
            let Ok(req) = serde_json::from_slice::<Value>(&buf) else {
                return;
            };
            let reply = handle_req(ctx, req).await;
            let Ok(bytes) = serde_json::to_vec(&reply) else {
                return;
            };
            let _ = stream.write_all(&(bytes.len() as u32).to_le_bytes()).await;
            let _ = stream.write_all(&bytes).await;
            let _ = stream.flush().await;
        });
    }
}

#[cfg(windows)]
async fn serve_win(ctx: Arc<ApiCtx>) -> Result<(), String> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::windows::named_pipe::ServerOptions;
    loop {
        let mut server = ServerOptions::new()
            .first_pipe_instance(false)
            .create(pipe_name())
            .map_err(|e| e.to_string())?;
        server.connect().await.map_err(|e| e.to_string())?;
        let ctx = ctx.clone();
        tokio::spawn(async move {
            let mut len_buf = [0u8; 4];
            if server.read_exact(&mut len_buf).await.is_err() {
                return;
            }
            let len = u32::from_le_bytes(len_buf) as usize;
            if len == 0 || len > MAX_FRAME {
                return;
            }
            let mut buf = vec![0u8; len];
            if server.read_exact(&mut buf).await.is_err() {
                return;
            }
            let Ok(req) = serde_json::from_slice::<Value>(&buf) else {
                return;
            };
            let reply = handle_req(ctx, req).await;
            let Ok(bytes) = serde_json::to_vec(&reply) else {
                return;
            };
            let _ = server.write_all(&(bytes.len() as u32).to_le_bytes()).await;
            let _ = server.write_all(&bytes).await;
            let _ = server.flush().await;
        });
    }
}
