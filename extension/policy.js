// 纯策略, 给 SW importScripts 与 node:test 共用.
(function (g) {
  /// 与引擎 MAX_IMPORT_BYTES 对齐. JSON base64 约 4/3, app body 上限 32MB.
  const MAX_INLINE_BYTES = 24 * 1024 * 1024;

  /// policy 同时跑在 Service Worker 与 Node 测试中，不能直接依赖 chrome 全局。
  function policyMsg(zh, en) {
    if (typeof chrome === "undefined" || !chrome.i18n) return en;
    return chrome.i18n.getUILanguage().toLowerCase().startsWith("zh") ? zh : en;
  }

  function isBlobLike(url) {
    return /^(blob|data|filesystem):/i.test(url || "");
  }

  /// Chrome 把跨进程的 blob 报成 blob:null/<uuid>, 页面侧仍是 blob:<origin>/<uuid>.
  function blobId(url) {
    const s = url || "";
    const i = s.lastIndexOf("/");
    return i >= 0 ? s.slice(i + 1) : s;
  }

  function itemKey(url) {
    if (isBlobLike(url)) return "blob:" + blobId(url);
    return url || "";
  }

  function hostOf(url) {
    try {
      return new URL(url).hostname.toLowerCase();
    } catch (_) {
      return "";
    }
  }

  /// host 命中黑名单自身或子域. `*.evil.com` 与 `evil.com` 同义.
  function hostDenied(host, denyHosts) {
    if (!host) return false;
    const list = denyHosts || [];
    for (let i = 0; i < list.length; i++) {
      const raw = String(list[i] || "").trim().toLowerCase();
      const n = raw.replace(/^\*\./, "");
      if (!n) continue;
      if (host === n || host.endsWith("." + n)) return true;
    }
    return false;
  }

  /// Chrome 早期 fileSize 常是 -1/0, 真实长度在 totalBytes. 只认 >0 的值.
  function itemBytes(item) {
    if (!item) return 0;
    const xs = [item.totalBytes, item.fileSize];
    for (let i = 0; i < xs.length; i++) {
      const v = Number(xs[i]);
      if (Number.isFinite(v) && v > 0) return v;
    }
    return 0;
  }

  /// 开关打开就接管所有下载. 内部协议 / HTML 导航 / 页面 blob 仍留给浏览器.
  function shouldTakeover(item) {
    const url = (item && (item.finalUrl || item.url)) || "";
    if (!url) return false;
    if (/^(chrome|chrome-extension|about|edge|devtools|javascript|mailto):/i.test(url)) {
      return false;
    }
    if (/^(blob|data|filesystem):/i.test(url)) return false;
    if (/^magnet:/i.test(url)) return true;
    const mime = ((item && item.mime) || "").split(";")[0].trim().toLowerCase();
    if (mime === "application/x-bittorrent" || /\.torrent(\?|#|$)/i.test(url)) return true;
    if (mime === "text/html") return false;
    return true;
  }

  function decodeDataUrl(url) {
    const i = url.indexOf(",");
    if (!/^data:/i.test(url) || i < 0) throw new Error(policyMsg("非法 data URL", "Invalid data URL"));
    const meta = url.slice(5, i);
    const data = url.slice(i + 1);
    const mime = (meta.split(";")[0] || "").trim();
    let b64;
    if (/;base64/i.test(meta)) {
      b64 = data;
    } else {
      const raw = decodeURIComponent(data);
      b64 = btoa(unescape(raw));
    }
    if (inlineTooLarge(Math.floor(b64.length * 3 / 4))) {
      throw new Error(policyMsg("data URL 过大", "Data URL is too large"));
    }
    return { mime, b64 };
  }

  function inlineTooLarge(n) {
    return Number.isFinite(n) && n > MAX_INLINE_BYTES;
  }

  const api = {
    isBlobLike, blobId, itemKey, shouldTakeover, decodeDataUrl, hostOf, hostDenied, itemBytes,
    inlineTooLarge, MAX_INLINE_BYTES,
  };
  g.ddPolicy = api;
  g.isBlobLike = isBlobLike;
  g.blobId = blobId;
  g.itemKey = itemKey;
  g.shouldTakeover = shouldTakeover;
  g.decodeDataUrl = decodeDataUrl;
  g.hostOf = hostOf;
  g.hostDenied = hostDenied;
  g.itemBytes = itemBytes;
  if (typeof module !== "undefined" && module.exports) module.exports = api;
})(typeof self !== "undefined" ? self : globalThis);
