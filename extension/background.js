// Takeover: 先把任务交给 app, 成功后再 abort Chrome. 控制面只走 native messaging.
importScripts("policy.js");

const NATIVE = "top.linran.dd";
/// 只认总开关. 体积阈值和域名黑名单不再进产品面, 旧 storage 里的值忽略.
const DEFAULTS = { enabled: true };
const msg = (key) => chrome.i18n.getMessage(key);

let cached = { ...DEFAULTS };
const inflight = new Set();
const sent = new Set();

chrome.storage.local.get(DEFAULTS, (cfg) => {
  cached.enabled = cfg.enabled;
});
chrome.storage.onChanged.addListener((changes, area) => {
  if (area !== "local") return;
  if (changes.enabled) cached.enabled = changes.enabled.newValue;
});

function sleep(ms) {
  return new Promise((r) => setTimeout(r, ms));
}

function native(msg) {
  return chrome.runtime.sendNativeMessage(NATIVE, msg);
}

async function ping() {
  try {
    const info = await native({ op: "ping" });
    if (!info || info.ok === false || !info.version) return null;
    return info;
  } catch (_) {
    return null;
  }
}

/// app 没跑时 native host 会拉 GUI 再把 ping 转进 ipc; 拉不起就不要 abort Chrome 下载.
async function ensureApp() {
  let info = await ping();
  if (info) return info;
  for (let i = 0; i < 40; i++) {
    await sleep(250);
    info = await ping();
    if (info) return info;
  }
  return null;
}

function nativeErr(r) {
  if (!r) throw new Error(msg("native_no_reply"));
  if (r.ok === false) throw new Error(r.error || msg("native_failed"));
  return r;
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

async function sendTorrent(payload) {
  const t = nativeErr(await native({ op: "add_torrent", ...payload }));
  native({ op: "focus" }).catch(() => {});
  return t;
}

async function sendToApp(url, extra) {
  extra = extra || {};
  if (/^magnet:/i.test(url)) {
    return sendTorrent({ magnet: url });
  }
  const torrent = extra.torrent || /\.torrent(\?|#|$)/i.test(url)
    || (extra.mime || "").toLowerCase().indexOf("bittorrent") >= 0;
  if (torrent && !extra.contentB64) {
    const headers = await buildHeaders(url, extra.referrer);
    return sendTorrent({ torrent_url: url, headers });
  }
  const headers = extra.contentB64 ? [] : await buildHeaders(url, extra.referrer);
  const body = { url, name: extra.filename, headers };
  if (extra.contentB64) {
    if (inlineTooLarge(Math.floor(extra.contentB64.length * 3 / 4))) {
      throw new Error(msg("content_too_large"));
    }
    body.content_b64 = extra.contentB64;
    body.mime = extra.mime || "";
  }
  const task = nativeErr(await native({ op: "add_task", ...body }));
  native({ op: "focus" }).catch(() => {});
  return task;
}

async function captureUrl(url, extra) {
  extra = extra || {};
  if (/^data:/i.test(url) && !extra.contentB64) {
    const d = decodeDataUrl(url);
    return sendToApp(url, { filename: extra.filename, contentB64: d.b64, mime: d.mime });
  }
  if (isBlobLike(url) && !extra.contentB64) {
    throw new Error(msg("blob_unsupported"));
  }
  return sendToApp(url, extra);
}

function takeover(item) {
  const url = item.finalUrl || item.url;
  const key = itemKey(url);
  if (!cached.enabled) return;
  if (!shouldTakeover(item)) return;
  if (sent.has(key)) {
    abortChrome(item.id);
    return;
  }
  if (inflight.has(item.id) || inflight.has(key)) return;
  inflight.add(item.id);
  inflight.add(key);
  setTimeout(() => { inflight.delete(item.id); inflight.delete(key); }, 12000);

  ensureApp().then(async (info) => {
    if (!info) return;
    await captureUrl(url, {
      referrer: item.referrer,
      filename: basename(item.filename),
      mime: item.mime,
    });
    sent.add(key);
    abortChrome(item.id);
  }).catch((e) => {
    console.warn("接管失败:", e);
    notify(msg("takeover_failed"), e && e.message ? e.message : e);
  });
}

chrome.downloads.onCreated.addListener((item) => takeover(item));

chrome.downloads.onDeterminingFilename.addListener((item, suggest) => {
  // suggest 必须同步调用, 否则 Chrome 会卡住下载对话框
  suggest();
  takeover(item);
});

chrome.runtime.onInstalled.addListener(() => {
  chrome.contextMenus.create({
    id: "dd-download-link",
    title: msg("context_download"),
    contexts: ["link"],
  });
});

chrome.contextMenus.onClicked.addListener(async (info, tab) => {
  if (info.menuItemId !== "dd-download-link" || !info.linkUrl) return;
  try {
    if (!(await ensureApp())) throw new Error(msg("app_start_failed"));
    await captureUrl(info.linkUrl, { referrer: tab && tab.url });
  } catch (e) {
    notify(msg("send_failed"), e && e.message ? e.message : e);
  }
});
