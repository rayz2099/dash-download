const { test } = require('node:test');
const assert = require('node:assert/strict');
const media = require('./media.js');
test('classifies extensionless manifests by MIME and rejects blobs and fragments', () => {
  assert.equal(media.classify('https://cdn.test/play?id=1', 'application/dash+xml'), 'dash');
  assert.equal(media.classify('https://cdn.test/play', 'application/vnd.apple.mpegurl'), 'hls');
  for (const path of ['blob:https://x.com/id','https://cdn.test/seg-1.mp4','https://cdn.test/chunk_2.m4s','https://cdn.test/segment1.ts']) assert.equal(media.classify(path), null);
  assert.equal(media.classify('https://cdn.test/video.webm'), 'file');
});
test('X resolutions and master playlist share a video identity', () => {
  const base = 'https://video.twimg.com/amplify_video/2102449230413725697/';
  assert.equal(media.key(base+'pl/master.m3u8?tag=29'), media.key(base+'vid/avc1/1280x720/video.mp4?tag=29'));
  assert.equal(media.label(base+'vid/avc1/1280x720/video.mp4?tag=29'), '720p');
  assert.notEqual(media.key('https://cdn.test/play?id=1'),media.key('https://cdn.test/play?id=2'));
});
test('HLS master joins alternate audio and relative variants without treating it as live', () => {
  const info = media.manifest('#EXTM3U\n#EXT-X-MEDIA:TYPE=AUDIO,URI="audio.m3u8?token=a"\n#EXT-X-STREAM-INF:BANDWIDTH=200\n../video/720.m3u8', 'https://cdn.test/list/master.m3u8', 'hls');
  assert.equal(info.unsupported, '');
  assert.deepEqual(info.children,['https://cdn.test/list/audio.m3u8?token=a','https://cdn.test/video/720.m3u8']);
});
test('detects live and DRM without rejecting ordinary AES-128', () => {
  assert.equal(media.manifest('#EXTM3U\n#EXTINF:2\nx.ts','https://cdn.test/a.m3u8','hls').unsupported,'LIVE');
  assert.equal(media.manifest('#EXTM3U\n#EXT-X-KEY:METHOD=AES-128,URI="key"\n#EXT-X-ENDLIST','https://cdn.test/a.m3u8','hls').unsupported,'');
  assert.equal(media.manifest('#EXTM3U\n#EXT-X-KEY:METHOD=SAMPLE-AES\n#EXT-X-ENDLIST','https://cdn.test/a.m3u8','hls').unsupported,'DRM');
  assert.equal(media.manifest('<MPD type="dynamic"/>','https://cdn.test/a.mpd','dash').unsupported,'LIVE');
  assert.equal(media.manifest('<MPD><c:ContentProtection/></MPD>','https://cdn.test/a.mpd','dash').unsupported,'DRM');
});
test('safe names retain the requested extension', () => {
  assert.equal(media.filename('../title:video','mp4'),'.._title_video.mp4');
});
