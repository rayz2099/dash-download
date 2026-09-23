const { test } = require('node:test');
const assert = require('node:assert/strict');
const vm = require('node:vm');
const fs = require('node:fs');
const DDMedia = require('./media.js');

test('a Bilibili blob player reports its page, including a newly selected part', () => {
  const messages = [], events = {};
  const video = {
    currentSrc: 'blob:https://www.bilibili.com/player', src: '', title: '',
    getAttribute: () => null, closest: () => null, querySelectorAll: () => [],
  };
  const location = { href: 'https://www.bilibili.com/video/BV1PZ9UBjEsH/?vd_source=tracking' };
  const context = vm.createContext({
    DDMedia, location, Map, window: { top: {} },
    document: {
      title: 'Video title',
      querySelector: () => video, querySelectorAll: () => [video],
      addEventListener: (name, fn) => { events[name] = fn; },
    },
    MutationObserver: class { observe() {} }, addEventListener() {},
    chrome: {
      i18n: { getUILanguage: () => 'en' },
      runtime: { sendMessage: async message => { messages.push(message); return {}; } },
    },
  });
  vm.runInContext(fs.readFileSync(__dirname + '/media-content.js', 'utf8'), context);
  events.loadedmetadata();
  events.play();
  assert.equal(messages.length, 1);
  assert.equal(messages[0].items[0].url, 'https://www.bilibili.com/video/BV1PZ9UBjEsH/');
  location.href = 'https://www.bilibili.com/video/BV1PZ9UBjEsH/?p=2';
  events.loadedmetadata();
  assert.equal(messages.length, 2);
  assert.equal(messages[1].items[0].url, location.href);
});
