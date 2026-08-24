set shell := ["bash", "-euo", "pipefail", "-c"]
set dotenv-load := false

root := justfile_directory()
tauri := root / "node_modules/.bin/tauri"
app_dir := root / "crates/app"

# 与用户约定的三端 triple: macos/arm, linux/x86, windows/x86 均指 64 位
target_macos := "aarch64-apple-darwin"
target_linux := "x86_64-unknown-linux-gnu"
target_windows := "x86_64-pc-windows-msvc"

# 本地 just 只要能跑的安装包. updater 签名走 CI 的 TAURI_SIGNING_PRIVATE_KEY, 避免本机无 TTY 时 minisign 抢密码失败.
no_updater := '{"bundle":{"createUpdaterArtifacts":false}}'

default:
    @just --list

# 安装 pnpm 依赖并补齐 rustup 交叉编译 target
setup:
    pnpm install
    pnpm --dir ui install
    rustup target add {{target_macos}} {{target_linux}} {{target_windows}}

# 开发模式: vite + tauri 热更新. 先清 debug 残留, 避免 41320 被占窗口起不来.
dev:
    pkill -f 'target/.*/debug/dd-app' 2>/dev/null || true
    cd "{{app_dir}}" && "{{tauri}}" dev

# 打开最近一次 macOS 产物 (优先带 --target 的路径)
open:
    #!/usr/bin/env bash
    set -euo pipefail
    triple="{{root}}/target/{{target_macos}}/release/bundle/macos/Dash Download.app"
    host="{{root}}/target/release/bundle/macos/Dash Download.app"
    if [[ -d "$triple" ]]; then
      open "$triple"
    elif [[ -d "$host" ]]; then
      open "$host"
    else
      echo "未找到 .app, 先跑: just macos-arm" >&2
      exit 1
    fi

# 编译 macOS arm64 (.app)
macos-arm:
    rustup target add {{target_macos}}
    cd "{{app_dir}}" && "{{tauri}}" build --target {{target_macos}} --bundles app --config '{{no_updater}}'
    codesign --verify --deep --strict "{{root}}/target/{{target_macos}}/release/bundle/macos/Dash Download.app"
    @echo "→ {{root}}/target/{{target_macos}}/release/bundle/macos/Dash Download.app"

# 编译 Linux x86_64 (deb). 在非 Linux 主机上需要对应 linker / webkit sysroot, 本机 mac 上通常编不过
linux-x86:
    rustup target add {{target_linux}}
    cd "{{app_dir}}" && "{{tauri}}" build --target {{target_linux}} --bundles deb --config '{{no_updater}}'
    @echo "→ {{root}}/target/{{target_linux}}/release/bundle/deb/"

# 编译 Windows x86_64 (nsis). 在非 Windows 主机上需要 cargo-xwin 或 msvc sysroot
windows-x86:
    rustup target add {{target_windows}}
    cd "{{app_dir}}" && "{{tauri}}" build --target {{target_windows}} --bundles nsis --config '{{no_updater}}'
    @echo "→ {{root}}/target/{{target_windows}}/release/bundle/nsis/"

# 本机没有 linux/windows sysroot, cargo --target 会卡 C 依赖.
# 用源码闸拦住 RunEvent::Reopen 这类只在 darwin 存在的变体.
check-cross:
    #!/usr/bin/env bash
    set -euo pipefail
    if ! awk '
      /#\[cfg\(target_os = "macos"\)\]/ { macos=1; next }
      /RunEvent::Reopen/ {
        if (!macos) { print FILENAME ":" NR ": Reopen 缺少 macos cfg"; exit 1 }
      }
      { macos=0 }
    ' crates/app/src/main.rs; then
      exit 1
    fi
    echo "ok: RunEvent::Reopen 已 cfg macos"

# 更新检查/安装决策单测. GitHub Release 真包这条验收走这里, 不靠点 UI.
test:
    cargo test -p dd-app
    cargo test -p dd-core
    pnpm --dir ui test
    node --test extension/policy.test.js
