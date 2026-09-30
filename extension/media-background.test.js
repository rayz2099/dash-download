const { test } = require('node:test');
const assert = require('node:assert/strict');
const vm = require('node:vm');
const fs = require('node:fs');
const DDMedia = require('./media.js');
function worker(native) {
  const noListener = { addListener() {} };
  const events = {};
  const context = vm.createContext({
    events,
    DDMedia, URL, Map, Set, Number, String, Promise, Error, AbortSignal, TextDecoder,
    setTimeout: () => 0, clearTimeout() {}, cached: { enabled: true }, native,
    nativeErr: r => { if (r.ok === false) throw new Error(r.error); return r; },
    buildHeaders: async (url, referrer) => { events.headers = [url, referrer]; return [['Referer', referrer]]; },
    fetch: async () => { throw new Error('fixture has no remote manifest'); },
    chrome: {
      storage: { session: { get: async () => ({}), set: async () => {} } },
      tabs: { sendMessage: async () => {}, get: async () => ({ url: 'https://page.test/post/1', title: 'Page title' }), onRemoved: noListener },
      webRequest: { onHeadersReceived: noListener },
      webNavigation: { onCommitted: noListener, onHistoryStateUpdated: { addListener(fn) { events.history = fn; } } },
      runtime: { onMessage: noListener },
    },
  });
  vm.runInContext(fs.readFileSync(__dirname + '/media-background.js', 'utf8'), context);
  return context;
}
test('simultaneous clicks add exactly one background task, preserving referrer and filename', async () => {
  const calls = [];
  const w = worker(async req => { calls.push(req); await new Promise(r => setTimeout(r, 10)); return { id: 5 }; });
  await w.recordMedia(7, 'https://cdn.test/video.mp4', 'video/mp4', { title: 'My video', referrer: 'https://page.test/post/1' });
  const request = { type: 'dd-media-download', id: 'https://cdn.test/video.mp4' };
  await Promise.all([w.handleMedia(request, { tab: { id: 7 } }), w.handleMedia(request, { tab: { id: 7 } })]);
  await w.handleMedia(request, { tab: { id: 7 } });
  assert.deepEqual(calls.map(r => r.op), ['focus', 'add_task']);
  assert.equal(calls[1].background, true);
  assert.equal(calls[1].name, 'My video.mp4');
  assert.equal(calls[1].headers[0][1], 'https://page.test/post/1');
});

test('Bilibili SPA navigation replaces the previous video or part without duplicating tracking URLs', async () => {
  const w = worker(async () => ({}));
  const first = 'https://www.bilibili.com/video/BV1PZ9UBjEsH/';
  await w.recordMedia(7, first, '');
  await w.events.history({ tabId: 7, frameId: 0, url: first + '?p=2' });
  await w.events.history({ tabId: 7, frameId: 0, url: first + '?p=2&vd_source=tracking' });
  const state = await w.handleMedia({ type: 'dd-media-list' }, { tab: { id: 7 } });
  assert.equal(state.resources.length, 1);
  assert.equal(state.resources[0].sources[0].url, first + '?p=2');
});
test('failed handoff remains retryable and foreign resource IDs cannot be submitted', async () => {
  let adds = 0;
  const w = worker(async req => {
    if (req.op === 'focus') return { ok: true };
    return ++adds === 1 ? { ok: false, error: 'app missing' } : { id: 2 };
  });
  await w.recordMedia(1, 'https://cdn.test/video.mp4', 'video/mp4');
  const request = { type: 'dd-media-download', id: 'https://cdn.test/video.mp4' };
  await assert.rejects(w.handleMedia(request, { tab: { id: 1 } }), /app missing/);
  await w.handleMedia(request, { tab: { id: 1 } });
  assert.equal(adds, 2);
  await assert.rejects(w.handleMedia({ ...request, id: 'https://foreign.test/a' }, { tab: { id: 1 } }));
});
test('live manifest is rejected before creating a desktop task', async () => {
  const calls = [];
  const w = worker(async req => { calls.push(req); return { ok: false, error: 'live unsupported' }; });
  await w.recordMedia(1, 'https://cdn.test/master.m3u8', 'application/vnd.apple.mpegurl');
  await assert.rejects(w.handleMedia({ type: 'dd-media-download', id: 'https://cdn.test/master.m3u8' }, { tab: { id: 1 } }), /live unsupported/);
  assert.deepEqual(calls.map(r => r.op), ['inspect_media']);
});

test('manifest downloads default to MP4 and honor an explicit MKV choice', async () => {
  for (const container of [undefined, 'mkv']) {
    const calls = [];
    const w = worker(async req => { calls.push(req); return req.op === 'inspect_media' ? { formats: [] } : { id: 5 }; });
    await w.recordMedia(7, 'https://cdn.test/master.m3u8', 'application/vnd.apple.mpegurl');
    await w.handleMedia({ type: 'dd-media-download', id: 'https://cdn.test/master.m3u8', container }, { tab: { id: 7 } });
    const task = calls.find(r => r.op === 'add_task');
    assert.equal(task.media.container, container || 'mp4');
    assert.ok(task.name.endsWith('.' + (container || 'mp4')));
  }
});

