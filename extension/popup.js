const API = "http://127.0.0.1:41320";
const NATIVE = "dev.ray.dash_download";
const DEFAULTS = { enabled: true, minBytes: 1024 * 1024, denyHosts: [] };
const $ = (id) => document.getElementById(id);

const state = { ...DEFAULTS };

function renderToggle() {
  $("toggle").classList.toggle("on", state.enabled);
}

function renderRules() {
  $("minMb").value = String(state.minBytes / (1024 * 1024));
  $("deny").value = (state.denyHosts || []).join("\n");
}

function parseDeny(text) {
  return text.split("\n").map((s) => s.trim()).filter(Boolean);
}

async function ping() {
  const resp = await fetch(API + "/api/ping");
  const info = await resp.json();
  $("dot").classList.add("on");
  $("status").textContent = "已连接 v" + info.version;
  return true;
}

async function checkHealth() {
  try {
    await ping();
    return;
  } catch (_) { /* 尝试拉起 */ }
  $("status").textContent = "正在拉起 app…";
  try {
    await chrome.runtime.sendNativeMessage(NATIVE, { op: "wake" });
    for (let i = 0; i < 40; i++) {
      await new Promise((r) => setTimeout(r, 250));
      try {
        await ping();
        return;
      } catch (_) { /* 还在启动 */ }
    }
  } catch (_) { /* native host 未注册 */ }
  $("dot").classList.remove("on");
  $("status").textContent = "app 未运行";
}

chrome.storage.local.get(DEFAULTS, (cfg) => {
  state.enabled = cfg.enabled;
  state.minBytes = cfg.minBytes;
  state.denyHosts = cfg.denyHosts;
  renderToggle();
  renderRules();
});

$("toggle").addEventListener("click", () => {
  state.enabled = !state.enabled;
  chrome.storage.local.set({ enabled: state.enabled });
  renderToggle();
});

$("minMb").addEventListener("change", () => {
  const n = Number($("minMb").value);
  if (!Number.isFinite(n) || n < 0) {
    renderRules();
    return;
  }
  state.minBytes = Math.round(n * 1024 * 1024);
  chrome.storage.local.set({ minBytes: state.minBytes });
});

$("deny").addEventListener("change", () => {
  state.denyHosts = parseDeny($("deny").value);
  chrome.storage.local.set({ denyHosts: state.denyHosts });
});

checkHealth();
