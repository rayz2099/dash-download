// MAIN world: blob 只属于创建它的 agent (含 origin=null 的沙箱 iframe).
// Chrome downloads 项常把地址改写成 blob:null/<uuid>, 所以按 uuid 缓存 Blob 本体.
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
    if (obj instanceof Blob) blobs.set(blobId(url), obj);
    return url;
  };

  const origRevoke = URL.revokeObjectURL;
  URL.revokeObjectURL = function (u) {
    // 页面常在 a.click() 后立刻 revoke, 延迟让我们读完再真正释放
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

  function takeoverOn() {
    return document.documentElement.getAttribute("data-dd-takeover") === "1";
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

  function emit(detail) {
    window.dispatchEvent(new CustomEvent("dd-catch", { detail }));
  }

  function grabBlob(url, name, onFail) {
    if (!takeoverOn()) return false;
    if (!/^(blob|data|filesystem):/i.test(url || "")) return false;
    readUrl(url)
      .then((got) => emit({ url, name: name || "", mime: got.mime, b64: got.b64 }))
      .catch(onFail);
    return true;
  }

  const origClick = HTMLAnchorElement.prototype.click;
  HTMLAnchorElement.prototype.click = function () {
    const a = this;
    const url = this.href;
    const name = this.getAttribute("download") || "";
    if (!grabBlob(url, name, () => origClick.call(a))) {
      return origClick.call(this);
    }
  };

  function anchorFrom(e) {
    const path = e.composedPath ? e.composedPath() : [];
    for (const n of path) {
      if (n && n.tagName === "A") return n;
    }
    return e.target && e.target.closest && e.target.closest("a");
  }

  document.addEventListener("click", (e) => {
    const a = anchorFrom(e);
    if (!a) return;
    const url = a.href;
    const name = a.getAttribute("download") || "";
    if (!grabBlob(url, name, () => origClick.call(a))) return;
    e.preventDefault();
    e.stopImmediatePropagation();
  }, true);

  window.addEventListener("dd-read", (e) => {
    const url = e.detail && e.detail.url;
    const req = e.detail && e.detail.req;
    if (!url) return;
    readUrl(url)
      .then((got) => window.dispatchEvent(new CustomEvent("dd-read-ok", { detail: { req, url, ...got } })))
      .catch((err) => window.dispatchEvent(new CustomEvent("dd-read-ok", { detail: { req, url, error: String(err) } })));
  });
})();
