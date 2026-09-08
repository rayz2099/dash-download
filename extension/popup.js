const NATIVE = "top.linran.dd";
const DEFAULTS = { enabled: true };
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

chrome.storage.local.get(DEFAULTS, (cfg) => {
  state.enabled = cfg.enabled;
  renderToggle();
});

$("toggle").addEventListener("click", () => {
  state.enabled = !state.enabled;
  chrome.storage.local.set({ enabled: state.enabled });
  renderToggle();
});

checkHealth();
