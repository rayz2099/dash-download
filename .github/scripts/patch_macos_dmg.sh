#!/usr/bin/env bash
# 往已打好的 UDZO DMG 里塞打开脚本. tauri-action 上传前必须跑, 否则用户拿到的还是空 DMG.
set -euo pipefail
if [[ $# -ne 2 ]]; then
  echo "usage: patch_macos_dmg.sh <dmg> <install.command>" >&2
  exit 2
fi
dmg=$1
cmd=$2
if [[ ! -f "$dmg" ]]; then
  echo "no dmg: $dmg" >&2
  exit 1
fi
if [[ ! -f "$cmd" ]]; then
  echo "no command: $cmd" >&2
  exit 1
fi
tmp=$(mktemp -d)
mnt=""
cleanup() {
  if [[ -n "$mnt" ]]; then
    hdiutil detach "$mnt" -quiet >/dev/null 2>&1 || true
  fi
  rm -rf "$tmp"
}
trap cleanup EXIT
rw="$tmp/rw.dmg"
hdiutil convert "$dmg" -format UDRW -o "$rw" >/dev/null
mkdir -p "$tmp/mnt"
hdiutil attach -readwrite -nobrowse -mountroot "$tmp/mnt" "$rw" >/dev/null
mnt=$(find "$tmp/mnt" -mindepth 1 -maxdepth 1 -type d | head -n 1)
if [[ -z "$mnt" || ! -d "$mnt/Dash Download.app" ]]; then
  echo "dmg missing Dash Download.app" >&2
  exit 1
fi
cp "$cmd" "$mnt/打开应用.command"
chmod 755 "$mnt/打开应用.command"
hdiutil detach "$mnt" -quiet
mnt=""
out="$tmp/out.dmg"
hdiutil convert "$rw" -format UDZO -imagekey zlib-level=9 -o "$out" >/dev/null
mv -f "$out" "$dmg"
echo "patched $dmg"
