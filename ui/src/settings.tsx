import { useEffect, useRef, useState } from "preact/hooks";
import * as api from "./api";
import type { Boot, EngineSettings, ProxyCfg, ProxyKind, ProxyProbe, UpdateStatus } from "./api";
import { MAX_CONN } from "./api";
import { fmtBytes } from "./util";
import { DirPick, NumStep } from "./fields";
import { t } from "./i18n";
import { applyCheck, armFlash } from "./update-check";

/** 探测默认打 Google, 因为多数代理场景就是为了出网. */
const DEF_PROBE_URL = "https://www.google.com";
const DEF_HOST = "127.0.0.1";

function isManual(kind: ProxyKind): boolean {
  return kind === "http" || kind === "socks5";
}

function defPort(kind: ProxyKind): number {
  if (kind === "socks5") return 1080;
  if (kind === "http") return 8080;
  return 0;
}

/** 空主机/端口按输入框占位值提交, 避免看见 127.0.0.1 实际却校验失败. */
function fillProxy(p: ProxyCfg): ProxyCfg {
  if (!isManual(p.kind)) return p;
  return {
    ...p,
    host: p.host.trim() || DEF_HOST,
    port: p.port || defPort(p.kind),
  };
}

/** 去掉 JS Error / reqwest http: 前缀，状态行只保留简短结论。 */
function errText(e: unknown): string {
  return String(e).replace(/^Error: /, "").replace(/^http: /, "");
}

/** 状态跟日志拆开, 避免成功/失败跟保存 flash 混在一块. */
function ProbeBox(props: { ok: ProxyProbe | null; err: string }) {
  if (!props.ok && !props.err) return null;
  const fail = !!props.err;
  const detail = fail
    ? props.err
    : `${props.ok!.status}  ${props.ok!.ms}ms\n${props.ok!.final_url}`;
  return (
    <div class={"probe-box" + (fail ? " err" : "")}>
      <div class="probe-status">{fail ? t("失败", "Failed") : t("成功", "Success")}</div>
      <pre class="probe-out">{detail}</pre>
    </div>
  );
}

export function updatePhaseText(st: UpdateStatus): string {
  switch (st.phase) {
    case "checking": return t("正在检查更新…", "Checking for updates…");
    case "up_to_date": return t("已是最新版本", "Up to date");
    case "available": return t(`发现 v${st.latest}`, `v${st.latest} available`);
    case "downloading": {
      const tot = st.total ? ` ${fmtBytes(st.done)} / ${fmtBytes(st.total)}` : "";
      return t(`正在下载 v${st.latest}${tot}`, `Downloading v${st.latest}${tot}`);
    }
    case "waiting": return t("等待当前下载结束后安装并重启", "Waiting for downloads to finish");
    case "installing": return t("正在安装，即将重启", "Installing, restarting soon");
    case "error": return st.error || t("检查更新失败", "Update check failed");
    default: return t("启动后自动检查 GitHub Release", "Checks GitHub Releases after launch");
  }
}

/** 空闲态不重复解释开关，只在状态变化时给反馈。 */
function upHint(st: UpdateStatus): string {
  if (st.phase === "error" || st.error) return t("检查失败，详见下方日志", "Check failed; see log below");
  if (st.phase === "idle") return "";
  return updatePhaseText(st);
}

/** 拼完整失败现场, 因为 hint 不可选中, 需要一块能直接复制的原文.
 * endpoint 必须是 GitHub API: 1.2.1+ 不再读 latest.json, 日志里写旧 URL 会误导排查. */
function upLogText(st: UpdateStatus, ver: string): string {
  return [
    `phase: ${st.phase}`,
    `error: ${st.error}`,
    `version: ${ver}`,
    `latest: ${st.latest}`,
    "endpoint: https://api.github.com/repos/rayz2099/dash-download/releases/latest",
  ].join("\n");
}