test('Bilibili hands the canonical page to the media engine for audio/video merging', async () => {
  const calls = [];
  const w = worker(async req => { calls.push(req); return req.op === 'inspect_media' ? { formats: [] } : { id: 9 }; });
  const page = 'https://www.bilibili.com/video/BV1PZ9UBjEsH/?p=2';
  await w.recordMedia(7, page + '&vd_source=tracking', '', { title: 'Bilibili video', referrer: page });
  const state = await w.handleMedia({ type: 'dd-media-list' }, { tab: { id: 7 } });
  assert.equal(state.resources.length, 1);
  await w.handleMedia({ type: 'dd-media-download', id: state.resources[0].id }, { tab: { id: 7 } });
  assert.deepEqual(calls.map(r => r.op), ['inspect_media', 'focus', 'add_task']);
  assert.equal(calls[2].url, page);
  assert.equal(calls[2].media.format, 'bestvideo+bestaudio/best');
  assert.equal(calls[2].name, 'Bilibili video.mp4');
  assert.equal(calls[2].headers[0][1], page);
});

test('Xiaohongshu note downloads only the selected originals and keeps the live clip on the video path', async () => {
  const calls = [];
  const w = worker(async req => { calls.push(req); return { id: calls.length }; });
  const id = '6abb25350000000018018728';
  const token = 'CBLPc7ZFwMneaF--Us_-C3saHSIxtHHpOmEs-9RZzHwew=';
  const page = `https://www.xiaohongshu.com/explore/${id}?xsec_token=${encodeURIComponent(token)}&xsec_source=app_share`;
  const file = '1040g3k031qtmfr5pno004a62p2ihj0itmhtlqk0';
  const live = '1040g2sg31exampleimage00000002';
  const payload = { note: { noteDetailMap: { [id]: { note: { noteId: id, type: 'normal', title: '示例笔记', imageList: [
    { traceId: file, urlDefault: `https://sns-webpic-qc.xhscdn.com/202601131644/e30d294b3ad5957a22d12421fac6da64/notes_pre_post/${file}!nc_n_webp_mw_1` },
    { traceId: live, urlDefault: `https://sns-webpic-qc.xhscdn.com/202601131644/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/${live}!nd_dft_wlteh_webp_3`, stream: { h265: [{ masterUrl: 'https://sns-video-bd.xhscdn.com/stream/1/110/live-h265.mp4', avgBitrate: 900 }], h264: [{ masterUrl: 'https://sns-video-bd.xhscdn.com/stream/1/110/live.mp4', avgBitrate: 100 }] } },
    { urlDefault: 'https://evil.test/watermark.jpg' },
  ] } } } } };
  await w.handleMedia({ type: 'dd-media-found', xhs: payload }, { tab: { id: 7 }, url: page });
  await w.handleMedia({ type: 'dd-media-found', xhs: payload }, { tab: { id: 7 }, url: page });
  let state = await w.handleMedia({ type: 'dd-media-list' }, { tab: { id: 7 } });
  assert.equal(state.resources.length, 2);
  assert.equal(state.resources.filter(r => r.sources.some(s => s.kind === 'images')).length, 1);
  const note = state.resources.find(r => r.sources.some(s => s.kind === 'images'));
  const clip = state.resources.find(r => r.sources.some(s => s.kind === 'file'));
  assert.equal(note.images.length, 3);
  assert.equal(clip.sources[0].url, 'https://sns-video-bd.xhscdn.com/stream/1/110/live-h265.mp4');
  await assert.rejects(w.handleMedia({ type: 'dd-media-download', id: note.id, images: [3] }, { tab: { id: 7 } }), /无水印原图不可用/);
  await assert.rejects(w.handleMedia({ type: 'dd-media-download', id: note.id, images: [] }, { tab: { id: 7 } }), /没有选中的图片/);
  assert.equal(calls.length, 0);
  await w.handleMedia({ type: 'dd-media-download', id: note.id, images: [2] }, { tab: { id: 7 } });
  assert.deepEqual(calls.map(r => r.op), ['focus', 'add_task']);
  assert.equal(calls[1].url, `https://ci.xiaohongshu.com/${live}`);
  assert.equal(calls[1].name, '示例笔记-02');
  assert.equal(calls[1].media, undefined);
  const canonical = `https://www.xiaohongshu.com/explore/${id}?xsec_token=${encodeURIComponent(token)}`;
  assert.equal(calls[1].headers[0][1], canonical);
  assert.deepEqual(w.events.headers, [canonical, canonical]);
  await w.events.history({ tabId: 7, frameId: 0, url: 'https://www.xiaohongshu.com/explore/aaaaaaaaaaaaaaaaaaaaaaaa' });
  state = await w.handleMedia({ type: 'dd-media-list' }, { tab: { id: 7 } });
  assert.equal(state.resources.length, 0);
});
