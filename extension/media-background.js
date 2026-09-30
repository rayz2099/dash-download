// Tab-scoped records survive MV3 worker suspension, but never browser restarts.
const mediaTabs = new Map();
const mediaReady = chrome.storage.session.get('mediaTabs').then(({ mediaTabs: saved }) => {
  for (const [id, state] of saved || []) {
    for (const resource of state.resources) if (resource.status === 'sending') {
      resource.status = ''; resource.error = '连接中断，请先检查桌面任务列表再重试 / Check desktop tasks before retrying';
    }
    mediaTabs.set(id, state);
  }
});
const mediaJobs = new Map();
const manifestJobs = new Set();
let persistTimer;
function persistMedia() {
  clearTimeout(persistTimer);
  persistTimer = setTimeout(() => chrome.storage.session.set({ mediaTabs: [...mediaTabs].map(([id, state]) => [id, publicState(state)]) }).catch(() => {}), 100);
}
function publicState(state) {
  return { ...state, resources: state.resources.map(({ segments, ...r }) => r) };
}
function tabState(id) {
  if (!mediaTabs.has(id)) mediaTabs.set(id, { resources: [], hidden: false });
  return mediaTabs.get(id);
}
function publishMedia(tabId) {
  const state = tabState(tabId);
  persistMedia();
  chrome.tabs.sendMessage(tabId, { type: 'dd-media-update', state: publicState(state) }, { frameId: 0 }).catch(() => {});
}
async function recordMedia(tabId, url, mime, meta = {}) {
  await mediaReady;
  if (tabId < 0 || !cached.enabled || typeof url !== "string" || url.length > 8192) return;
  url = DDMedia.page(url) || url;
  const kind = DDMedia.classify(url, mime);
  if (!kind) return;
  const state = tabState(tabId);
  if (state.resources.some(r => r.segments?.includes(url))) return;
  const id = DDMedia.key(url);
  let resource = state.resources.find(r => r.id === id || r.sources.some(s => s.url === url) || r.children?.includes(url));
  if (!resource) {
    if (state.resources.length >= 50) return;
    resource = { id, title: meta.title || '', referrer: meta.referrer || '', sources: [], status: '', error: '' };
    state.resources.push(resource);
  }
  if (meta.title) resource.title = meta.title.slice(0, 200);
  if (meta.referrer) resource.referrer = meta.referrer;
  if (meta.group) resource.group = meta.group;
  if (!resource.sources.some(s => s.url === url) && !resource.children?.includes(url)) {
    resource.sources.push({ url, kind, label: DDMedia.label(url) });
    resource.sources.sort((a, b) => (a.kind === 'file') - (b.kind === 'file') || parseInt(b.label) - parseInt(a.label));
    resource.sources = resource.sources.slice(0, 12);
  }
  publishMedia(tabId);
  if (kind === 'hls' || kind === 'dash') readManifest(tabId, resource, url, kind).catch(() => {});
}
// 一条图文笔记一个条目。序号由悬浮窗决定，桌面端按无水印原图各建一条任务。
function recordNote(tabId, note) {
  const state = tabState(tabId);
  state.resources = state.resources.filter(r => !r.sources.some(s => s.kind === 'images') || r.id === note.page);
  let resource = state.resources.find(r => r.id === note.page);
  if (!resource) {
    if (state.resources.length >= 50) return;
    resource = { id: note.page, title: '', referrer: note.page, sources: [], images: [], added: [], status: '', error: '' };
    state.resources.push(resource);
  }
  resource.title = (note.title || '').slice(0, 200);
  resource.referrer = note.page;
  resource.images = note.images;
  resource.sources = [{ url: note.page, kind: 'images', label: String(note.images.length) }];
  publishMedia(tabId);
}
// 序号必须对得上笔记里的原图。带水印的地址即使被改进来也不能提交。
function selectedImages(resource, request) {
  const picked = Array.isArray(request.images) ? [...new Set(request.images)] : [];
  if (!picked.length) throw new Error('没有选中的图片');
  const todo = [];
  for (const n of picked) {
    if (!Number.isInteger(n)) throw new Error('没有选中的图片');
    const image = (resource.images || []).find(img => img.n === n);
    if (!image) throw new Error('图片已失效');
    if (!image.url || !DDMedia.xhsOriginOk(image.url)) throw new Error('无水印原图不可用');
    todo.push(image);
  }
  return todo;
}
// 点下载才把桌面窗口拉到前台。读清晰度不走这里，避免人还在网页上选画质时窗口跳出来。
async function focusApp() {
  try { await native({ op: 'focus' }); } catch { /* 窗口拉不起来时任务仍要入队 */ }
}
async function addImages(resource, images, title, referrer, tabId) {
  resource.added = resource.added || [];
  const pending = images.filter(img => !resource.added.includes(img.n));
  if (pending.length) await focusApp();
  for (const image of pending) {
    // Cookie 跟笔记页走。图片域名收不到 www.xiaohongshu.com 上的登录态。
    nativeErr(await native({
      op: 'add_task',
      url: image.url,
      name: DDMedia.imageName(title, image.n),
      headers: await buildHeaders(referrer, referrer),
      background: true,
    }));
    resource.added.push(image.n);
    publishMedia(tabId);
  }
  const ready = (resource.images || []).filter(img => img.url);
  resource.status = ready.length && ready.every(img => resource.added.includes(img.n)) ? 'added' : '';
}
async function readManifest(tabId, resource, url, kind) {
  const jobKey = `${tabId}:${url}`;
  if (manifestJobs.has(jobKey)) return;
  manifestJobs.add(jobKey);
  try {
    const response = await fetch(url, { credentials: "include", signal: AbortSignal.timeout(10000) });
    if (!response.ok || Number(response.headers.get("content-length")) > 2 * 1024 * 1024) return;
    const reader = response.body.getReader();
    let text = "", size = 0; const decoder = new TextDecoder();
    try {
      while (true) {
        const { value, done } = await reader.read(); if (done) break;
        size += value.length; if (size > 2 * 1024 * 1024) return;
        text += decoder.decode(value, { stream: true });
      }
    } finally { await reader.cancel().catch(() => {}); }
    if (kind === "hls" && !text.trimStart().startsWith("#EXTM3U")) return;
    const parsed = DDMedia.manifest(text, response.url, kind);
    const state = tabState(tabId);
    if (!state.resources.includes(resource)) return;
    resource.children = [...new Set([...(resource.children || []), ...parsed.children])].slice(0, 100);
    resource.segments = [...new Set([...(resource.segments || []), ...parsed.segments])].slice(0, 1000);
    if (parsed.unsupported) resource.unsupported = parsed.unsupported;
    if (parsed.master) {
      resource.sources.sort((a, b) => (b.url === url) - (a.url === url));
      const children = state.resources.filter(r => r !== resource && r.sources.some(s => resource.children.includes(s.url)));
      if (children.some(r => r.unsupported)) resource.unsupported = children.find(r => r.unsupported).unsupported;
      state.resources = state.resources.filter(r => !children.includes(r) && (r === resource || !r.sources.some(s => resource.segments.includes(s.url))));
      resource.sources = resource.sources.filter(s => !resource.children.includes(s.url));
    }
    publishMedia(tabId);
  } finally { manifestJobs.delete(jobKey); }
}
chrome.webRequest.onHeadersReceived.addListener(async details => {
  const mime = details.responseHeaders?.find(h => h.name.toLowerCase() === 'content-type')?.value || '';
  if (details.statusCode < 200 || details.statusCode >= 300) return;
  if (details.tabId < 0 || !DDMedia.classify(details.url, mime)) return;
  let referrer = details.initiator;
  try { referrer = (await chrome.webNavigation.getFrame({ tabId: details.tabId, frameId: details.frameId }))?.url || referrer; } catch { /* frame detached */ }
  recordMedia(details.tabId, details.url, mime, { referrer }).catch(() => {});
}, { urls: ['http://*/*', 'https://*/*'] }, ['responseHeaders']);
chrome.webNavigation.onCommitted.addListener(async d => {
  if (d.frameId !== 0) return;
  await mediaReady;
  const hidden = mediaTabs.get(d.tabId)?.hidden || false;
  mediaTabs.set(d.tabId, { resources: [], hidden });
  persistMedia();
});
chrome.webNavigation.onHistoryStateUpdated.addListener(async d => {
  if (d.frameId !== 0) return;
  // SPA transitions retain loaded resources: a playing video may issue no new request.
  await mediaReady;
  const page = DDMedia.page(d.url);
  const xhs = DDMedia.xhsPage(d.url);
  const state = tabState(d.tabId);
  // A new Bilibili video/part may reuse the same blob and player element.
  if (page) {
    state.resources = state.resources.filter(r => !r.sources.some(s => s.kind === 'site') || r.id === page);
    await recordMedia(d.tabId, page, '', { referrer: page });
  }
  // 离开这条笔记后，图片条目和它带出来的实况视频都不留在列表里。
  state.resources = state.resources.filter(r => {
    const images = r.sources.some(s => s.kind === 'images');
    if (!images && !r.group) return true;
    return Boolean(xhs) && (images ? r.id === xhs.page : r.group === xhs.page);
  });
  publishMedia(d.tabId);
});
chrome.tabs.onRemoved.addListener(async id => { await mediaReady; mediaTabs.delete(id); persistMedia(); });

