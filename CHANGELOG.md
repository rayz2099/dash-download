## 1.2.1 - 2026-08-24

### Fixes
- Linux/Windows pack no longer fails on macOS-only `RunEvent::Reopen`
- Auto-update no longer depends on `latest.json` for new builds; it reads GitHub Releases API
- Release workflow stays draft until signed packages exist, so `/releases/latest` does not 404 mid-build

## 1.2.0 - 2026-08-24

### Features
- Magnet / .torrent downloads via librqbit, with file selection after resolve
- P2P settings, off by default; public trackers prefer XIU2 with ngosang as a supplement
- Magnet resolve uses the itorrents.net HTTP cache so a file list works without P2P
- Multi-file torrents wrap in a named folder; a single file goes straight to the save dir
- Chrome takeover for magnet clicks and `.torrent` downloads
- Number steppers accept typed values; directory fields have Browse

### Fixes
- File-pick PATCH blocked by CORS (`TypeError: Load failed` in WKWebView)
- Torrents stuck in queued when the BT session was not up yet, and after a download finishes
- Pasting the same magnet again reopens the file list instead of silently closing
- itorrents cache redirects to a different infohash are treated as a miss and fall through to DHT
- Crash recovery pauses torrents instead of auto-resuming
- `private=1` torrents no longer announce to public trackers
- Magnet clicks only suppress navigation after a successful handoff
- Opening a missing file shows "文件不存在" instead of jumping to the folder
- DHT bootstrap drops dead bitcomet DNS and adds IP nodes

## 1.1.0 - 2026-08-22

### Features
- Add in-app settings, launch-at-login, and GitHub updater
- Take over blob/data downloads through the page hook and extension policy

### Fixes
- Harden native-host wake, takeover, and probe diagnostics
- Reject blob/data imports larger than 24MB and stop writing the page takeover flag
