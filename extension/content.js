// isolated: MAIN 捕获的字节转到 SW. dataset 跨 world 共享, 用来开关接管.
// match_origin_as_fallback 让这段脚本进 origin=null 的沙箱 iframe.
function setFlag(on) {
  document.documentElement.setAttribute("data-dd-takeover", on ? "1" : "0");
}
setFlag(true);
chrome.storage.local.get({ enabled: true }, (cfg) => setFlag(!!cfg.enabled));
chrome.storage.onChanged.addListener((ch, area) => {
  if (area === "local" && ch.enabled) setFlag(!!ch.enabled.newValue);
});

window.addEventListener("dd-catch", (e) => {
  const d = e.detail;
  if (!d) return;
  chrome.runtime.sendMessage({ op: "captured", referrer: location.href, ...d });
});

window.addEventListener("dd-read-ok", (e) => {
  const d = e.detail;
  if (!d || !port) return;
  try { port.postMessage({ op: "read-blob-ok", ...d }); } catch (_) { /* SW 已断开 */ }
});

let port = null;
function connect() {
  port = chrome.runtime.connect({ name: "dd-page" });
  port.onMessage.addListener((msg) => {
    if (msg.op !== "read-blob") return;
    window.dispatchEvent(new CustomEvent("dd-read", { detail: { url: msg.url, req: msg.req } }));
  });
  port.onDisconnect.addListener(() => {
    port = null;
    setTimeout(connect, 500);
  });
}
connect();

chrome.runtime.onMessage.addListener((msg, _s, sendResponse) => {
  if (msg.op !== "read-blob") return;
  const req = msg.req || (String(Date.now()) + Math.random());
  const timer = setTimeout(() => sendResponse({ ok: false, error: "timeout" }), 4000);
  const onOk = (e) => {
    const d = e.detail;
    if (!d || d.req !== req) return;
    window.removeEventListener("dd-read-ok", onOk);
    clearTimeout(timer);
    if (d.error) sendResponse({ ok: false, error: d.error });
    else sendResponse({ ok: true, mime: d.mime, b64: d.b64 });
  };
  window.addEventListener("dd-read-ok", onOk);
  window.dispatchEvent(new CustomEvent("dd-read", { detail: { url: msg.url, req } }));
  return true;
});
