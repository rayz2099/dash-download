const { test } = require('node:test');
const assert = require('node:assert/strict');
const vm = require('node:vm');
const fs = require('node:fs');
const DDMedia = require('./media.js');

// 模拟 X 的照片链接边界，让测试覆盖真实采集脚本而不是复制一份采集逻辑。
function collector(links, href = 'https://x.com/home') {
  const messages = [], events = {}, timers = [];
  const win = {};
  win.top = win;
  const context = vm.createContext({
    DDMedia, window: win, location: { href }, Map, JSON,
    document: { querySelectorAll: () => links },
    MutationObserver: class { constructor(fn) { events.mutate = fn; } observe() {} },
    setTimeout: fn => { timers.push(fn); return timers.length; },
    addEventListener: (name, fn) => { events[name] = fn; },
    chrome: {
      runtime: { sendMessage: async message => { messages.push(message); return {}; } },
      storage: {
        local: { get: (_, fn) => fn({ enabled: true }) },
        onChanged: { addListener(fn) { events.storage = fn; } },
      },
    },
  });
  const source = fs.readFileSync(__dirname + '/x-images.js', 'utf8');
  vm.runInContext(source, context);
  const flush = () => { const pending = timers.splice(0); for (const fn of pending) fn(); };
  return { messages, events, flush };
}
function photo(id, n, url, title = '测试帖子') {
  return {
    href: `https://x.com/test/status/${id}/photo/${n}`,
    querySelectorAll: () => [{ currentSrc: url, src: url }],
    closest: () => ({ querySelector: () => ({ textContent: title }) }),
  };
}

test('X photo links group by post, exclude covers and report originals once', () => {
  const links = [
    photo('100', 2, 'https://pbs.twimg.com/media/Second?format=png&name=small'),
    photo('100', 1, 'https://pbs.twimg.com/media/First.jpg:small'),
    photo('200', 1, 'https://pbs.twimg.com/media/Quoted?format=jpg&name=large', '引用帖'),
    photo('300', 1, 'https://pbs.twimg.com/ext_tw_video_thumb/300/cover.jpg'),
    photo('400', 1, 'https://pbs.twimg.com/profile_images/400/avatar.jpg'),
  ];
  const c = collector(links);
  c.flush();
  assert.equal(c.messages.length, 2);
  assert.equal(c.messages[0].x.page, 'https://x.com/i/web/status/100');
  assert.equal(c.messages[0].x.images[0].n, 1);
  assert.equal(c.messages[0].x.images[1].url, 'https://pbs.twimg.com/media/Second?format=png&name=orig');
  assert.equal(c.messages[1].x.title, '引用帖');
  c.events.mutate();
  c.flush();
  assert.equal(c.messages.length, 2);
  links.push(photo('100', 3, 'https://pbs.twimg.com/media/Third?format=jpg&name=medium'));
  c.events.mutate();
  c.flush();
  assert.equal(c.messages.length, 3);
  assert.equal(c.messages[2].x.images.length, 3);
});

test('Twitter detail pages collect photos and enabling again rescans unchanged DOM', () => {
  const link = photo('100', 1, 'https://pbs.twimg.com/media/First.jpg');
  link.href = 'https://twitter.com/test/status/100/photo/1';
  const c = collector([link], 'https://twitter.com/test/status/100');
  c.flush();
  assert.equal(c.messages.length, 1);
  c.events.storage({ enabled: { newValue: false } }, 'local');
  c.events.mutate();
  c.flush();
  assert.equal(c.messages.length, 1);
  c.events.storage({ enabled: { newValue: true } }, 'local');
  c.flush();
  assert.equal(c.messages.length, 2);
});

test('foreign pages do not install X collection', () => {
  const c = collector([], 'https://x.com.evil.test/home');
  c.flush();
  assert.equal(c.messages.length, 0);
  assert.equal(c.events.mutate, undefined);
});
