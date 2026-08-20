// Takeover: 拦截浏览器下载并转交 app 核心 (携带 Request Context, 见 CONTEXT.md)
const API = "http://127.0.0.1:41320";
const MIN_SIZE = 1024 * 1024; // 已知大小 < 1MB 不接管, 不值得走多连接

const DEFAULTS = { enabled: true, token: "" };

async function settings() {
  return new Promise((resolve) => chrome.storage.local.get(DEFAULTS, resolve));
}

async function api(path, opts, token) {
  const resp = await fetch(API + path, {
    ...opts,
    headers: {
      "x-dd-token": token,
      ...(opts && opts.body ? { "content-type": "application/json" } : {}),
    },
  });
  if (!resp.ok) throw new Error("HTTP " + resp.status);
  return resp.json().catch(() => null);
}

/// 组装 Request Context: cookies 从浏览器取, referer/UA 一并带走,
/// 否则站点的鉴权下载在 app 侧会 403
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

async function sendToApp(url, { referrer, filename } = {}) {
  const cfg = await settings();
  if (!cfg.token) throw new Error("未配置 token");
  const headers = await buildHeaders(url, referrer);
  return api("/api/tasks", {
    method: "POST",
    body: JSON.stringify({ url, name: filename, headers }),
  }, cfg.token);
}

function notify(title, message) {
  chrome.notifications.create({
    type: "basic",
    iconUrl: "icons/128.png",
    title,
    message,
  });
}

// ── 下载接管 ──
chrome.downloads.onCreated.addListener(async (item) => {
  const cfg = await settings();
  if (!cfg.enabled || !cfg.token) return;

  const url = item.finalUrl || item.url;
  if (!/^https?:\/\//i.test(url)) return;
  if (item.fileSize > 0 && item.fileSize < MIN_SIZE) return;
  if ((item.mime || "").startsWith("text/html")) return;

  try {
    await sendToApp(url, { referrer: item.referrer, filename: basename(item.filename) });
    // 先确认 app 已受理, 再取消浏览器下载, 失败则不打扰原下载
    await chrome.downloads.cancel(item.id);
    chrome.downloads.erase({ id: item.id });
    notify("Dash Download 已接管", basename(item.filename) || url);
  } catch (e) {
    console.warn("接管失败, 保留浏览器下载:", e);
  }
});

// ── 右键菜单 ──
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
    notify("已发送到 Dash Download", info.linkUrl);
  } catch (e) {
    notify("发送失败", String(e && e.message ? e.message : e));
  }
});
