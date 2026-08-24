#!/usr/bin/env bash
# 跑 release .app --hidden, 等 localhost API. updater 配错会在 Builder.build 处直接 panic.
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
for _ in $(seq 1 40); do
  if ! kill -0 "$pid" 2>/dev/null; then
    wait "$pid" || true
    echo "app exited before API came up" >&2
    cat "$log" >&2
    exit 1
  fi
  if grep -q 'PluginInitialization' "$log"; then
    echo "updater config rejected in release" >&2
    cat "$log" >&2
    exit 1
  fi
  if curl -sf "http://127.0.0.1:41320/api/ping" >/dev/null; then
    echo "smoke ok: API up"
    exit 0
  fi
  sleep 0.4
done
echo "API did not come up" >&2
cat "$log" >&2
exit 1