/** 失败日志单独成块, 才能用等宽 + user-select 把原文拷走. */
function UpdateLog(props: { up: UpdateStatus; ver: string }) {
  const txt = upLogText(props.up, props.ver);
  return (
    <div class="update-log">
      <div class="update-log-head">
        <span>{t("日志", "Log")}</span>
        <button class="btn" type="button"
          onClick={() => { void navigator.clipboard.writeText(txt); }}>{t("复制", "Copy")}</button>
      </div>
      <pre class="update-log-body">{txt}</pre>
    </div>
  );
}

type Tab = "general" | "p2p" | "proxy" | "update";

/** 设置页做成右侧内嵌导航, 避免跟任务表抢同一套 toolbar. */
export function SettingsPage(props: {
  boot: Boot;
  eng: EngineSettings;
  onEng: (s: EngineSettings) => void;
}) {
  const [tab, setTab] = useState<Tab>("general");
  const [eng, setEng] = useState(props.eng);
  const [up, setUp] = useState<UpdateStatus | null>(null);
  const [autoStart, setAutoStart] = useState(true);
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState("");
  const [flash, setFlash] = useState("");
  const [probeUrl, setProbeUrl] = useState(DEF_PROBE_URL);
  const [probing, setProbing] = useState(false);
  const [probeOk, setProbeOk] = useState<ProxyProbe | null>(null);
  const [probeErr, setProbeErr] = useState("");
  const probeSeq = useRef(0);
  const flashTimer = useRef<ReturnType<typeof window.setTimeout> | null>(null);
  const upText = up ? upHint(up) : t("读取中…", "Loading…");

  useEffect(() => { setEng(props.eng); }, [props.eng]);

  useEffect(() => () => {
    if (flashTimer.current) clearTimeout(flashTimer.current);
  }, []);

  useEffect(() => {
    let on = true;
    const tick = () => {
      api.updateStatus().then((s) => on && setUp(s));
      api.autoStartOn().then((v) => on && setAutoStart(v));
    };
    tick();
    const id = setInterval(tick, 1500);
    return () => { on = false; clearInterval(id); };
  }, []);

  const persist = async (next: EngineSettings) => {
    const body = { ...next, proxy: fillProxy(next.proxy) };
    setEng(body);
    setErr("");
    try {
      const s = await api.putSettings(body);
      setEng(s);
      props.onEng(s);
    } catch (e) {
      setErr(errText(e));
    }
  };

  /** 切类型后丢弃进行中的探测, 避免把 HTTP 的结果贴到直连上. */
  const clearProbe = () => {
    probeSeq.current += 1;
    setProbing(false);
    setProbeOk(null);
    setProbeErr("");
  };

  /** 跟通用页一样点选即写 prefs. 探测结果不跨 kind. */
  const setKind = (kind: ProxyKind) => {
    if (kind === eng.proxy.kind) return;
    clearProbe();
    const proxy = fillProxy({ ...eng.proxy, kind });
    persist({ ...eng, proxy });
  };

  const saveProxy = (proxy: ProxyCfg) => persist({ ...eng, proxy: fillProxy(proxy) });

  const showFlash = (msg: string) => {
    armFlash(
      setFlash,
      {
        setTimeout: (fn, ms) => window.setTimeout(fn, ms),
        clearTimeout: (id: ReturnType<typeof window.setTimeout>) => window.clearTimeout(id),
      },
      flashTimer,
      msg,
    );
  };

  /** 手动检查只探测; 有新版本先问, 确认后才走 UpdateBar 同一套 install. */
  const onCheck = async () => {
    setBusy(true);
    setFlash("");
    try {
      const r = await applyCheck({
        check: api.checkUpdate,
        install: api.checkNow,
        ask: (p) => confirm(p),
      });
      setUp(r.status);
      if (r.flash) showFlash(r.flash);
    } catch {
      try {
        setUp(await api.updateStatus());
      } catch {
        /* 失败态由 phase=error 的日志块展示 */
      }
    } finally {
      setBusy(false);
    }
  };

  /** 走草稿 proxy, 点测试不等于点应用. */
  const runProbe = async () => {
    const url = probeUrl.trim();
    const seq = ++probeSeq.current;
    setProbeOk(null);
    if (!url) {
      setProbeErr(t("URL 不能为空", "URL is required"));
      return;
    }
    setProbeErr("");
    setProbing(true);
    try {
      const r = await api.testProxy(url, fillProxy(eng.proxy));
      if (seq !== probeSeq.current) return;
      setProbeOk(r);
    } catch (e) {
      if (seq !== probeSeq.current) return;
      setProbeErr(errText(e));
    } finally {
      if (seq === probeSeq.current) setProbing(false);
    }
  };

  return (
    <div class="settings-shell">
      <div class="settings-rail">
        <div class="settings-rail-title">{t("设置", "Settings")}</div>
        {([
          ["general", t("通用", "General")],
          ["p2p", "P2P"],
          ["proxy", t("代理", "Proxy")],
          ["update", t("更新", "Update")],
        ] as [Tab, string][]).map(([k, label]) => (
          <div class={"settings-rail-item" + (tab === k ? " on" : "")} onClick={() => setTab(k)}>
            {label}
          </div>
        ))}
      </div>
      <div class="settings-main">
        <div class="page-inner">
          {tab === "general" && (
            <>
              <h1 class="page-title">{t("通用", "General")}</h1>
              <div class="settings-card">
                <div class="settings-row">
                  <div>
                    <div class="settings-label">{t("默认下载目录", "Default download folder")}</div>
                  </div>
                </div>
                <DirPick value={eng.default_dir} onChange={(dir) => persist({ ...props.eng, default_dir: dir })} />
                <div class="settings-row">
                  <div>
                    <div class="settings-label">{t("同时下载", "Concurrent downloads")}</div>
                  </div>
                  <NumStep value={eng.max_concurrent} min={1} max={MAX_CONN}
                    onChange={(n) => persist({ ...props.eng, max_concurrent: n })} />
                </div>
                <div class="settings-row">
                  <div>
                    <div class="settings-label">{t("每任务连接数", "Connections per task")}</div>
                    <div class="settings-hint">{t("仅影响新任务", "New tasks only")}</div>
                  </div>
                  <NumStep value={eng.max_segments} min={1} max={MAX_CONN}
                    onChange={(n) => persist({ ...props.eng, max_segments: n })} />
                </div>
                <div class="settings-row">
                  <div>
                    <div class="settings-label">{t("开机自启", "Launch at login")}</div>
                  </div>
                  <button class={"toggle" + (autoStart ? " on" : "")}
                    onClick={() => {
                      const next = !autoStart;
                      api.setAutoStart(next).then(setAutoStart);
                    }} />
                </div>
              </div>
            </>
          )}

          {tab === "p2p" && (
            <>
              <h1 class="page-title">{t("P2P 网络", "P2P Network")}</h1>
              <div class="settings-card">
                <div class="settings-row">
                  <div>
                    <div class="settings-label">{t("启用 P2P", "Enable P2P")}</div>
                    <div class="settings-hint">{t("关闭时不监听端口", "No listening port when disabled")}</div>
                  </div>
                  <button class={"toggle" + (eng.p2p ? " on" : "")}
                    onClick={() => persist({ ...props.eng, p2p: !eng.p2p })} />
                </div>
                <div class="settings-row">
                  <div>
                    <div class="settings-label">{t("同时下载", "Concurrent downloads")}</div>
                  </div>
                  <NumStep value={eng.max_bt_active || 3} min={1} max={MAX_CONN}
                    onChange={(n) => persist({ ...props.eng, max_bt_active: n })} />
                </div>
                <div class="settings-row">
                  <div>
                    <div class="settings-label">{t("做种上限", "Seeding limit")}</div>
                  </div>
                  <NumStep value={eng.max_bt_seed || 10} min={1} max={MAX_CONN}
                    onChange={(n) => persist({ ...props.eng, max_bt_seed: n })} />
                </div>
                <div class="settings-row">
                  <div>
                    <div class="settings-label">{t("入站端口", "Incoming port")}</div>
                    <div class="settings-hint">{t("修改后重启", "Restart to apply")}</div>
                  </div>
                  <NumStep value={eng.listen_port || 0} min={0} max={65535}
                    onChange={(n) => persist({ ...props.eng, listen_port: n })} />
                </div>
                <label class="settings-row">
                  <div>
                    <div class="settings-label">UPnP</div>
                    <div class="settings-hint">{t("自动映射入站端口", "Map the incoming port automatically")}</div>
                  </div>
                  <input type="checkbox" checked={eng.upnp !== false}
                    onChange={(e) => persist({ ...props.eng, upnp: (e.target as HTMLInputElement).checked })} />
                </label>
                <label class="settings-row">
                  <div>
                    <div class="settings-label">{t("附加公共 Tracker", "Add public trackers")}</div>
                    <div class="settings-hint">{t("Private Torrent 不附加", "Not added to private torrents")}</div>
                  </div>
                  <input type="checkbox" checked={!!eng.extra_trackers}
                    onChange={(e) => persist({ ...props.eng, extra_trackers: (e.target as HTMLInputElement).checked })} />
                </label>
                <div class="settings-row">
                  <div>
                    <div class="settings-label">{t("磁力解析超时", "Magnet timeout")}</div>
                  </div>
                  <NumStep value={eng.resolve_secs || 30} min={5} max={300}
                    onChange={(n) => persist({ ...props.eng, resolve_secs: n })} />
                </div>
                {eng.bt_direct && (
                  <div class="settings-hint">{t("HTTP 代理下 BT 直连", "BT uses direct connections with an HTTP proxy")}</div>
                )}
              </div>
            </>
          )}

          {tab === "proxy" && (
            <>
              <h1 class="page-title">{t("代理", "Proxy")}</h1>
              <div class="settings-card">
                <div class="settings-label" style={{ marginBottom: 10 }}>{t("类型", "Type")}</div>
                <div class="radio-list">
                  {([
                    ["direct", t("直连", "Direct")],
                    ["no_proxy", t("无代理", "System proxy")],
                    ["http", "HTTP"],
                    ["socks5", "SOCKS5"],
                  ] as [ProxyKind, string][]).map(([k, label]) => (
                    <label class="radio-row" key={k}>
                      <input type="radio" name="proxy-kind" checked={eng.proxy.kind === k}
                        onChange={() => setKind(k)} />
                      <span>{label}</span>
                    </label>
                  ))}
                </div>
                {isManual(eng.proxy.kind) && (
                  <>
                    <div class="proxy-grid">
                      <div class="field">
                        <label>{t("主机", "Host")}</label>
                        <input type="text" value={eng.proxy.host} placeholder={DEF_HOST}
                          autoCapitalize="none" autoCorrect="off" autoComplete="off" spellcheck={false}
                          onInput={(e) => setEng({
                            ...eng,
                            proxy: { ...eng.proxy, host: (e.target as HTMLInputElement).value },
                          })}
                          onBlur={(e) => saveProxy({
                            ...eng.proxy, host: (e.target as HTMLInputElement).value,
                          })} />
                      </div>
                      <div class="field">
                        <label>{t("端口", "Port")}</label>
                        <input type="text" value={eng.proxy.port || ""} placeholder={eng.proxy.kind === "socks5" ? "1080" : "8080"}
                          onInput={(e) => {
                            const v = (e.target as HTMLInputElement).value.replace(/\D/g, "");
                            setEng({ ...eng, proxy: { ...eng.proxy, port: v ? Number(v) : 0 } });
                          }}
                          onBlur={(e) => {
                            const v = (e.target as HTMLInputElement).value.replace(/\D/g, "");
                            saveProxy({ ...eng.proxy, port: v ? Number(v) : 0 });
                          }} />
                      </div>
                    </div>
                    <div class="settings-row">
                      <div>
                        <div class="settings-label">{t("代理认证", "Authentication")}</div>
                      </div>
                      <button class={"toggle" + (eng.proxy.auth ? " on" : "")}
                        onClick={() => saveProxy({ ...eng.proxy, auth: !eng.proxy.auth })} />
                    </div>
                    {eng.proxy.auth && (
                      <div class="proxy-grid">
                        <div class="field">
                          <label>{t("用户名", "Username")}</label>
                          <input type="text" value={eng.proxy.user}
                            onInput={(e) => setEng({
                              ...eng, proxy: { ...eng.proxy, user: (e.target as HTMLInputElement).value },
                            })}
                            onBlur={(e) => saveProxy({
                              ...eng.proxy, user: (e.target as HTMLInputElement).value,
                            })} />
                        </div>
                        <div class="field">
                          <label>{t("密码", "Password")}</label>
                          <input type="password" value={eng.proxy.pass}
                            placeholder={eng.proxy.pass_set ? t("已保存", "Saved") : ""}
                            onInput={(e) => setEng({
                              ...eng, proxy: { ...eng.proxy, pass: (e.target as HTMLInputElement).value },
                            })}
                            onBlur={(e) => saveProxy({
                              ...eng.proxy, pass: (e.target as HTMLInputElement).value,
                            })} />
                        </div>
                      </div>
                    )}
                  </>
                )}
                <div class="settings-row">
                  <div>
                    <div class="settings-label">{t("测试代理", "Test proxy")}</div>
                  </div>
                </div>
                <div class="proxy-test-row">
                  <div class="field">
                    <label>URL</label>
                    <input type="text" value={probeUrl} placeholder={DEF_PROBE_URL}
                      onInput={(e) => setProbeUrl((e.target as HTMLInputElement).value)} />
                  </div>
                  <button class="btn" type="button" disabled={probing}
                    onClick={() => { void runProbe(); }}>
                    {probing ? t("测试中…", "Testing…") : t("测试", "Test")}
                  </button>
                </div>
                <ProbeBox ok={probeOk} err={probeErr} />
              </div>
            </>
          )}

          {tab === "update" && (
            <>
              <h1 class="page-title">{t("更新", "Update")}</h1>
              <div class="settings-card">
                <div class="settings-row">
                  <div>
                    <div class="settings-label">{t("自动检查并安装", "Check and install automatically")}</div>
                    {upText && <div class="settings-hint">{upText}</div>}
                  </div>
                  <button class={"toggle" + (up?.auto_update ? " on" : "")}
                    disabled={!up}
                    onClick={() => {
                      if (!up) return;
                      api.setAutoUpdate(!up.auto_update).then(setUp);
                    }} />
                </div>
                <div class="settings-row">
                  <div>
                    <div class="settings-label">{t("核心版本", "Core version")}</div>
                  </div>
                  <div class="settings-actions">
                    <span class="mono-chip">v{props.boot.version}</span>
                    <button class="btn"
                      disabled={busy || up?.phase === "checking" || up?.phase === "downloading"
                        || up?.phase === "waiting" || up?.phase === "installing"}
                      onClick={() => { void onCheck(); }}>
                      {busy ? t("检查中…", "Checking…") : t("检查更新", "Check for updates")}
                    </button>
                  </div>
                </div>
              </div>
              {flash && (
                <div class="settings-flash">{flash}</div>
              )}
              {up && (up.phase === "error" || up.error) && (
                <UpdateLog up={up} ver={props.boot.version} />
              )}
            </>
          )}

          {err && (
            <div class="settings-flash err">{err}</div>
          )}
        </div>
      </div>
    </div>
  );
}
