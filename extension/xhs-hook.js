// 笔记数据只在页面自己的 __INITIAL_STATE__ 里。这里不重放要签名的接口。
(() => {
  const noteId = () => (location.pathname.match(/^\/explore\/([0-9a-f]{24})\/?$/i) || [])[1];
  // 隔离世界的监听可能晚于第一次赋值。重复投递由那边按笔记内容去重。
  function publish(payload) {
    try { JSON.stringify(payload); } catch { return; }
    window.postMessage({ source: 'dd-xhs', payload }, location.origin);
  }
  function snapshot() {
    const id = noteId();
    const state = window.__INITIAL_STATE__;
    const slot = id && state && state.note && state.note.noteDetailMap && state.note.noteDetailMap[id];
    const note = slot && (slot.note || slot.note_card || slot.noteCard);
    if (!note) return;
    let imageList;
    try { imageList = JSON.parse(JSON.stringify(note.imageList || note.image_list || null)); } catch { return; }
    publish({
      note: {
        noteDetailMap: {
          [id]: {
            note: {
              noteId: note.noteId || note.note_id || id,
              type: note.type || note.note_type,
              title: note.title || '',
              displayTitle: note.displayTitle || note.display_title || '',
              desc: typeof note.desc === 'string' ? note.desc.slice(0, 300) : '',
              imageList,
            },
          },
        },
      },
    });
  }
  try {
    let stored = window.__INITIAL_STATE__;
    Object.defineProperty(window, '__INITIAL_STATE__', {
      configurable: true,
      enumerable: true,
      get() { return stored; },
      set(value) { stored = value; snapshot(); },
    });
  } catch { /* 页面已占用该属性时，下面的轮询仍能读到 */ }
  const push = history.pushState, replace = history.replaceState;
  history.pushState = function () { const result = push.apply(this, arguments); snapshot(); return result; };
  history.replaceState = function () { const result = replace.apply(this, arguments); snapshot(); return result; };
  addEventListener('popstate', snapshot);
  setInterval(() => { if (noteId()) snapshot(); }, 700);
})();
