#!/usr/bin/env python3
"""把 Release 资产改成 DashDownload-{ver}-{os}-{arch}.ext.

Tauri / tauri-action 的默认文件名带空格和点 (Dash.Download_1.1.0_x64-setup.exe),
updater 和手工分发都靠 URL 文件名; 先全部改名再重写 latest.json, 避免清单指向已删资产.

下载必须走 `gh release download`. `gh api` 默认 Accept 是 JSON,
会把资产元数据 (~1.6KB) 当成二进制传上去.
"""

from __future__ import annotations

import json
import os
import re
import subprocess
import sys
import tempfile
from pathlib import Path

APP = "DashDownload"


def run(cmd: list[str], **kwargs) -> subprocess.CompletedProcess:
    return subprocess.run(cmd, check=True, **kwargs)


def repo_slug() -> str:
    env = os.environ.get("GITHUB_REPOSITORY")
    if env:
        return env
    raw = run(
        ["gh", "repo", "view", "--json", "nameWithOwner", "-q", ".nameWithOwner"],
        capture_output=True,
        text=True,
    )
    slug = raw.stdout.strip()
    if not slug:
        raise SystemExit("GITHUB_REPOSITORY unset and gh repo view empty")
    return slug


def tag_version(tag: str) -> str:
    return tag[1:] if tag.startswith("v") else tag


def canonical_name(filename: str, version: str) -> str | None:
    """未知或不该改的文件返回 None, 避免误伤 latest.json / source tarball."""
    if filename in {"latest.json"} or filename.startswith("Source code"):
        return None
    prefix = f"{APP}-{version}-"
    if filename.startswith(prefix):
        return filename

    sig = ""
    base = filename
    if base.endswith(".sig"):
        sig = ".sig"
        base = base[: -len(".sig")]

    if re.search(r"chrome.*\.zip$", base, re.I) or base.startswith(
        "dash-download-chrome-"
    ):
        return f"{APP}-{version}-chrome.zip{sig}"
    if base.endswith(".dmg") and re.search(r"aarch64|arm64", base, re.I):
        return f"{APP}-{version}-mac-arm64.dmg{sig}"
    if base.endswith(".app.tar.gz"):
        return f"{APP}-{version}-mac-arm64.app.tar.gz{sig}"
    if base.endswith(".AppImage"):
        return f"{APP}-{version}-linux-x64.AppImage{sig}"
    if base.endswith(".deb"):
        return f"{APP}-{version}-linux-x64.deb{sig}"
    # Tauri 2 签的是 NSIS setup.exe, 不再出 nsis.zip
    if re.search(r"x64-setup\.exe$", base):
        return f"{APP}-{version}-win-x64.exe{sig}"
    return None


def rewrite_manifest(path: Path, version: str) -> None:
    data = json.loads(path.read_text())
    if "platforms" not in data:
        raise SystemExit(f"{path.name} is not an updater manifest: {list(data)[:8]}")
    platforms = data["platforms"]
    for item in platforms.values():
        url = item["url"]
        old_name = url.rsplit("/", 1)[-1]
        new_name = canonical_name(old_name, version)
        if new_name and new_name != old_name:
            item["url"] = url[: -len(old_name)] + new_name
    path.write_text(json.dumps(data, indent=2) + "\n")


def download_asset(
    tag: str,
    name: str,
    dest: Path,
    size: int,
) -> None:
    run(
        [
            "gh",
            "release",
            "download",
            tag,
            "--pattern",
            name,
            "--output",
            str(dest),
            "--clobber",
        ]
    )
    got = dest.stat().st_size
    if got != size:
        raise SystemExit(f"{name}: downloaded {got} bytes, github size {size}")


def main() -> int:
    if len(sys.argv) != 2:
        print("usage: normalize_release_assets.py <tag>", file=sys.stderr)
        return 2
    tag = sys.argv[1]
    version = tag_version(tag)
    repo = repo_slug()
    raw = run(
        ["gh", "api", f"repos/{repo}/releases/tags/{tag}"],
        capture_output=True,
        text=True,
    ).stdout
    rel = json.loads(raw)
    assets = rel["assets"]
    renames: list[tuple[str, str]] = []
    with tempfile.TemporaryDirectory() as tmp:
        tmp_path = Path(tmp)
        for asset in assets:
            old = asset["name"]
            new = canonical_name(old, version)
            if new is None or new == old:
                continue
            dest = tmp_path / new
            print(f"rename {old} -> {new} ({asset['size']} bytes)")
            download_asset(tag, old, dest, asset["size"])
            run(["gh", "release", "upload", tag, str(dest), "--clobber"])
            renames.append((old, new))

        # 清单必须在删旧文件名之前改 URL, 否则已装 app 拉 latest.json 会 404
        latest = next((a for a in assets if a["name"] == "latest.json"), None)
        if latest:
            dest = tmp_path / "latest.json"
            download_asset(tag, "latest.json", dest, latest["size"])
            rewrite_manifest(dest, version)
            run(["gh", "release", "upload", tag, str(dest), "--clobber"])

        for old, _new in renames:
            run(["gh", "release", "delete-asset", tag, old, "--yes"])
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
