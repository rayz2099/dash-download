// 纯策略, 给 SW importScripts 与 node:test 共用.
(function (g) {
  const MIN_BYTES = 1024 * 1024;

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

  /// 只接管值得交给引擎的文件: 内部协议 / HTML / 低于体积阈值 / 黑名单域名留给浏览器.
  function shouldTakeover(item, rules) {
    const url = (item && (item.finalUrl || item.url)) || "";
    if (!url) return false;
    if (/^(chrome|chrome-extension|about|edge|devtools|javascript|mailto):/i.test(url)) {
      return false;
    }
    const mime = ((item && item.mime) || "").split(";")[0].trim().toLowerCase();
    if (mime === "text/html") return false;
    const minBytes = rules && Number.isFinite(Number(rules.minBytes))
      ? Number(rules.minBytes)
      : MIN_BYTES;
    const size = itemBytes(item);
    if (minBytes > 0 && size > 0 && size < minBytes) return false;
    if (hostDenied(hostOf(url), rules && rules.denyHosts)) return false;
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

  const api = {
    isBlobLike, blobId, itemKey, shouldTakeover, decodeDataUrl, hostOf, hostDenied, itemBytes, MIN_BYTES,
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
  g.MIN_BYTES = MIN_BYTES;
  if (typeof module !== "undefined" && module.exports) module.exports = api;
})(typeof self !== "undefined" ? self : globalThis);
