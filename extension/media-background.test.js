const { test } = require('node:test');
const assert = require('node:assert/strict');
const vm = require('node:vm');
const fs = require('node:fs');
const DDMedia = require('./media.js');
function worker(native) {
  const noListener = { addListener() {} };
  const context = vm.createContext({
    DDMedia, URL, Map, Set, Number, String, Promise, Error, AbortSignal, TextDecoder,
    setTimeout: () => 0, clearTimeout() {}, cached: { enabled: true }, native,
    nativeErr: r => { if (r.ok === false) throw new Error(r.error); return r; },
    buildHeaders: async (_url, referrer) => [['Referer', referrer]],
    fetch: async () => { throw new Error('fixture has no remote manifest'); },
    chrome: {
      storage: { session: { get: async () => ({}), set: async () => {} } },
      tabs: { sendMessage: async () => {}, get: async () => ({ url: 'https://page.test/post/1', title: 'Page title' }), onRemoved: noListener },
      webRequest: { onHeadersReceived: noListener },
      webNavigation: { onCommitted: noListener, onHistoryStateUpdated: noListener },
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
  assert.equal(calls.length, 1);
  assert.equal(calls[0].background, true);
  assert.equal(calls[0].name, 'My video.mp4');
  assert.equal(calls[0].headers[0][1], 'https://page.test/post/1');
});
test('failed handoff remains retryable and foreign resource IDs cannot be submitted', async () => {
  let count = 0;
  const w = worker(async () => ++count === 1 ? { ok: false, error: 'app missing' } : { id: 2 });
  await w.recordMedia(1, 'https://cdn.test/video.mp4', 'video/mp4');
  const request = { type: 'dd-media-download', id: 'https://cdn.test/video.mp4' };
  await assert.rejects(w.handleMedia(request, { tab: { id: 1 } }), /app missing/);
  await w.handleMedia(request, { tab: { id: 1 } });
  assert.equal(count, 2);
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
