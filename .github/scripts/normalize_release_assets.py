#!/usr/bin/env python3
"""把 Release 资产改成 DashDownload-{ver}-{os}-{arch}.ext.

Tauri / tauri-action 默认文件名带空格和点 (Dash.Download_1.1.0_x64-setup.exe).
手工分发靠干净文件名; 1.2.1 起 updater 走 GitHub API 按后缀匹配, 不读 latest.json.

必须用 release_id: GET /releases/tags/{tag} 对 draft 返回 404,
而发版流水线在转正之前就要改名.
下载走 asset id + Accept: application/octet-stream.
`gh api` 默认 Accept 是 JSON, 会把 ~1.6KB 元数据当成二进制.
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
    """未知或不该改的文件返回 None, 避免误伤 source tarball."""
    if filename.startswith("Source code"):
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


def gh_api_json(path: str) -> dict:
    raw = run(
        ["gh", "api", path],
        capture_output=True,
        text=True,
    ).stdout
    return json.loads(raw)


def download_asset(repo: str, asset_id: int, dest: Path, size: int) -> None:
    # draft 不能靠 tag 下载; octet-stream 才能拿到真实字节而不是 asset JSON.
    with dest.open("wb") as out:
        run(
            [
                "gh",
                "api",
                "-H",
                "Accept: application/octet-stream",
                f"repos/{repo}/releases/assets/{asset_id}",
            ],
            stdout=out,
        )
    got = dest.stat().st_size
    if got != size:
        raise SystemExit(f"asset {asset_id}: downloaded {got} bytes, github size {size}")


def upload_asset(tag: str, path: Path) -> None:
    # uploads.github.com + gh api 不会带 GH_TOKEN, 1.2.2 在这里挂了.
    # gh release upload 跟 extension job 同一条路, draft 也能传.
    run(["gh", "release", "upload", tag, str(path), "--clobber"])


def delete_asset(tag: str, name: str) -> None:
    run(["gh", "release", "delete-asset", tag, name, "--yes"])


def main() -> int:
    if len(sys.argv) not in (2, 3):
        print(
            "usage: normalize_release_assets.py <tag> [release_id]",
            file=sys.stderr,
        )
        return 2
    tag = sys.argv[1]
    version = tag_version(tag)
    repo = repo_slug()
    if len(sys.argv) == 3 and sys.argv[2]:
        release_id = sys.argv[2]
        rel = gh_api_json(f"repos/{repo}/releases/{release_id}")
    else:
        # 已转正的 Release 才能按 tag 查; draft 必须传 release_id.
        rel = gh_api_json(f"repos/{repo}/releases/tags/{tag}")
        release_id = str(rel["id"])
    assets = rel["assets"]
    by_name = {a["name"]: a for a in assets}
    with tempfile.TemporaryDirectory() as tmp:
        tmp_path = Path(tmp)
        for asset in assets:
            old = asset["name"]
            new = canonical_name(old, version)
            if new is None or new == old:
                continue
            if new in by_name:
                print(f"skip {old}: {new} already exists")
                continue
            dest = tmp_path / new
            print(f"rename {old} -> {new} ({asset['size']} bytes)")
            download_asset(repo, int(asset["id"]), dest, asset["size"])
            upload_asset(tag, dest)
            delete_asset(tag, old)
            by_name[new] = asset
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
