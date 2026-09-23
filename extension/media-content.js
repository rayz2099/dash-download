(() => {
  const zh = chrome.i18n.getUILanguage().startsWith('zh');
  const t = (cn, en) => zh ? cn : en;
  const send = async data => {
    const result = await chrome.runtime.sendMessage(data);
    if (result?.error) throw new Error(result.error);
    return result;
  };
  const seen = new Map();
  function report() {
    const items = [];
    const page = DDMedia.page(location.href);
    if (page && document.querySelector('video') && seen.get(page) !== document.title) {
      seen.set(page, document.title);
      items.push({ url: page, title: document.title });
    }
    for (const video of document.querySelectorAll('video')) {
      const title = video.getAttribute('aria-label') || video.title || video.closest('article')?.querySelector('[data-testid="tweetText"]')?.textContent || document.title;
      for (const url of [video.currentSrc, video.src, ...[...video.querySelectorAll('source')].map(s => s.src)]) {
        if (!DDMedia.classify(url) || seen.get(url) === title) continue;
        seen.set(url, title);
        items.push({ url, title });
      }
    }
    if (items.length) send({ type: 'dd-media-found', items }).catch(() => {});
  }
  // Browser network observation also covers MSE/blob players and cross-origin iframes.
  document.addEventListener('loadedmetadata', report, true);
  document.addEventListener('play', report, true);
  let scanTimer;
  new MutationObserver(() => {
    clearTimeout(scanTimer); scanTimer = setTimeout(report, 400);
  }).observe(document, { subtree: true, childList: true, attributes: true, attributeFilter: ['src'] });
  addEventListener('pageshow', report);
  if (window !== window.top) return;

  let state = { resources: [], hidden: false }, opened = false, enabled = true;
  const choices = new Map();
  const host = document.createElement('div');
  host.style.cssText = 'all:initial;position:fixed!important;right:20px!important;bottom:20px!important;z-index:2147483647!important;display:none;';
  host.style.setProperty("font", '13px/1.5 -apple-system,BlinkMacSystemFont,"Segoe UI",sans-serif', "important");
  const shadow = host.attachShadow({ mode: 'closed' });
  shadow.innerHTML = `<style>
    :host{color-scheme:light dark;font:13px/1.5 -apple-system,BlinkMacSystemFont,"Segoe UI",sans-serif;color:#202124}
    *{box-sizing:border-box}button,select{font:inherit}button{cursor:pointer}button:disabled{cursor:default;opacity:.6}
    button:focus-visible,select:focus-visible{outline:2px solid #2563eb;outline-offset:3px}
    .trigger{border:1px solid #d9dee7;border-radius:24px;padding:10px 16px;background:#fff;color:#202124;box-shadow:0 3px 16px #0002;touch-action:none;user-select:none}
    .panel{width:min(350px,calc(100vw - 24px));background:#fff;color:#202124;border:1px solid #d9dee7;border-radius:14px;box-shadow:0 8px 36px #0003;overflow:hidden;margin-bottom:10px}
    header{display:flex;align-items:center;gap:8px;padding:14px 16px;border-bottom:1px solid #e7e9ee;cursor:move;touch-action:none;user-select:none}
    header strong{flex:1;font-size:14px}header button{border:0;background:none;color:inherit;font-size:20px;line-height:1;padding:4px}
    .list{max-height:min(440px,55vh);overflow:auto;overscroll-behavior:contain}.empty{padding:24px 16px;color:#737985}
    article{padding:14px 16px;border-bottom:1px solid #e7e9ee}article:last-child{border-bottom:0}
    .title{font-weight:600;overflow-wrap:anywhere;display:-webkit-box;-webkit-line-clamp:2;-webkit-box-orient:vertical;overflow:hidden}
    .meta{font-size:11px;color:#737985;margin:3px 0 10px}.actions{display:flex;gap:8px}select{min-width:0;flex:1;border:1px solid #d9dee7;border-radius:7px;padding:7px;background:#f8f9fb;color:inherit}
    .download{border:0;border-radius:7px;background:#2563eb;color:#fff;padding:7px 13px;white-space:nowrap}
    .quality{border:0;background:none;color:#2563eb;font-size:12px;padding:5px 0}.feedback{font-size:12px;margin-top:6px;color:#b34125;overflow-wrap:anywhere}
    footer{padding:10px 16px;background:#f8f9fb;color:#737985;font-size:11px}
    [hidden]{display:none!important}
    @media(prefers-color-scheme:dark){:host{color:#f1f3f5}.trigger,.panel{background:#20242b;color:#f1f3f5;border-color:#3d434e}header,article{border-color:#3d434e}select,footer{background:#282d35;color:#bdc4d0;border-color:#3d434e}.meta,.empty{color:#a4aebb}.quality{color:#82aaff}.feedback{color:#ffa28a}}
  </style><section class="panel" aria-label="${t('视频资源', 'Video resources')}" hidden>
    <header><strong>Dash Download</strong><button class="collapse" aria-label="${t('收起', 'Collapse')}">−</button><button class="close" aria-label="${t('隐藏悬浮入口', 'Hide widget')}">×</button></header>
    <div class="list"></div><footer>${t('任务进度在桌面应用中查看', 'Manage downloads in the desktop app')}</footer>
  </section><button class="trigger" aria-expanded="false"></button>`;
  const panel = shadow.querySelector('.panel'), trigger = shadow.querySelector('.trigger'), list = shadow.querySelector('.list');
  function mount() { if (!host.isConnected && document.documentElement) document.documentElement.append(host); }
  function visibility() {
    mount();
    host.style.setProperty('display', enabled && !state.hidden && (state.resources.length || opened) ? 'block' : 'none', 'important');
    panel.hidden = !opened; trigger.setAttribute('aria-expanded', String(opened));
    trigger.textContent = `↓ ${t('视频', 'Videos')} · ${state.resources.length}`;
  }
  function element(tag, className, text) {
    const node = document.createElement(tag); if (className) node.className = className; if (text) node.textContent = text; return node;
  }
  function render() {
    visibility();
    list.replaceChildren();
    if (!state.resources.length) { list.append(element('div', 'empty', t('还没有发现视频，请先播放页面上的视频。', 'No videos found yet. Play a video on this page.'))); return; }
    for (const resource of state.resources) {
      const article = element('article');
      const title = resource.title || document.title || t('视频', 'Video');
      article.append(element('div', 'title', title));
      const source = resource.sources[0];
      article.append(element('div', 'meta', `${new URL(source.url).hostname} · ${source.kind.toUpperCase()}`));
      let choice = choices.get(resource.id);
      if (!choice) { choice = { value: '', container: 'mp4', formats: [], loading: false, error: '' }; choices.set(resource.id, choice); }
      const actions = element('div', 'actions'), select = document.createElement('select');
      select.setAttribute('aria-label', t('清晰度', 'Quality'));
      select.append(new Option(t('最高画质（默认）', 'Best quality (default)'), ''));
      for (const f of choice.formats) select.append(new Option(f.height ? `${f.height}p · ${f.ext || ''}` : f.label || f.format, f.format));
      select.value = choice.value;
      select.disabled = choice.loading || resource.status === 'sending' || resource.status === 'added';
      select.addEventListener('change', () => { choice.value = select.value; });
      const button = element('button', 'download', resource.status === 'added' ? t('已添加', 'Added') : resource.status === 'sending' ? (resource.phase === 'inspecting' ? t('解析中…', 'Inspecting…') : t('添加中…', 'Adding…')) : t('下载', 'Download'));
      button.disabled = ['added', 'sending'].includes(resource.status) || !!resource.unsupported;
      button.addEventListener('click', async () => {
        choice.error = ''; button.disabled = true; button.textContent = t('添加中…', 'Adding…');
        try { await send({ type: 'dd-media-download', id: resource.id, format: choice.value, container: choice.container, title }); }
        catch (e) { choice.error = e.message; render(); }
      });
      actions.append(select, button);
      if (source.kind !== 'file') {
        const output = document.createElement('select');
        output.setAttribute('aria-label', t('视频格式', 'Video format'));
        output.append(new Option('MP4', 'mp4'), new Option('MKV', 'mkv'));
        output.value = choice.container;
        output.disabled = ['added', 'sending'].includes(resource.status);
        output.addEventListener('change', () => { choice.container = output.value; });
        const containerRow = element('div', 'actions');
        containerRow.style.marginBottom = '8px';
        containerRow.append(element('span', 'meta', t('保存格式', 'Save as')), output);
        article.append(containerRow);
      }
      article.append(actions);
      if (!choice.formats.length && resource.status !== 'added') {
        const quality = element('button', 'quality', choice.loading ? t('读取清晰度…', 'Loading qualities…') : t('选择清晰度', 'Choose quality'));
        quality.disabled = choice.loading;
        quality.addEventListener('click', async () => {
          choice.loading = true; choice.error = ''; render();
          try { const result = await send({ type: 'dd-media-inspect', id: resource.id }); choice.formats = result.formats; }
          catch (e) { choice.error = e.message; }
          finally { choice.loading = false; render(); }
        });
        article.append(quality);
      }
      const error = resource.unsupported ? (resource.unsupported === "LIVE" ? t("暂不支持直播，仅支持点播视频", "Live streams are not supported") : t("暂不支持 DRM 保护的视频", "DRM video is not supported")) : resource.error || choice.error;
      if (error) { const feedback = element('div', 'feedback', error); feedback.setAttribute('role', 'status'); article.append(feedback); }
      list.append(article);
    }
    clampPosition();
  }
  trigger.addEventListener('click', () => { if (dragged) { dragged = false; return; } opened = !opened; render(); });
  shadow.querySelector('.collapse').onclick = () => { opened = false; render(); trigger.focus(); };
  shadow.querySelector('.close').onclick = () => { state.hidden = true; opened = false; visibility(); send({ type: 'dd-media-visible', visible: false }).catch(() => {}); };
  shadow.addEventListener('keydown', e => { if (e.key === 'Escape') { opened = false; visibility(); trigger.focus(); } });
  let dragging = null, dragged = false;
  function clampPosition() {
    if (!Number.isFinite(parseFloat(host.style.left))) return;
    const r = host.getBoundingClientRect();
    host.style.setProperty('left', `${Math.max(8, Math.min(r.left, innerWidth - r.width - 8))}px`, 'important');
    host.style.setProperty('top', `${Math.max(8, Math.min(r.top, innerHeight - r.height - 8))}px`, 'important');
  }
  for (const handle of [trigger, shadow.querySelector('header')]) {
    handle.addEventListener('pointerdown', e => {
      if (e.button !== 0 || !e.isPrimary || (handle !== trigger && e.target.closest('button'))) return;
      dragging = { x: e.clientX, y: e.clientY, rect: host.getBoundingClientRect() }; dragged = false; handle.setPointerCapture(e.pointerId);
    });
    handle.addEventListener('pointermove', e => {
      if (!dragging) return;
      const dx = e.clientX - dragging.x, dy = e.clientY - dragging.y;
      if (Math.abs(dx) + Math.abs(dy) < 5 && !dragged) return;
      dragged = true;
      host.style.setProperty('right', 'auto', 'important'); host.style.setProperty('bottom', 'auto', 'important');
      host.style.setProperty('left', `${dragging.rect.left + dx}px`, 'important'); host.style.setProperty('top', `${dragging.rect.top + dy}px`, 'important'); clampPosition();
    });
    handle.addEventListener('pointerup', () => { dragging = null; });
    handle.addEventListener('pointercancel', () => { dragging = null; dragged = false; });
  }
  addEventListener('resize', clampPosition);
  chrome.runtime.onMessage.addListener(request => {
    if (request.type === 'dd-media-update') { state = request.state; render(); }
    if (request.type === 'dd-media-open') { opened = true; state.hidden = false; render(); }
  });
  chrome.storage.local.get({ enabled: true }, c => { enabled = c.enabled; visibility(); });
  chrome.storage.onChanged.addListener((c, area) => { if (area === 'local' && c.enabled) { enabled = c.enabled.newValue; visibility(); } });
  send({ type: 'dd-media-list' }).then(s => { state = s; render(); }).catch(() => {});
})();
