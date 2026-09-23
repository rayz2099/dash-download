/* Pure media classification shared by the service worker and tests. */
(function (root) {
  // Bilibili uses separate MSE audio/video tracks, not a downloadable MPD.
  // Give yt-dlp the page so it can refresh signed URLs and pair both tracks.
  function page(raw) {
    let u;
    try { u = new URL(raw); } catch { return null; }
    if (!['http:', 'https:'].includes(u.protocol) || u.hostname !== 'www.bilibili.com' || u.port || u.username || u.password) return null;
    const match = u.pathname.match(/^\/video\/(BV[\da-zA-Z]{10}|av\d+)\/?$/);
    if (!match) return null;
    const part = u.searchParams.get('p');
    return `https://www.bilibili.com/video/${match[1]}/` + (part && /^[1-9]\d*$/.test(part) && part !== '1' ? `?p=${part}` : '');
  }
  function classify(raw, mime = '') {
    if (page(raw)) return 'site';
    let u;
    try { u = new URL(raw); } catch { return null; }
    if (!['http:', 'https:'].includes(u.protocol)) return null;
    const path = u.pathname.toLowerCase();
    if (/\.m3u8$/.test(path) || /mpegurl/i.test(mime)) return 'hls';
    if (/\.mpd$/.test(path) || /dash\+xml/i.test(mime)) return 'dash';
    if (/(?:^|[/_.-])(?:seg(?:ment)?|chunk|frag(?:ment)?|init)[-_\d./]/i.test(path) || /\.(?:m4s|ts|aac|vtt)$/.test(path)) return null;
    if (/\.(?:mp4|webm|mov)$/.test(path) || /^video\/(?:mp4|webm|quicktime)(?:;|$)/i.test(mime)) return 'file';
    return null;
  }
  function key(raw) {
    if (page(raw)) return page(raw);
    const u = new URL(raw);
    // X serves each resolution and playlist under the same media id.
    const x = u.hostname === 'video.twimg.com' && u.pathname.match(/\/(?:ext_tw_video|amplify_video|tweet_video)\/([^/]+)/);
    if (x) return `x:${x[1]}`;
    u.hash = '';
    // Signed URLs remain intact; never remove arbitrary query parameters.
    return u.href;
  }
  function label(raw) {
    const u = new URL(raw);
    const dimensions = u.pathname.match(/(?:\/|_)(\d{2,5})x(\d{2,5})(?:\/|[_.])/);
    return dimensions ? `${dimensions[2]}p` : u.pathname.split('/').pop() || u.hostname;
  }
  function filename(title, ext) {
    const clean = String(title || 'video').replace(/[<>:"/\\|?*\x00-\x1f]/g, '_').replace(/[. ]+$/g, '').slice(0, 120) || 'video';
    return clean.toLowerCase().endsWith(`.${ext}`) ? clean : `${clean}.${ext}`;
  }
  function manifest(text, base, kind) {
    if (kind === 'dash') return {
      unsupported: /<(?:[\w.-]+:)?ContentProtection\b/i.test(text) ? 'DRM' : /\btype\s*=\s*["']dynamic["']/i.test(text) ? 'LIVE' : '',
      children: [], segments: [], master: true,
    };
    const lines = text.split(/\r?\n/).map(l => l.trim());
    const master = lines.some(l => l.startsWith('#EXT-X-STREAM-INF:'));
    const resolve = value => { try { const u = new URL(value, base); return /^https?:$/.test(u.protocol) ? u.href : null; } catch { return null; } };
    const children = [], segments = [];
    for (let i = 0; i < lines.length; i++) {
      const line = lines[i];
      if (line.startsWith('#EXT-X-MEDIA:')) {
        const uri = line.match(/URI="([^"]+)"/); if (uri) children.push(resolve(uri[1]));
      }
      if (line && !line.startsWith('#')) {
        (master ? children : segments).push(resolve(line));
      }
    }
    const drm = lines.some(l => /^#EXT-X-(?:SESSION-)?KEY:/.test(l) && (/METHOD=SAMPLE-AES/.test(l) || (/KEYFORMAT=/.test(l) && !/KEYFORMAT="identity"/.test(l))));
    return { master, children: children.filter(Boolean), segments: segments.filter(Boolean), unsupported: drm ? 'DRM' : !master && !lines.includes('#EXT-X-ENDLIST') ? 'LIVE' : '' };
  }
  root.DDMedia = { classify, key, label, filename, manifest, page };
  if (typeof module !== 'undefined') module.exports = root.DDMedia;
})(globalThis);
