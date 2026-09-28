# dash-download

## New version

用户说 **new version**（可带版本号，如 `new version 1.3.3`）时，这就是打 tag 并推送的授权，直接触发打包，不再确认。

版本号取自用户给出的号。没给号时，用工作区里已经写好的版本，三处必须相同：

- `Cargo.toml` 的 `version`
- `crates/app/tauri.conf.json` 的 `version`
- `extension/manifest.json` 的 `version`

条件不满足就停下并说明原因，不要打 tag：

- 三处版本不一致，或工作区有未提交改动
- 该版本的提交还没在当前 `HEAD`
- 远端已有 `vX.Y.Z`

满足时：

1. 在当前 `HEAD` 打 annotated tag：`vX.Y.Z`，说明为 `Dash Download vX.Y.Z`
2. 只推这个 tag：`git push origin vX.Y.Z`。推 tag 会触发 `.github/workflows/release.yml`（`v*`），全平台安装包加 Chrome 扩展
3. 回复里给出 Actions run 链接

不在这句话里改版本号、写 changelog，也不推 `main`。
