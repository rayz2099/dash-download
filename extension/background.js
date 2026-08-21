// Takeover: 先把任务交给 app, 成功后再 abort Chrome. blob:null 必须从页面读字节.
importScripts("policy.js");

const API = "http://127.0.0.1:41320";
const NATIVE = "dev.ray.dash_download";
const DEFAULTS = { enabled: true, minBytes: 1024 * 1024, denyHosts: [] };

let cached = { ...DEFAULTS };
const inflight = new Set();
const sent = new Set();
const pages = new Map(); // port -> { blobs: Set }

chrome.storage.local.get(DEFAULTS, (cfg) => {
  cached.enabled = cfg.enabled;
  cached.minBytes = cfg.minBytes;
  cached.denyHosts = cfg.denyHosts;
});
chrome.storage.onChanged.addListener((changes, area) => {
  if (area !== "local") return;
  if (changes.enabled) cached.enabled = changes.enabled.newValue;
  if (changes.minBytes) cached.minBytes = changes.minBytes.newValue;
  if (changes.denyHosts) cached.denyHosts = changes.denyHosts.newValue;
});

function sleep(ms) {
  return new Promise((r) => setTimeout(r, ms));
}

async function ping() {
  try {
    const resp = await fetch(API + "/api/ping");
    return resp.ok;
  } catch (_) {
    return false;
  }
}

async function registerOrigin() {
  try {
    await api("/api/ext-origin", {
      method: "POST",
      body: JSON.stringify({ origin: "chrome-extension://" + chrome.runtime.id + "/" }),
    });
  } catch (e) { console.warn("登记 origin 失败:", e); }
}

/// app 没跑时走 native host 拉起; 拉不起就不要 abort Chrome 下载.
async function ensureApp() {
  if (await ping()) {
    registerOrigin();
    return true;
  }
  try {
    await chrome.runtime.sendNativeMessage(NATIVE, { op: "wake" });
  } catch (_) {
    return false;
  }
  for (let i = 0; i < 40; i++) {
    await sleep(250);
    if (await ping()) {
      registerOrigin();
      return true;
    }
  }
  return false;
}

async function api(path, opts) {
  const resp = await fetch(API + path, {
    ...opts,
    headers: {
      "x-dd-client": "ext",
      ...(opts && opts.body ? { "content-type": "application/json" } : {}),
    },
  });
  if (!resp.ok) throw new Error("HTTP " + resp.status);
  return resp.json().catch(() => null);
}

async function buildHeaders(url, referrer) {
  const headers = [];
  try {
    const cookies = await chrome.cookies.getAll({ url });
    if (cookies.length) {
      headers.push(["Cookie", cookies.map((c) => `${c.name}=${c.value}`).join("; ")]);
    }
  } catch (_) { /* blob/data 没有 cookie 域 */ }
  if (referrer) headers.push(["Referer", referrer]);
  headers.push(["User-Agent", navigator.userAgent]);
  return headers;
}

function basename(path) {
  if (!path) return undefined;
  const s = path.replace(/\\/g, "/");
  const name = s.substring(s.lastIndexOf("/") + 1);
  return name || undefined;
}

function abortChrome(id) {
  try { chrome.downloads.cancel(id); } catch (_) { /* 可能已取消 */ }
  try { chrome.downloads.erase({ id }); } catch (_) { /* ignore */ }
}

function notify(title, message) {
  try {
    chrome.notifications.create({
      type: "basic",
      iconUrl: "icons/128.png",
      title,
      message: String(message || ""),
    });
  } catch (_) { /* 无通知权限时只打日志 */ }
}

async function sendToApp(url, extra) {
  extra = extra || {};
  const headers = extra.contentB64 ? [] : await buildHeaders(url, extra.referrer);
  const body = { url, name: extra.filename, headers };
  if (extra.contentB64) {
    body.content_b64 = extra.contentB64;
    body.mime = extra.mime || "";
  }
  const task = await api("/api/tasks", {
    method: "POST",
    body: JSON.stringify(body),
  });
  api("/api/focus", { method: "POST" }).catch(() => {});
  return task;
}

