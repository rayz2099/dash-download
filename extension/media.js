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
  // 原图地址没有后缀，桌面端探测到真实媒体类型后再补。这里先不猜 jpg。
  function imageName(title, n) {
    const clean = String(title || 'image').replace(/[<>:"/\\|?*\x00-\x1f]/g, '_').replace(/[. ]+$/g, '').slice(0, 100) || 'image';
    return `${clean}-${String(n).padStart(2, '0')}`;
  }
  // 只认图文笔记页。xsec_token 是分享链打开笔记所必需的，其它查询参数是跟踪信息。
  function xhsPage(raw) {
    let u;
    try { u = new URL(raw); } catch { return null; }
    if (u.protocol !== 'https:' || u.hostname !== 'www.xiaohongshu.com' || u.port || u.username || u.password) return null;
    const match = u.pathname.match(/^\/explore\/([0-9a-f]{24})\/?$/i);
    if (!match) return null;
    const token = u.searchParams.get('xsec_token');
    const id = match[1];
    const page = `https://www.xiaohongshu.com/explore/${id}` + (token ? `?xsec_token=${encodeURIComponent(token)}` : '');
    return { id, page };
  }
  // 文件 id 可以带 notes_pre_post、spectrum 这类目录，但不能带水印样式。
  function xhsToken(id) {
    if (typeof id !== 'string') return null;
    const token = id.trim().replace(/^\/+/, '');
    if (/imageview|watermark|wlteh/i.test(token)) return null;
    if (!/^(?:[A-Za-z0-9_-]+\/)*[A-Za-z0-9_-]{8,}$/.test(token)) return null;
    return token;
  }
  function xhsTokenFromUrl(raw) {
    let u;
    try { u = new URL(raw); } catch { return null; }
    if (!['http:', 'https:'].includes(u.protocol) || u.username || u.password) return null;
    const host = u.hostname;
    if (host !== 'ci.xiaohongshu.com' && !host.endsWith('.xhscdn.com')) return null;
    let path = u.pathname.replace(/^\/+/, '');
    const bang = path.indexOf('!');
    if (bang >= 0) path = path.slice(0, bang);
    const signed = path.match(/^\d{10,14}\/[a-f0-9]{16,}\/(.+)$/i);
    if (signed) path = signed[1];
    try { path = decodeURIComponent(path); } catch { return null; }
    return xhsToken(path);
  }
  function xhsUrlToken(image) {
    const raws = [image.url, image.urlDefault, image.url_default, image.urlPre, image.url_pre];
    for (const info of image.infoList || image.info_list || []) if (info && info.url) raws.push(info.url);
    let best = null;
    for (const raw of raws) {
      const token = xhsTokenFromUrl(raw);
      if (token && (!best || token.length > best.length)) best = token;
    }
    return best;
  }
  // traceId 常常只是文件名，带目录的那一段在带水印的 webpic 地址里。下载仍只用拼出来的原图。
  function xhsImageToken(image) {
    const id = xhsToken(image.traceId) || xhsToken(image.trace_id) || xhsToken(image.fileId) || xhsToken(image.file_id);
    const fromUrl = xhsUrlToken(image);
    if (id && fromUrl && (fromUrl === id || fromUrl.endsWith('/' + id))) return fromUrl;
    return id || fromUrl;
  }
  function xhsOriginOk(raw) {
    let u;
    try { u = new URL(raw); } catch { return false; }
    if (u.protocol !== 'https:' || u.hostname !== 'ci.xiaohongshu.com' || u.port || u.search || u.username || u.password || u.pathname.includes('!')) return false;
    let path = u.pathname.replace(/^\/+/, '');
    try { path = decodeURIComponent(path); } catch { return false; }
    return xhsToken(path) === path;
  }
  function xhsOrigin(token) {
    const url = `https://ci.xiaohongshu.com/${token}`;
    return xhsOriginOk(url) ? url : null;
  }
  function xhsVideoUrl(raw) {
    let u;
    try { u = new URL(raw); } catch { return null; }
    if (!['http:', 'https:'].includes(u.protocol) || u.username || u.password) return null;
    const host = u.hostname;
    if (!host.endsWith('.xhscdn.com') && !host.endsWith('.xiaohongshu.com')) return null;
    const path = u.pathname.toLowerCase();
    if (path.includes('!') || path.includes('watermark')) return null;
    if (!/\.(?:mp4|webm|mov)$/.test(path) && !path.includes('/stream/')) return null;
    return u.href;
  }
  // 实况短视频有多套编码。只留码率最高的一条 master，不拿备用域名充数。
  function xhsLive(image) {
    const stream = image && image.stream;
    if (!stream || typeof stream !== 'object') return null;
    let best = null;
    for (const name of ['h264', 'h265', 'av1']) {
      const group = stream[name] || stream[name.toUpperCase()];
      if (!Array.isArray(group)) continue;
      for (const item of group) {
        if (!item) continue;
        const url = xhsVideoUrl(item.masterUrl || item.master_url);
        if (!url) continue;
        const bitrate = Number(item.avgBitrate ?? item.avg_bitrate ?? item.videoBitrate ?? item.video_bitrate);
        const width = Number(item.width), height = Number(item.height);
        const rank = {
          url,
          bitrate: Number.isFinite(bitrate) && bitrate > 0 ? bitrate : 0,
          pixels: (Number.isFinite(width) && width > 0 ? width : 0) * (Number.isFinite(height) && height > 0 ? height : 0),
        };
        if (!best || rank.bitrate > best.bitrate || (rank.bitrate === best.bitrate && rank.pixels > best.pixels)) best = rank;
      }
    }
    return best ? best.url : null;
  }
  function xhsFind(payload, id) {
    if (!payload || typeof payload !== 'object') return null;
    const noteRoot = payload.note;
    const map = noteRoot && (noteRoot.noteDetailMap || noteRoot.note_detail_map);
    if (map && map[id]) {
      const slot = map[id];
      if (slot && typeof slot === 'object') return slot.note || slot.note_card || slot.noteCard || null;
    }
    const items = (payload.data && payload.data.items) || payload.items;
    if (!Array.isArray(items)) return null;
    for (const item of items) {
      if (!item) continue;
      const card = item.note_card || item.noteCard || item.note;
      if (!card) continue;
      if (String(card.note_id || card.noteId || item.id || '') === id) return card;
    }
    return null;
  }
  function xhsTitle(note) {
    const title = String(note.title || note.displayTitle || note.display_title || '').trim();
    if (title) return title.slice(0, 200);
    return String(note.desc || '').trim().split('\n')[0].slice(0, 200);
  }
  // 整页视频笔记不进图片列表。实况的静图留在图片里，短视频另走视频任务。
  function xhsNote(payload, pageUrl) {
    const loc = xhsPage(pageUrl);
    if (!loc) return null;
    const note = xhsFind(payload, loc.id);
    if (!note || note.type === 'video' || note.note_type === 'video') return null;
    const list = note.imageList || note.image_list;
    if (!Array.isArray(list) || !list.length) return null;
    const title = xhsTitle(note);
    const images = [];
    const videos = [];
    for (const image of list.slice(0, 30)) {
      const n = images.length + 1;
      const token = image && typeof image === 'object' ? xhsImageToken(image) : null;
      const url = token && xhsOrigin(token);
      images.push(url ? { n, url } : { n, error: 'nowm' });
      const live = image && typeof image === 'object' ? xhsLive(image) : null;
      if (live) videos.push({ url: live, title: `${title || '图片'} 实况 ${n}` });
    }
    return { id: loc.id, page: loc.page, title, images, videos };
  }
  // 收起时忽略勾选，展开后只提交仍有原图且被勾上的序号。
  function xhsPick(images, open, picked) {
    const ready = [];
    for (const image of images || []) if (image && image.url) ready.push(image.n);
    if (!open) return ready;
    const on = new Set(picked || []);
    return ready.filter(n => on.has(n));
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
  root.DDMedia = { classify, key, label, filename, imageName, manifest, page, xhsPage, xhsNote, xhsPick, xhsOriginOk };
  if (typeof module !== 'undefined') module.exports = root.DDMedia;
})(globalThis);
