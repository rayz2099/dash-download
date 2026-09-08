#!/usr/bin/env bash
# 跑 release .app --hidden, 等 ipc.sock 上的 ping.
# updater 配错会在 Builder.build 处直接 panic; IPC 比 tauri build 更早起来,
# 所以 ping 通之后还要再盯一会儿, 避免秒退被漏掉.
set -euo pipefail
if [[ $# -ne 1 ]]; then
  echo "usage: smoke_macos_app.sh <Dash Download.app>" >&2
  exit 2
fi
app=$1
bin="$app/Contents/MacOS/dd-app"
if [[ ! -x "$bin" ]]; then
  echo "no binary: $bin" >&2
  exit 1
fi
log=$(mktemp)
pid=""
cleanup() {
  if [[ -n "$pid" ]] && kill -0 "$pid" 2>/dev/null; then
    kill "$pid" 2>/dev/null || true
    wait "$pid" 2>/dev/null || true
  fi
  rm -f "$log"
}
trap cleanup EXIT
# 单实例插件会把第二份 release 进程直接掐掉, 冒烟必须独占
pkill -x dd-app 2>/dev/null || true
sleep 0.3
"$bin" --hidden >"$log" 2>&1 &
pid=$!

# 1.2.7 起不再 bind 127.0.0.1:41320, 控制面在用户配置目录的 ipc.sock.
ipc_ping() {
  python3 - <<'PY'
import json, socket, struct, sys
from pathlib import Path

sock = Path.home() / "Library/Application Support/dash-download/ipc.sock"
if not sock.exists():
    sys.exit(1)
req = json.dumps({"op": "ping"}).encode()
s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
s.settimeout(1)
try:
    s.connect(str(sock))
    s.sendall(struct.pack("<I", len(req)) + req)
    hdr = b""
    while len(hdr) < 4:
        chunk = s.recv(4 - len(hdr))
        if not chunk:
            sys.exit(1)
        hdr += chunk
    n = struct.unpack("<I", hdr)[0]
    if n == 0 or n > 64 * 1024 * 1024:
        sys.exit(1)
    buf = b""
    while len(buf) < n:
        chunk = s.recv(n - len(buf))
        if not chunk:
            sys.exit(1)
        buf += chunk
    data = json.loads(buf)
    if data.get("ok") is True and data.get("name") == "dash-download":
        print(data.get("version", ""), flush=True)
        sys.exit(0)
except Exception:
    sys.exit(1)
sys.exit(1)
PY
}

fail() {
  echo "$1" >&2
  echo "sock: $HOME/Library/Application Support/dash-download/ipc.sock" >&2
  cat "$log" >&2
  exit 1
}

for _ in $(seq 1 40); do
  if ! kill -0 "$pid" 2>/dev/null; then
    wait "$pid" || true
    fail "app exited before IPC came up"
  fi
  if grep -q 'PluginInitialization' "$log"; then
    fail "updater config rejected in release"
  fi
  if ver=$(ipc_ping); then
    # IPC 在 tauri::Builder.build 之前就 listen, 再等 2s 抓 updater 秒退
    sleep 2
    if ! kill -0 "$pid" 2>/dev/null; then
      wait "$pid" || true
      fail "app exited after IPC ping (likely tauri build panic)"
    fi
    if grep -q 'PluginInitialization' "$log"; then
      fail "updater config rejected in release"
    fi
    if ! ipc_ping >/dev/null; then
      fail "IPC ping dropped after tauri start"
    fi
    echo "smoke ok: IPC up version=${ver:-unknown}"
    exit 0
  fi
  sleep 0.4
done
fail "IPC did not come up"
