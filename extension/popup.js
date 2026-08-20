const API = "http://127.0.0.1:41320";
const $ = (id) => document.getElementById(id);

const state = { enabled: true, token: "" };

function renderToggle() {
  $("toggle").classList.toggle("on", state.enabled);
}

async function checkHealth() {
  try {
    const resp = await fetch(API + "/api/ping");
    const info = await resp.json();
    $("dot").classList.add("on");
    $("status").textContent = "已连接 v" + info.version;
  } catch (_) {
    $("dot").classList.remove("on");
    $("status").textContent = "app 未运行";
  }
}

chrome.storage.local.get({ enabled: true, token: "" }, (cfg) => {
  state.enabled = cfg.enabled;
  state.token = cfg.token;
  $("token").value = cfg.token;
  renderToggle();
});

$("toggle").addEventListener("click", () => {
  state.enabled = !state.enabled;
  chrome.storage.local.set({ enabled: state.enabled });
  renderToggle();
});

$("save").addEventListener("click", () => {
  state.token = $("token").value.trim();
  chrome.storage.local.set({ token: state.token });
  $("save").textContent = "已保存";
  setTimeout(() => ($("save").textContent = "保存"), 1200);
});

checkHealth();
