// 纯策略, 给 SW importScripts 与 node:test 共用.
(function (g) {
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

  /// 接管一切浏览器下载; 只挡 chrome/about 这类没有文件体的内部页.
  function shouldTakeover(item) {
    const url = (item && (item.finalUrl || item.url)) || "";
    if (!url) return false;
    if (/^(chrome|chrome-extension|about|edge|devtools|javascript|mailto):/i.test(url)) {
      return false;
    }
    return true;
  }

  function decodeDataUrl(url) {
    const i = url.indexOf(",");
    if (!/^data:/i.test(url) || i < 0) throw new Error("非法 data URL");
    const meta = url.slice(5, i);
    const data = url.slice(i + 1);
    const mime = (meta.split(";")[0] || "").trim();
    if (/;base64/i.test(meta)) return { mime, b64: data };
    const raw = decodeURIComponent(data);
    return { mime, b64: btoa(unescape(raw)) };
  }

  const api = { isBlobLike, blobId, itemKey, shouldTakeover, decodeDataUrl };
  g.ddPolicy = api;
  g.isBlobLike = isBlobLike;
  g.blobId = blobId;
  g.itemKey = itemKey;
  g.shouldTakeover = shouldTakeover;
  g.decodeDataUrl = decodeDataUrl;
  if (typeof module !== "undefined" && module.exports) module.exports = api;
})(typeof self !== "undefined" ? self : globalThis);
