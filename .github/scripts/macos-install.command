#!/bin/bash
# 未公证的 .app 带 quarantine 时, Finder 双击只给 Done / Move to Bin.
# 脚本拷到 Applications 再剥隔离属性, 不走 LaunchServices 那次 Gatekeeper 评估.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
src="$here/Dash Download.app"
if [[ ! -d "$src" ]]; then
  echo "未找到 Dash Download.app (当前目录: $here)" >&2
  exit 1
fi
if [[ -w /Applications ]]; then
  dest="/Applications/Dash Download.app"
else
  mkdir -p "$HOME/Applications"
  dest="$HOME/Applications/Dash Download.app"
fi
# 旧进程占着 bundle 时 ditto 会写花
pkill -x dd-app 2>/dev/null || true
sleep 0.2
rm -rf "$dest"
ditto "$src" "$dest"
xattr -cr "$dest"
open "$dest"
echo "已安装并打开: $dest"
