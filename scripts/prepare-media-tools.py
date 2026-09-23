"""Prepare pinned, standalone media tools for development and release bundles.
Requires: python -m pip install imageio-ffmpeg==0.6.0
"""
import hashlib
import io
import zipfile
import os
import pathlib
import platform
import shutil
import urllib.request
import imageio_ffmpeg

ROOT = pathlib.Path(__file__).resolve().parents[1]
DEST = ROOT / 'crates/app/media-tools'
DEST.mkdir(parents=True, exist_ok=True)
VERSION = '2026.08.19'
system = platform.system()
asset = {'Darwin': 'yt-dlp_macos.zip', 'Windows': 'yt-dlp.exe', 'Linux': 'yt-dlp_linux'}[system]
if system == 'Linux' and platform.machine() in ('aarch64', 'arm64'):
    asset += '_aarch64'
base = f'https://github.com/yt-dlp/yt-dlp/releases/download/{VERSION}/'
checks = urllib.request.urlopen(base + 'SHA2-256SUMS').read().decode()
expected = next(line.split()[0] for line in checks.splitlines() if line.split()[-1].lstrip('*') == asset)
data = urllib.request.urlopen(base + asset).read()
if hashlib.sha256(data).hexdigest() != expected:
    raise RuntimeError('yt-dlp checksum mismatch')
ext = '.exe' if system == 'Windows' else ''
path = DEST / ('yt-dlp' + ext)
if system == 'Darwin':
    archive = zipfile.ZipFile(io.BytesIO(data))
    for member in archive.infolist():
        target = (DEST / member.filename).resolve()
        if not target.is_relative_to(DEST.resolve()):
            raise RuntimeError('Invalid archive path')
    archive.extractall(DEST)
    (DEST / 'yt-dlp_macos').replace(path)
else:
    path.write_bytes(data)
path.chmod(0o755)
shutil.copy2(imageio_ffmpeg.get_ffmpeg_exe(), DEST / ('ffmpeg' + ext))
(DEST / ('ffmpeg' + ext)).chmod(0o755)
# Preserve the distributor's license and build information alongside the binary.
pkg = pathlib.Path(imageio_ffmpeg.__file__).parent
for file in pkg.rglob('*'):
    if file.is_file() and ('license' in file.name.lower() or 'readme' in file.name.lower()):
        shutil.copy2(file, DEST / ('ffmpeg-' + file.name))
(DEST / 'yt-dlp-LICENSE.txt').write_bytes(urllib.request.urlopen(f'https://raw.githubusercontent.com/yt-dlp/yt-dlp/{VERSION}/LICENSE').read())
print(DEST)
