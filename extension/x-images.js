(() => {
  if (window !== window.top || !DDMedia.xSite(location.href)) return;
  const seen = new Map();
  let enabled = false;
  // 从照片链接取帖子和序号，不按页面位置猜归属，避免把引用帖图片并入外层帖子。
  function report() {
    if (!enabled) return;
    const posts = new Map();
    for (const link of document.querySelectorAll('a[href*="/photo/"]')) {
      const loc = DDMedia.xPage(link.href);
      if (!loc || !loc.n) continue;
      const urls = [];
      for (const image of link.querySelectorAll('[data-testid="tweetPhoto"] img')) {
        const url = DDMedia.xImage(image.src);
        if (url) urls.push(url);
      }
      if (urls.length !== 1) continue;
      let post = posts.get(loc.page);
      if (!post) {
        const tweet = link.closest('[data-testid="tweet"]');
        const text = tweet?.querySelector('[data-testid="tweetText"]')?.textContent || '';
        post = { page: loc.page, title: text.trim() || `X-${loc.id}`, images: [] };
        posts.set(loc.page, post);
      }
      post.images.push({ n: loc.n, url: urls[0] });
    }
    for (const post of posts.values()) {
      const parsed = DDMedia.xPost(post, location.href);
      if (!parsed) continue;
      const sig = JSON.stringify(parsed);
      if (seen.get(parsed.page) === sig) continue;
      seen.set(parsed.page, sig);
      if (seen.size > 50) {
        const oldest = seen.keys().next().value;
        seen.delete(oldest);
      }
      chrome.runtime.sendMessage({ type: 'dd-media-found', x: parsed }).catch(() => {
        // 通道失败不算采集成功，下次 DOM 更新仍可重新上报。
        if (seen.get(parsed.page) === sig) seen.delete(parsed.page);
      });
    }
  }
  let timer;
  // X 信息流持续更新；限频而非等待停止变化，避免滚动时一直无法上报。
  function schedule() {
    if (timer) return;
    timer = setTimeout(() => { timer = null; report(); }, 300);
  }
  new MutationObserver(schedule).observe(document, {
    subtree: true, childList: true, attributes: true, attributeFilter: ['src', 'srcset', 'href'],
  });
  addEventListener('pageshow', schedule);
  chrome.storage.local.get({ enabled: true }, config => { enabled = config.enabled; schedule(); });
  chrome.storage.onChanged.addListener((changes, area) => {
    if (area !== 'local' || !changes.enabled) return;
    enabled = changes.enabled.newValue;
    seen.clear();
    if (enabled) schedule();
  });
})();
