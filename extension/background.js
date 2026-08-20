// Takeover: 拦截必须先于 Chrome 画出下载气泡; 先 cancel/erase 再交给 app.
const API = "http://127.0.0.1:41320";
const MIN_SIZE = 1024 * 1024;

const DEFAULTS = { enabled: true };
let cached = { ...DEFAULTS };
const inflight = new Set();

chrome.storage.local.get(DEFAULTS, (cfg) => {
  cached = { enabled: cfg.enabled };
  applyDownloadUi();
});
chrome.storage.onChanged.addListener((changes, area) => {
  if (area !== "local") return;
  if (changes.enabled) cached.enabled = changes.enabled.newValue;
  applyDownloadUi();
});

function applyDownloadUi() {
  // 关掉原生下载气泡/shelf, 否则 cancel 也来不及挡住 (需要 downloads.ui)
  const enabled = !cached.enabled;
  const opts = { enabled };
  if (chrome.downloads.setUiOptions) {
    chrome.downloads.setUiOptions(opts).catch(() => {});
  } else if (chrome.downloads.setShelfEnabled) {
    chrome.downloads.setShelfEnabled(enabled);
  }
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
  } catch (_) { /* 无权限时降级为裸请求 */ }
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

function shouldTakeover(item) {
  const url = item.finalUrl || item.url || "";
  if (!/^https?:\/\//i.test(url)) return false;
  if (item.fileSize > 0 && item.fileSize < MIN_SIZE) return false;
  if ((item.mime || "").startsWith("text/html")) return false;
  return true;
}

function abortChrome(id) {
  try { chrome.downloads.cancel(id); } catch (_) { /* 可能已取消 */ }
  try { chrome.downloads.erase({ id }); } catch (_) { /* ignore */ }
}

async function sendToApp(url, extra) {
  const headers = await buildHeaders(url, extra && extra.referrer);
  const task = await api("/api/tasks", {
    method: "POST",
    body: JSON.stringify({
      url,
      name: extra && extra.filename,
      headers,
    }),
  });
  // 拉起主窗口, 失败不阻断接管
  api("/api/focus", { method: "POST" }).catch(() => {});
  return task;
}

function takeover(item) {
  const url = item.finalUrl || item.url;
  if (!cached.enabled) return;
  if (!shouldTakeover(item)) return;
  if (inflight.has(item.id) || inflight.has(url)) return;
  inflight.add(item.id);
  inflight.add(url);
  setTimeout(() => { inflight.delete(item.id); inflight.delete(url); }, 8000);

  abortChrome(item.id);
  sendToApp(url, { referrer: item.referrer, filename: basename(item.filename) })
    .catch((e) => console.warn("接管失败:", e));
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
    title: "使用 Dash Download 下载",
    contexts: ["link"],
  });
});

chrome.contextMenus.onClicked.addListener(async (info, tab) => {
  if (info.menuItemId !== "dd-download-link" || !info.linkUrl) return;
  try {
    await sendToApp(info.linkUrl, { referrer: tab && tab.url });
  } catch (e) {
    chrome.notifications.create({
      type: "basic",
      iconUrl: "icons/128.png",
      title: "发送失败",
      message: String(e && e.message ? e.message : e),
    });
  }
});
