# Dash Download

English | [中文](#dash-download-中文)

Cross-platform HTTP/HTTPS and BitTorrent download manager with Chrome takeover.

![Dash Download](docs/app.png)

## Quick start

### 1. Download

Get the desktop app and Chrome extension with the same version from [GitHub Releases](https://github.com/rayz2099/dash-download/releases/latest).

| Platform | Asset |
|---|---|
| macOS arm64 | `DashDownload-*-mac-arm64.dmg` |
| Linux x86_64 | `DashDownload-*-linux-x64.deb` |
| Windows x86_64 | `DashDownload-*-win-x64.exe` |
| Chrome extension | `DashDownload-*-chrome.zip` |

### 2. Install the app

- macOS: drag `Dash Download.app` into Applications. If macOS blocks it, use System Settings → Privacy & Security → Open Anyway.
- Linux: open the `.deb` with the system package installer.
- Windows: run the setup `.exe`.

Launch the app once to register the Chrome connection.

### 3. Install the extension

1. Unzip `DashDownload-*-chrome.zip`.
2. Open `chrome://extensions`.
3. Enable **Developer mode**.
4. Click **Load unpacked** and select the folder containing `manifest.json`.
5. Pin Dash Download to the toolbar.

The orange badge on the extension card means “Unpacked extension” and is normal.

### 4. Use

- The toolbar popup should show a green connection indicator.
- Click a file link to let Dash Download take over.
- Right-click a link → **使用 Dash Download 下载** to send it directly.
- Use **新建下载** in the app for URLs, magnets, or `.torrent` files.
- Closing the window keeps downloads running in the tray.

Keep the app and extension versions aligned. After replacing the extension files, click **Reload** on `chrome://extensions`.

### Troubleshooting

| Problem | Fix |
|---|---|
| Popup shows `app 未运行` | Launch the app once, then reopen the popup |
| Browser still downloads the file | Check takeover, size threshold, and deny list; use the context menu to force it |
| App download is empty or returns 403 | Start it from the logged-in browser page so cookies can be forwarded |
| Takeover stops after an update | Update both pieces, then reload the extension |

---

# Dash Download（中文）

[English](#dash-download) | 中文

支持 HTTP/HTTPS 与 BitTorrent 的跨平台下载管理器，可接管 Chrome 下载。

![Dash Download](docs/app.png)

## 快速使用

### 1. 下载

从 [GitHub Releases](https://github.com/rayz2099/dash-download/releases/latest) 下载相同版本的桌面应用和 Chrome 扩展。

| 平台 | 文件 |
|---|---|
| macOS arm64 | `DashDownload-*-mac-arm64.dmg` |
| Linux x86_64 | `DashDownload-*-linux-x64.deb` |
| Windows x86_64 | `DashDownload-*-win-x64.exe` |
| Chrome 扩展 | `DashDownload-*-chrome.zip` |

### 2. 安装应用

- macOS：把 `Dash Download.app` 拖进 Applications。若被系统拦截，到“系统设置 → 隐私与安全性 → 仍要打开”。
- Linux：使用系统软件安装器打开 `.deb`。
- Windows：运行安装程序 `.exe`。

首次安装后启动一次应用，以注册 Chrome 通道。

### 3. 安装扩展

1. 解压 `DashDownload-*-chrome.zip`。
2. 打开 `chrome://extensions`。
3. 开启**开发者模式**。
4. 点击**加载已解压的扩展程序**，选择包含 `manifest.json` 的目录。
5. 把 Dash Download 固定到工具栏。

扩展卡片上的橙色角标表示“未打包扩展”，属于正常提示。

### 4. 使用

- 工具栏弹窗显示绿色连接状态即正常。
- 点击文件链接，由 Dash Download 自动接管。
- 链接右键 → **使用 Dash Download 下载**，可直接发送。
- 应用内点击**新建下载**，可添加 URL、磁力或 `.torrent`。
- 关闭窗口后任务继续在托盘运行。

应用与扩展版本需保持一致。替换扩展文件后，到 `chrome://extensions` 点击**重新加载**。

### 常见问题

| 问题 | 处理 |
|---|---|
| 弹窗显示 `app 未运行` | 启动一次应用，再重新打开弹窗 |
| 浏览器仍自行下载 | 检查接管开关、体积阈值和黑名单；可用右键菜单强制发送 |
| 下载为空或返回 403 | 从已登录网页发起，以便携带 Cookie |
| 更新后不再接管 | 同时更新应用和扩展，再重新加载扩展 |

[Apache License 2.0](LICENSE)
