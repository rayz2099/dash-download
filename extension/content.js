// isolated: 只转发 SW 指定的 blob 回读. 不听页面 CustomEvent 写任务, 那条路可被网页伪造.
// match_origin_as_fallback 让这段脚本进 origin=null 的沙箱 iframe.
const pendingSeen = [];

function postPort(msg) {
  if (!port) return false;
  try { port.postMessage(msg); return true; } catch (_) { return false; }
}

window.addEventListener("dd-blob-seen", (e) => {
  const id = e.detail && e.detail.id;
  if (!id) return;
  if (!postPort({ op: "blob-seen", id })) pendingSeen.push(id);
});

window.addEventListener("dd-read-ok", (e) => {
  const d = e.detail;
  if (!d) return;
  postPort({ op: "read-blob-ok", ...d });
});

let port = null;
function connect() {
  port = chrome.runtime.connect({ name: "dd-page" });
  while (pendingSeen.length) postPort({ op: "blob-seen", id: pendingSeen.shift() });
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