function readBlob(url) {
  return new Promise((resolve, reject) => {
    const id = blobId(url);
    const targets = [];
    for (const [port, st] of pages) {
      if (st.blobs.has(id)) targets.push(port);
    }
    // 只问声明持有该 uuid 的页面. 广播会被其它 Tab 伪造 dd-read-ok 抢答.
    if (targets.length === 0) {
      reject(new Error("没有页面持有该 blob"));
      return;
    }
    const req = String(Date.now()) + Math.random();
    let left = targets.length;
    let settled = false;
    const timer = setTimeout(() => {
      if (settled) return;
      settled = true;
      reject(new Error("无法读取 " + url));
    }, 4000);
    for (const port of targets) {
      const onMsg = (msg) => {
        if (msg.op !== "read-blob-ok" || msg.req !== req) return;
        port.onMessage.removeListener(onMsg);
        if (settled) return;
        if (msg.error || !msg.b64) {
          left -= 1;
          if (left <= 0) {
            settled = true;
            clearTimeout(timer);
            reject(new Error(msg.error || "blob 读取失败"));
          }
          return;
        }
        settled = true;
        clearTimeout(timer);
        resolve({ mime: msg.mime || "", b64: msg.b64 });
      };
      port.onMessage.addListener(onMsg);
      try {
        port.postMessage({ op: "read-blob", url, req });
      } catch (_) {
        left -= 1;
        port.onMessage.removeListener(onMsg);
      }
    }
    if (left <= 0 && !settled) {
      settled = true;
      clearTimeout(timer);
      reject(new Error("页面端口不可用"));
    }
  });
}

async function captureUrl(url, extra) {
  extra = extra || {};
  if (/^data:/i.test(url) && !extra.contentB64) {
    const d = decodeDataUrl(url);
    return sendToApp(url, { filename: extra.filename, contentB64: d.b64, mime: d.mime });
  }
  if (isBlobLike(url) && !extra.contentB64) {
    const got = await readBlob(url);
    return sendToApp(url, {
      filename: extra.filename,
      contentB64: got.b64,
      mime: extra.mime || got.mime,
    });
  }
  if (extra.contentB64) {
    return sendToApp(url, extra);
  }
  extra.referrer = extra.referrer;
  return sendToApp(url, extra);
}

function takeover(item) {
  const url = item.finalUrl || item.url;
  const key = itemKey(url);
  if (!cached.enabled) return;
  if (!shouldTakeover(item, cached)) return;
  if (sent.has(key)) {
    abortChrome(item.id);
    return;
  }
  if (inflight.has(item.id) || inflight.has(key)) return;
  inflight.add(item.id);
  inflight.add(key);
  setTimeout(() => { inflight.delete(item.id); inflight.delete(key); }, 12000);

  ensureApp().then(async (ok) => {
    if (!ok) return;
    await captureUrl(url, {
      referrer: item.referrer,
      filename: basename(item.filename),
    });
    sent.add(key);
    abortChrome(item.id);
  }).catch((e) => {
    console.warn("接管失败:", e);
    notify("接管失败", e && e.message ? e.message : e);
  });
}

chrome.runtime.onConnect.addListener((port) => {
  if (port.name !== "dd-page") return;
  const st = { blobs: new Set() };
  pages.set(port, st);
  port.onMessage.addListener((msg) => {
    if (msg && msg.op === "blob-seen" && msg.id) st.blobs.add(String(msg.id));
  });
  port.onDisconnect.addListener(() => pages.delete(port));
});

chrome.downloads.onCreated.addListener((item) => takeover(item));

chrome.downloads.onDeterminingFilename.addListener((item, suggest) => {
  // suggest 必须同步调用, 否则 Chrome 会卡住下载对话框
  suggest();
  takeover(item);
});

chrome.runtime.onInstalled.addListener(() => {
  chrome.contextMenus.create({
    id: "dd-download-link",
    title: "使用 Dash Download 下载",
    contexts: ["link"],
  });
});

chrome.contextMenus.onClicked.addListener(async (info, tab) => {
  if (info.menuItemId !== "dd-download-link" || !info.linkUrl) return;
  try {
    if (!(await ensureApp())) throw new Error("无法拉起 Dash Download");
    await captureUrl(info.linkUrl, { referrer: tab && tab.url });
  } catch (e) {
    notify("发送失败", e && e.message ? e.message : e);
  }
});
