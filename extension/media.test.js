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

test('Bilibili video pages use the site extractor, keeping only the selected part', () => {
  const page = 'https://www.bilibili.com/video/BV1PZ9UBjEsH/?p=2&vd_source=tracking';
  assert.equal(media.classify(page), 'site');
  assert.equal(media.page(page), 'https://www.bilibili.com/video/BV1PZ9UBjEsH/?p=2');
  assert.notEqual(media.key(page), media.key('https://www.bilibili.com/video/BV1PZ9UBjEsH/'));
  for (const url of ['https://www.bilibili.com.evil.test/video/BV1PZ9UBjEsH/', 'https://live.bilibili.com/123', 'https://www.bilibili.com/video/not-a-video']) {
    assert.equal(media.page(url), null);
    assert.equal(media.classify(url), null);
  }
  assert.equal(media.classify('https://upos-sz-estghw.bilivideo.com/upgcxcode/36/28/37937742836/37937742836-1-30080.m4s'), null);
});

const xhsId = '6abb25350000000018018728';
const xhsToken = 'CBLPc7ZFwMneaF--Us_-C3saHSIxtHHpOmEs-9RZzHwew=';
const xhsPage = `https://www.xiaohongshu.com/explore/${xhsId}?xsec_token=${encodeURIComponent(xhsToken)}`;
const xhsFile = '1040g3k031qtmfr5pno004a62p2ihj0itmhtlqk0';
const xhsLive = '1040g2sg31exampleimage00000002';
function xhsState() {
  return {
    note: {
      noteDetailMap: {
        [xhsId]: {
          note: {
            noteId: xhsId,
            type: 'normal',
            title: '示例笔记',
            imageList: [
              {
                traceId: xhsFile,
                urlDefault: `https://sns-webpic-qc.xhscdn.com/202601131644/e30d294b3ad5957a22d12421fac6da64/notes_pre_post/${xhsFile}!nc_n_webp_mw_1`,
              },
              {
                traceId: xhsLive,
                urlDefault: `https://sns-webpic-qc.xhscdn.com/202601131644/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/${xhsLive}!nd_dft_wlteh_webp_3`,
                stream: {
                  h264: [{ masterUrl: 'https://sns-video-bd.xhscdn.com/stream/1/110/live.mp4', avgBitrate: 100, width: 720, height: 1280 }],
                  h265: [{ masterUrl: 'https://sns-video-bd.xhscdn.com/stream/1/110/live-h265.mp4', avgBitrate: 900, width: 1080, height: 1920 }],
                },
              },
              { urlDefault: 'https://evil.test/watermark.jpg' },
            ],
          },
        },
      },
    },
  };
}

test('Xiaohongshu image notes keep the share token and only emit watermark-free originals', () => {
  const raw = `https://www.xiaohongshu.com/explore/${xhsId}?xsec_token=${xhsToken}&xsec_source=app_share`;
  assert.equal(media.xhsPage(raw).page, xhsPage);
  assert.equal(media.xhsPage(`https://www.xiaohongshu.com.evil.test/explore/${xhsId}`), null);
  assert.equal(media.xhsPage('https://www.xiaohongshu.com/user/profile/abc'), null);
  const note = media.xhsNote(xhsState(), raw);
  assert.equal(note.id, xhsId);
  assert.equal(note.page, xhsPage);
  assert.equal(note.images.length, 3);
  assert.equal(note.images[0].url, `https://ci.xiaohongshu.com/notes_pre_post/${xhsFile}`);
  assert.equal(note.images[1].url, `https://ci.xiaohongshu.com/${xhsLive}`);
  assert.equal(note.images[2].error, 'nowm');
  for (const image of note.images) {
    if (!image.url) continue;
    assert.equal(media.xhsOriginOk(image.url), true);
    assert.equal(image.url.includes('!'), false);
    assert.equal(image.url.includes('imageView'), false);
    assert.equal(image.url.includes('sns-webpic'), false);
  }
  assert.deepEqual(note.videos.map(v => v.url), ['https://sns-video-bd.xhscdn.com/stream/1/110/live-h265.mp4']);
  assert.equal(media.xhsNote(xhsState(), 'https://www.bilibili.com/video/BV1PZ9UBjEsH/'), null);
  const videoNote = xhsState();
  videoNote.note.noteDetailMap[xhsId].note.type = 'video';
  assert.equal(media.xhsNote(videoNote, raw), null);
  const images = note.images;
  assert.deepEqual(media.xhsPick(images, false, []), [1, 2]);
  assert.deepEqual(media.xhsPick(images, true, [2]), [2]);
  assert.deepEqual(media.xhsPick(images, true, []), []);
  assert.equal(media.imageName('示例笔记', 1), '示例笔记-01');
});
