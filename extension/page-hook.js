// MAIN world: blob 只属于创建它的 agent (含 origin=null 的沙箱 iframe).
// Chrome downloads 项常把地址改写成 blob:null/<uuid>, 所以按 uuid 缓存 Blob 本体.
// 不向页面暴露 "写下载目录" 入口: CustomEvent 可被网页伪造, 只在 SW 按 chrome.downloads 回读字节.
(() => {
  const blobs = new Map();

  function blobId(url) {
    const s = String(url || "");
    const i = s.lastIndexOf("/");
    return i >= 0 ? s.slice(i + 1) : s;
  }

  const origCreate = URL.createObjectURL;
  URL.createObjectURL = function (obj) {
    const url = origCreate.call(URL, obj);
    if (obj instanceof Blob) {
      const id = blobId(url);
      blobs.set(id, obj);
      window.dispatchEvent(new CustomEvent("dd-blob-seen", { detail: { id } }));
    }
    return url;
  };

  const origRevoke = URL.revokeObjectURL;
  URL.revokeObjectURL = function (u) {
    // 页面常在 a.click() 后立刻 revoke, 延迟让 SW 读完再真正释放
    const id = blobId(u);
    setTimeout(() => {
      origRevoke.call(URL, u);
      blobs.delete(id);
    }, 5000);
  };

  function b64(buf) {
    const u8 = new Uint8Array(buf);
    const step = 0x8000;
    let s = "";
    for (let i = 0; i < u8.length; i += step) {
      s += String.fromCharCode.apply(null, u8.subarray(i, i + step));
    }
    return btoa(s);
  }

  async function readUrl(url) {
    const id = blobId(url);
    const blob = blobs.get(id);
    if (blob) {
      const buf = await blob.arrayBuffer();
      return { mime: blob.type || "", b64: b64(buf) };
    }
    const r = await fetch(url);
    const buf = await r.arrayBuffer();
    return { mime: r.headers.get("content-type") || "", b64: b64(buf) };
  }

  window.addEventListener("dd-read", (e) => {
    const url = e.detail && e.detail.url;
    const req = e.detail && e.detail.req;
    if (!url) return;
    readUrl(url)
      .then((got) => window.dispatchEvent(new CustomEvent("dd-read-ok", { detail: { req, url, ...got } })))
      .catch((err) => window.dispatchEvent(new CustomEvent("dd-read-ok", { detail: { req, url, error: String(err) } })));
  });
})();