async function handleMedia(request, sender) {
  await mediaReady;
  const tabId = sender.tab?.id ?? request.tabId;
  if (!Number.isInteger(tabId)) throw new Error('没有可用的网页标签页');
  const state = tabState(tabId);
  if (request.type === 'dd-media-list') return publicState(state);
  if (request.type === 'dd-media-found') {
    const parsed = request.xhs ? DDMedia.xhsNote(request.xhs, sender.url || '') : null;
    if (parsed) {
      recordNote(tabId, parsed);
      for (const video of parsed.videos) {
        await recordMedia(tabId, video.url, 'video/mp4', { title: video.title, referrer: parsed.page, group: parsed.page });
      }
    }
    for (const item of (request.items || []).slice(0, 100)) {
      await recordMedia(tabId, item.url, item.mime || '', { title: item.title, referrer: item.referrer || sender.url, group: item.group });
    }
    return { ok: true };
  }
  if (request.type === 'dd-media-visible') {
    state.hidden = !request.visible;
    publishMedia(tabId);
    return { ok: true };
  }
  const resource = state.resources.find(r => r.id === request.id);
  if (!resource) throw new Error('资源已失效，请重新播放视频');
  if (resource.unsupported) throw new Error(resource.unsupported === 'LIVE' ? '暂不支持直播，仅支持点播视频 / Live streams are not supported' : '暂不支持 DRM 保护的视频 / DRM video is not supported');
  const jobKey = `${tabId}:${resource.id}:${request.type}`;
  if (mediaJobs.has(jobKey)) return mediaJobs.get(jobKey);
  const job = (async () => {
    const tab = await chrome.tabs.get(tabId);
    const source = resource.sources[0];
    const referrer = resource.referrer || tab.url;
    const title = resource.title || request.title || tab.title || 'video';
    if (request.type === 'dd-media-inspect') {
      if (source.kind === 'images') throw new Error('图片没有清晰度选项');
      if (source.kind === 'file') return { formats: resource.sources.map(s => ({ format: s.url, label: s.label })), direct: true };
      const headers = await buildHeaders(source.url, referrer);
      return nativeErr(await native({ op: 'inspect_media', url: source.url, headers, background: true }));
    }
    if (request.type !== 'dd-media-download') throw new Error('未知资源操作');
    if (resource.status === 'added') return { ok: true };
    const images = source.kind === 'images' ? selectedImages(resource, request) : null;
    resource.status = 'sending'; resource.phase = source.kind === 'file' || source.kind === 'images' ? 'adding' : 'inspecting'; resource.error = ''; publishMedia(tabId);
    try {
      if (images) {
        await addImages(resource, images, title, referrer, tabId);
        return { ok: true };
      }
      let url = source.url;
      if (source.kind === 'file' && request.format) {
        if (!resource.sources.some(s => s.url === request.format)) throw new Error('清晰度已失效');
        url = request.format;
      }
      const container = request.container || 'mp4';
      if (!['mp4', 'mkv'].includes(container)) throw new Error('不支持的视频格式');
      const ext = source.kind === 'file' ? (/\.webm(?:\?|$)/i.test(url) ? 'webm' : /\.mov(?:\?|$)/i.test(url) ? 'mov' : 'mp4') : container;
      const body = { op: 'add_task', url, name: DDMedia.filename(title, ext), headers: await buildHeaders(url, referrer), background: true };
      if (source.kind !== 'file') {
        // Validate live/DRM and availability before acknowledging task creation.
        nativeErr(await native({ op: 'inspect_media', url, headers: body.headers, background: true }));
      }
      if (source.kind !== 'file') body.media = { format: request.format || 'bestvideo+bestaudio/best', container };
      resource.phase = 'adding'; publishMedia(tabId);
      await focusApp();
      nativeErr(await native(body));
      resource.status = 'added';
      return { ok: true };
    } catch (e) {
      resource.status = ''; resource.error = e.message || String(e); throw e;
    } finally { publishMedia(tabId); }
  })();
  mediaJobs.set(jobKey, job);
  try { return await job; } finally { mediaJobs.delete(jobKey); }
}
chrome.runtime.onMessage.addListener((request, sender, reply) => {
  if (!request?.type?.startsWith('dd-media-')) return;
  handleMedia(request, sender).then(reply, e => reply({ error: e.message || String(e) }));
  return true;
});
