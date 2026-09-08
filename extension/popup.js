const NATIVE = "top.linran.dd";
const DEFAULTS = { enabled: true, minBytes: 1024 * 1024, denyHosts: [] };
const SITE_ORIGINS = ["http://*/*", "https://*/*"];
const $ = (id) => document.getElementById(id);
const msg = (key, subs) => chrome.i18n.getMessage(key, subs);

document.documentElement.lang = chrome.i18n.getUILanguage().toLowerCase().startsWith("zh") ? "zh-CN" : "en";
document.querySelectorAll("[data-i18n]").forEach((el) => {
  el.textContent = msg(el.dataset.i18n);
});

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
  const info = await chrome.runtime.sendNativeMessage(NATIVE, { op: "ping" });
  if (!info || info.ok === false || !info.version) throw new Error(msg("ping_failed"));
  $("dot").classList.add("on");
  $("status").textContent = msg("connected", info.version);
  return true;
}

async function checkHealth() {
  try {
    await ping();
    return;
  } catch (_) { /* 尝试拉起 */ }
  $("status").textContent = msg("starting_app");
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
  $("status").textContent = msg("app_not_running");
}

chrome.storage.local.get(DEFAULTS, async (cfg) => {
  state.enabled = cfg.enabled;
  state.minBytes = cfg.minBytes;
  state.denyHosts = cfg.denyHosts;
  if (state.enabled) {
    const has = await chrome.permissions.contains({ origins: SITE_ORIGINS });
    if (!has) {
      const ok = await chrome.permissions.request({ origins: SITE_ORIGINS });
      if (!ok) {
        state.enabled = false;
        chrome.storage.local.set({ enabled: false });
      }
    }
  }
  renderToggle();
  renderRules();
});

$("toggle").addEventListener("click", async () => {
  if (!state.enabled) {
    const ok = await chrome.permissions.request({ origins: SITE_ORIGINS });
    if (!ok) return;
    state.enabled = true;
  } else {
    state.enabled = false;
    await chrome.permissions.remove({ origins: SITE_ORIGINS });
  }
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
