import type { JSX } from "preact";
import { useEffect, useMemo, useState } from "preact/hooks";
import * as api from "./api";
import type { Boot, TaskInfo } from "./api";
import {
  fileType, fmtBytes, fmtEta, fmtSpeed, fmtTime, pct, STATE_META, TYPE_LABEL,
} from "./util";
import type { FileType } from "./util";
import {
  IcoAlert, IcoCheck, IcoClock, IcoDown, IcoGear, IcoMoon, IcoPause,
  IcoPlay, IcoPlus, IcoQueue, IcoSearch, IcoSun, IcoTrash, IcoX, TypeIcon,
} from "./icons";

type Filter = "all" | "active" | "queued" | "completed" | "failed" | `type:${FileType}`;
type SortKey = "name" | "size" | "state" | "speed" | "created" | "time";

const SIDE_STATES: { key: Filter; label: string; icon: (p: { size?: number }) => JSX.Element }[] = [
  { key: "all", label: "全部", icon: IcoQueue },
  { key: "active", label: "下载中", icon: IcoDown },
  { key: "queued", label: "等待中", icon: IcoClock },
  { key: "completed", label: "已完成", icon: IcoCheck },
  { key: "failed", label: "失败", icon: IcoAlert },
];

const FILE_TYPES: FileType[] = ["video", "doc", "archive", "app", "audio", "other"];

const lastTry = (t: TaskInfo) => t.completed_at ?? t.created_at;
const isRunning = (t: TaskInfo) => t.state === "active" || t.state === "probing";

function initTheme(): string {
  const saved = localStorage.getItem("dd_theme");
  if (saved) return saved;
  return matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
}

interface CtxMenu {
  x: number;
  y: number;
  id: number;
}

export function App() {
  const [boot, setBoot] = useState<Boot | null>(null);
  const [bootErr, setBootErr] = useState("");
  const [tasks, setTasks] = useState<TaskInfo[]>([]);
  const [filter, setFilter] = useState<Filter>("all");
  const [query, setQuery] = useState("");
  const [selectedId, setSelectedId] = useState<number | null>(null);
  const [progressId, setProgressId] = useState<number | null>(null);
  const [menu, setMenu] = useState<CtxMenu | null>(null);
  const [sort, setSort] = useState<{ key: SortKey; asc: boolean }>({ key: "time", asc: false });
  const [showNew, setShowNew] = useState(false);
  const [showSettings, setShowSettings] = useState(false);
  const [theme, setTheme] = useState(initTheme);

  useEffect(() => {
    document.documentElement.dataset.theme = theme;
    localStorage.setItem("dd_theme", theme);
  }, [theme]);

  useEffect(() => {
    let dispose: (() => void) | undefined;
    api.init()
      .then((b) => {
        setBoot(b);
        dispose = api.connectEvents((ev) => {
          switch (ev.type) {
            case "snapshot":
              setTasks(ev.tasks);
              break;
            case "task_added":
              setTasks((p) => [ev.task, ...p.filter((t) => t.id !== ev.task.id)]);
              setSelectedId(ev.task.id);
              setMenu(null);
              setProgressId(ev.task.id);
              break;
            case "task_updated":
              setTasks((p) => p.map((t) => (t.id === ev.task.id ? ev.task : t)));
              break;
            case "task_removed":
              setTasks((p) => p.filter((t) => t.id !== ev.id));
              break;
            case "progress":
              setTasks((p) =>
                p.map((t) => {
                  const pr = ev.tasks.find((x) => x.id === t.id);
                  if (!pr) return t;
                  return {
                    ...t,
                    done: pr.done,
                    speed: pr.speed,
                    segments: t.segments.map((s, i) => ({ ...s, done: pr.seg_done[i] ?? s.done })),
                  };
                }),
              );
              break;
          }
        });
      })
      .catch((e) => setBootErr(String(e)));
    return () => dispose?.();
  }, []);

  // Escape 关掉最上层: 菜单 → 新建页 → 设置页 → 详情栏
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      if (menu) { setMenu(null); return; }
      if (showNew) { setShowNew(false); return; }
      if (showSettings) { setShowSettings(false); return; }
      if (progressId != null) setProgressId(null);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [menu, showNew, showSettings, progressId]);

  useEffect(() => {
    if (!menu) return;
    const onPtr = (e: PointerEvent) => {
      const el = (e.target as HTMLElement | null)?.closest?.(".ctx-menu");
      if (el) return;
      setMenu(null);
    };
    // 下一帧再听: 否则本次右键的 pointerup 会立刻把刚打开的菜单关掉
    const id = requestAnimationFrame(() => {
      window.addEventListener("pointerdown", onPtr, true);
    });
    return () => {
      cancelAnimationFrame(id);
      window.removeEventListener("pointerdown", onPtr, true);
    };
  }, [menu]);

  const visible = useMemo(() => {
    let list = tasks;
    if (filter === "active") list = list.filter((t) => ["active", "probing", "paused"].includes(t.state));
    else if (filter.startsWith("type:")) list = list.filter((t) => fileType(t.name) === filter.slice(5));
    else if (filter !== "all") list = list.filter((t) => t.state === filter);
    if (query) list = list.filter((t) => t.name.toLowerCase().includes(query.toLowerCase()));
    const dir = sort.asc ? 1 : -1;
    return [...list].sort((a, b) => {
      switch (sort.key) {
        case "name": return dir * a.name.localeCompare(b.name, "zh");
        case "size": return dir * ((a.size ?? 0) - (b.size ?? 0));
        case "state": return dir * a.state.localeCompare(b.state);
        case "speed": return dir * (a.speed - b.speed);
        case "created": return dir * (a.created_at - b.created_at);
        default: return dir * (lastTry(a) - lastTry(b));
      }
    });
  }, [tasks, filter, query, sort]);

  const globalSpeed = tasks.reduce((a, t) => a + (t.state === "active" ? t.speed : 0), 0);
  const selected = tasks.find((t) => t.id === selectedId) || null;
  const menuTask = menu ? tasks.find((t) => t.id === menu.id) || null : null;
  const hasActive = tasks.some(isRunning);

  const toggleTask = (t: TaskInfo) => {
    const op = isRunning(t) || t.state === "queued" ? api.pauseTask(t.id) : api.resumeTask(t.id);
    op.catch(console.error);
  };

  const clickHeader = (key: SortKey) =>
    setSort((s) => ({ key, asc: s.key === key ? !s.asc : key === "name" }));

  const filterTitle = filter.startsWith("type:")
    ? TYPE_LABEL[filter.slice(5) as FileType]
    : SIDE_STATES.find((s) => s.key === filter)?.label || "全部";

  if (bootErr) {
    return (
      <div class="boot-fail">
        <IcoAlert size={28} />
        <div>核心未启动: {bootErr}</div>
      </div>
    );
  }

  const HEADERS: { key: SortKey | null; label: string }[] = [
    { key: "name", label: "文件名" },
    { key: "size", label: "大小" },
    { key: "state", label: "状态" },
    { key: "created", label: "创建时间" },
    { key: "time", label: "最后尝试" },
  ];

  return (
    <div class="app-window">
      <div class="sidebar">
        <div class="brand">
          <span class="brand-mark"><IcoDown size={16} /></span>
          <span class="brand-name">Dash Download</span>
        </div>
        <div class="side-group">
          <div class="side-title">下载</div>
          {SIDE_STATES.map((s) => {
            const count = s.key === "all"
              ? tasks.length
              : s.key === "active"
                ? tasks.filter((t) => ["active", "probing", "paused"].includes(t.state)).length
                : tasks.filter((t) => t.state === s.key).length;
            return (
              <div class={"side-item" + (!showSettings && !showNew && filter === s.key ? " on" : "")}
                onClick={() => { setShowSettings(false); setShowNew(false); setFilter(s.key); }}>
                <span class="side-ico"><s.icon size={18} /></span>
                <span class="side-label">{s.label}</span>
                <span class="side-count">{count || ""}</span>
              </div>
            );
          })}
        </div>
        <div class="side-group">
          <div class="side-title">分类</div>
          {FILE_TYPES.map((ft) => {
            const count = tasks.filter((t) => fileType(t.name) === ft).length;
            return (
              <div class={"side-item" + (!showSettings && !showNew && filter === `type:${ft}` ? " on" : "")}
                onClick={() => { setShowSettings(false); setShowNew(false); setFilter(`type:${ft}` as Filter); }}>
                <span class="side-ico"><TypeIcon type={ft} size={18} /></span>
                <span class="side-label">{TYPE_LABEL[ft]}</span>
                <span class="side-count">{count || ""}</span>
              </div>
            );
          })}
        </div>
        <div class="side-foot">
          <div class="global-speed">
            {globalSpeed > 0 ? fmtSpeed(globalSpeed).split(" ")[0] : "0"}
            <small>{globalSpeed > 0 ? fmtSpeed(globalSpeed).split(" ")[1] + " 总速度" : "空闲"}</small>
          </div>
          <div class="ext-pill">
            <span class={"ext-dot" + (boot ? "" : " off")}></span>
            {boot ? `核心运行中 v${boot.version}` : "连接中…"}
          </div>
          <div class={"side-item" + (showSettings ? " on" : "")}
            onClick={() => { setShowSettings(true); setShowNew(false); setMenu(null); }}>
            <span class="side-ico"><IcoGear size={18} /></span>
            <span class="side-label">设置</span>
          </div>
        </div>
      </div>

      {showSettings && boot ? (
        <SettingsPage boot={boot} />
      ) : showNew && boot ? (
        <NewTaskPage defaultDir={boot.default_dir} onClose={() => setShowNew(false)}
          onCreated={(id) => { setShowNew(false); setMenu(null); setSelectedId(id); setProgressId(id); }} />
      ) : (
        <div class="workspace">
          <div class="main">
            <div class="page-head">
              <div>
                <h1 class="page-title">{filterTitle}</h1>
                <p class="page-sub">{visible.length} 个任务</p>
              </div>
              <div class="page-head-right">
                <div class="search">
                  <IcoSearch size={16} />
                  <input placeholder="搜索文件名" value={query}
                    onInput={(e) => setQuery((e.target as HTMLInputElement).value)} />
                </div>
                <button class="icon-btn" title="切换主题" onClick={() => setTheme(theme === "light" ? "dark" : "light")}>
                  {theme === "light" ? <IcoMoon size={18} /> : <IcoSun size={18} />}
                </button>
              </div>
            </div>
            <div class="toolbar">
              <button class="btn primary" onClick={() => { setShowNew(true); setShowSettings(false); setMenu(null); }}>
                <IcoPlus size={16} /> 新建下载
              </button>
              <button class="btn" disabled={!selected || selected.state === "completed" || isRunning(selected) || selected.state === "queued"}
                onClick={() => selected && api.resumeTask(selected.id).catch(console.error)}>
                <IcoPlay size={16} /> 继续
              </button>
              <button class="btn" disabled={!selected || !(isRunning(selected) || selected.state === "queued")}
                onClick={() => selected && api.pauseTask(selected.id).catch(console.error)}>
                <IcoPause size={16} /> 暂停
              </button>
              <button class="btn danger" disabled={!selected}
                onClick={() => selected && api.removeTask(selected.id, true).catch(console.error)}>
                <IcoTrash size={16} /> 删除
              </button>
              <div class="spacer"></div>
              <button class="btn" onClick={() => (hasActive ? api.pauseAll() : api.resumeAll()).catch(console.error)}>
                {hasActive ? <IcoPause size={16} /> : <IcoPlay size={16} />}
                {hasActive ? "全部暂停" : "全部开始"}
              </button>
            </div>

            <div class="table">
              <div class="thead">
                {HEADERS.map((h) => (
                  <div class={h.key ? "sortable" : ""} onClick={() => h.key && clickHeader(h.key)}>
                    {h.label}
                    {h.key === sort.key && <span class="sort-arrow">{sort.asc ? "▲" : "▼"}</span>}
                  </div>
                ))}
              </div>
              {visible.length === 0 ? (
                <div class="empty" style={{ height: "60%" }}>
                  <IcoDown size={32} />
                  <span>没有{filterTitle === "全部" ? "" : filterTitle}任务</span>
                </div>
              ) : (
                visible.map((t) => (
                  <TaskRow key={t.id} t={t} selected={t.id === selectedId}
                    onSelect={() => { setSelectedId(t.id); setProgressId(t.id); }}
                    onOpenDetail={() => { setSelectedId(t.id); setMenu(null); setProgressId(t.id); }}
                    onMenu={(x, y) => { setSelectedId(t.id); setMenu({ x, y, id: t.id }); }} />
                ))
              )}
            </div>
          </div>

          {progressId != null && tasks.find((t) => t.id === progressId) && (
            <DetailPane
              t={tasks.find((t) => t.id === progressId)!}
              onToggle={() => toggleTask(tasks.find((t) => t.id === progressId)!)}
              onCancel={() => {
                const id = progressId;
                api.cancelTask(id).catch(console.error);
              }}
              onHide={() => { setMenu(null); setProgressId(null); }}
            />
          )}
        </div>
      )}

      {menu && menuTask && !showNew && !showSettings && (
        <ContextMenu menu={menu} t={menuTask}
          onClose={() => setMenu(null)}
          onDetail={() => { setMenu(null); setProgressId(menuTask.id); }} />
      )}
    </div>
  );
}

function TaskRow(props: {
  t: TaskInfo; selected: boolean;
  onSelect: () => void;
  onOpenDetail: () => void;
  onMenu: (x: number, y: number) => void;
}) {
  const { t } = props;
  const p = pct(t);
  const statusCell = t.state === "failed" || t.state === "queued" || t.state === "canceled"
    ? STATE_META[t.state].label
    : `${Math.floor(p * 100)}%`;

  const onDblClick = () => props.onOpenDetail();

  return (
    <div class={"trow" + (props.selected ? " selected" : "")}
      onClick={props.onSelect}
      onDblClick={onDblClick}
      onContextMenu={(e) => {
        e.preventDefault();
        props.onMenu(Math.min(e.clientX, window.innerWidth - 190), Math.min(e.clientY, window.innerHeight - 300));
      }}>
      <div class="cell-name">
        <span class="mini-ico"><TypeIcon type={fileType(t.name)} size={16} /></span>
        <span class="nm">{t.name || t.url}</span>
      </div>
      <div class="num">{t.size ? fmtBytes(t.size) : t.state === "completed" ? fmtBytes(t.done) : "—"}</div>
      <div class="num">{statusCell}</div>
      <div class="num">{fmtTime(t.created_at)}</div>
      <div class="num">{fmtTime(lastTry(t))}</div>
    </div>
  );
}

function ContextMenu(props: {
  menu: CtxMenu; t: TaskInfo;
  onClose: () => void;
  onDetail: () => void;
}) {
  const { t } = props;
  const running = isRunning(t) || t.state === "queued";
  const filePath = `${t.dir}/${t.name}`;
  const item = (label: string, action: () => void, cls = "") => (
    <div class={"ctx-item " + cls} onClick={() => { props.onClose(); action(); }}>{label}</div>
  );
  return (
    <div class="ctx-menu" style={{ left: props.menu.x, top: props.menu.y }}>
      {t.state !== "completed" && (running
        ? item("暂停", () => api.pauseTask(t.id).catch(console.error))
        : item("继续", () => api.resumeTask(t.id).catch(console.error)))}
      {item("重新下载", () => api.redownloadTask(t.id).catch(console.error))}
      <div class="ctx-sep"></div>
      {t.state === "completed" && item("打开", () => api.openPath(filePath))}
      {t.state === "completed" && item("打开所在文件夹", () => api.revealFile(filePath))}
      {item("复制链接地址", () => navigator.clipboard.writeText(t.url))}
      {item("进度", props.onDetail)}
      {t.state !== "completed" && t.state !== "canceled" && (
        item("取消", () => api.cancelTask(t.id).catch(console.error))
      )}
      <div class="ctx-sep"></div>
      {item("删除", () => api.removeTask(t.id, true).catch(console.error), "danger")}
    </div>
  );
}

function DetailPane(props: { t: TaskInfo; onToggle: () => void; onCancel: () => void; onHide: () => void }) {
  const { t } = props;
  const [tab, setTab] = useState<"download" | "options" | "connections">("download");
  const p = pct(t);
  const pctLabel = `${Math.floor(p * 100)}%`;
  const running = isRunning(t) || t.state === "queued";
  const conn = t.max_segments || t.segments.length || 8;
  const line = (k: string, v: string) => (
    <div class="kv-line"><span class="k">{k}</span><span class="v">{v}</span></div>
  );
  const resumable = t.state === "probing" || t.size == null
    ? "Unknown"
    : t.resumable ? "Yes" : "No";

  return (
    <aside class="detail">
      <div class="detail-top">
        <div class="prog-title">{pctLabel} {t.name || t.url}</div>
        <button class="icon-btn" title="关闭" onClick={props.onHide}><IcoX size={16} /></button>
      </div>
      <div class="prog-tabs">
        {(["download", "options", "connections"] as const).map((k) => (
          <button class={"prog-tab" + (tab === k ? " on" : "")} onClick={() => setTab(k)}>
            {k === "download" ? "下载" : k === "options" ? "选项" : "连接"}
          </button>
        ))}
      </div>
        <div class="prog-body">
          {tab === "download" && (
            <>
              {line("URL", t.url)}
              {line("Status", STATE_META[t.state].label + (t.error ? ` — ${t.error}` : ""))}
              {line("File Size", t.size ? fmtBytes(t.size) : "Unknown")}
              {line("Downloaded", `${fmtBytes(t.done)} ( ${pctLabel} )`)}
              {line("Bandwidth", t.state === "active" ? fmtSpeed(t.speed) : "0 Byte/sec")}
              {line("Remaining Time", t.state === "active" ? fmtEta(t) : "Unknown")}
              {line("Resumable", resumable)}
              <div class="prog-bar"><div style={{ width: `${(p * 100).toFixed(2)}%` }}></div></div>
              <div class="seg-title" style={{ margin: 0 }}>
                <span>Segments: {t.segments.length}</span>
              </div>
              <div class="seg-map">
                {(t.segments.length ? t.segments : [{ start: 0, end: 1, done: 0, idx: 0 }]).map((s) => {
                  const len = s.end > s.start ? s.end - s.start : Math.max(s.done, 1);
                  return (
                    <div class="seg-cell" style={{ flex: Math.max(len, 1) }}>
                      <div class="seg-cell-fill" style={{ width: `${Math.min(100, (s.done / len) * 100).toFixed(1)}%` }}></div>
                    </div>
                  );
                })}
              </div>
            </>
          )}
          {tab === "options" && (
            <>
              <div class="kv-line">
                <span class="k">Connections</span>
                <span class="v">
                  <div class="stepper">
                    <button onClick={() => api.setConnections(t.id, Math.max(1, conn - 1)).catch(console.error)}>−</button>
                    <b>{conn}</b>
                    <button onClick={() => api.setConnections(t.id, Math.min(16, conn + 1)).catch(console.error)}>+</button>
                  </div>
                </span>
              </div>
              <div class="settings-hint">
                {isRunning(t) ? "下载中修改会记下来, 暂停后再继续即按新连接数重切剩余分段." : "暂停状态下修改会立刻重切剩余分段."}
              </div>
            </>
          )}
          {tab === "connections" && (
            <>
              {t.segments.length === 0 && <div class="settings-hint">尚未开始分段</div>}
              {t.segments.map((s) => {
                const len = s.end > s.start ? s.end - s.start : Math.max(s.done, 1);
                return (
                  <div class="conn-row">
                    <span>#{s.idx}</span>
                    <span>{fmtBytes(s.start)} – {s.end ? fmtBytes(s.end) : "?"}</span>
                    <span>{Math.floor((s.done / len) * 100)}%</span>
                  </div>
                );
              })}
            </>
          )}
        </div>
        <div class="prog-foot">
          {t.state !== "completed" && (
            <button class="btn" onClick={props.onToggle} disabled={t.state === "probing"}>
              {running ? "暂停" : t.state === "failed" ? "重试" : "继续"}
            </button>
          )}
          {t.state !== "completed" && t.state !== "canceled" && (
            <button class="btn danger" onClick={props.onCancel}>取消</button>
          )}
        </div>
    </aside>
  );
}

function NewTaskPage(props: { defaultDir: string; onClose: () => void; onCreated: (id: number) => void }) {
  const [url, setUrl] = useState("");
  const [dir, setDir] = useState(props.defaultDir);
  const [conn, setConn] = useState(8);
  const [showAdv, setShowAdv] = useState(false);
  const [headers, setHeaders] = useState("");
  const [err, setErr] = useState("");
  const [busy, setBusy] = useState(false);

  // NDM 的 New URL 会自动读剪贴板: 打开弹窗时若剪贴板是 URL 则预填
  useEffect(() => {
    navigator.clipboard.readText()
      .then((text) => {
        if (/^https?:\/\/\S+$/i.test(text.trim())) setUrl(text.trim());
      })
      .catch(() => { /* 无剪贴板权限时忽略 */ });
  }, []);

  const urls = url.split("\n").map((s) => s.trim()).filter((s) => /^https?:\/\//i.test(s));
  const previewName = useMemo(() => {
    if (!urls.length) return "";
    try {
      const path = new URL(urls[0]).pathname;
      return decodeURIComponent(path.split("/").filter(Boolean).pop() || "") || "由服务器决定";
    } catch {
      return "";
    }
  }, [url]);

  const parseHeaders = (): [string, string][] =>
    headers.split("\n").map((l) => {
      const i = l.indexOf(":");
      return i > 0 ? [l.slice(0, i).trim(), l.slice(i + 1).trim()] as [string, string] : null;
    }).filter((x): x is [string, string] => !!x && !!x[0] && !!x[1]);

  const create = async (queueOnly: boolean) => {
    setBusy(true);
    setErr("");
    try {
      let firstId = 0;
      for (const u of urls) {
        const t = await api.addTask({
          url: u, dir, segments: conn, queue_only: queueOnly, headers: parseHeaders(),
        });
        if (!firstId) firstId = t.id;
      }
      props.onCreated(firstId);
    } catch (e) {
      setErr(String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div class="page">
      <div class="page-inner">
        <h1 class="page-title">新建下载</h1>
        <p class="page-sub">粘贴 http/https 链接, 支持多行批量.</p>
        <div class="field">
          <label>下载链接</label>
          <textarea rows={4} autofocus placeholder="https://…" value={url}
            onInput={(e) => setUrl((e.target as HTMLTextAreaElement).value)} />
        </div>
        {previewName && (
          <div class="file-preview">
            <TypeIcon type={fileType(previewName)} size={15} />
            <span style={{ flex: 1, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
              {previewName}{urls.length > 1 ? ` 等 ${urls.length} 个文件` : ""}
            </span>
            <span style={{ color: "var(--text-3)", fontSize: 11 }}>大小待探测</span>
          </div>
        )}
        <div class="field-row">
          <div class="field">
            <label>保存到</label>
            <input type="text" value={dir} onInput={(e) => setDir((e.target as HTMLInputElement).value)} />
          </div>
          <div class="field">
            <label>连接数</label>
            <div class="stepper">
              <button onClick={() => setConn(Math.max(1, conn - 1))}>−</button>
              <b>{conn}</b>
              <button onClick={() => setConn(Math.min(16, conn + 1))}>+</button>
            </div>
          </div>
        </div>
        <div>
          <button class="btn" style={{ border: "none", padding: 0, height: "auto", color: "var(--text-2)", fontSize: 12 }}
            onClick={() => setShowAdv(!showAdv)}>
            {showAdv ? "隐藏高级选项" : "高级选项…"}
          </button>
          {showAdv && (
            <div class="field" style={{ marginTop: 10 }}>
              <label>自定义 Header (每行一条, 如 Cookie: xxx)</label>
              <textarea rows={3} value={headers}
                placeholder={"Referer: https://example.com\nCookie: session=…"}
                onInput={(e) => setHeaders((e.target as HTMLTextAreaElement).value)} />
            </div>
          )}
        </div>
        {err && <div class="detail-err">{err}</div>}
        <div class="modal-foot">
          <button class="btn" onClick={props.onClose}>返回</button>
          <button class="btn" disabled={!urls.length || busy} onClick={() => create(true)}>加入队列</button>
          <button class="btn primary" disabled={!urls.length || busy} onClick={() => create(false)}>
            <IcoDown size={16} /> 开始下载
          </button>
        </div>
      </div>
    </div>
  );
}

function SettingsPage(props: { boot: Boot }) {
  const { boot } = props;
  return (
    <div class="page">
      <div class="page-inner">
        <h1 class="page-title">设置</h1>
        <p class="page-sub">本机下载引擎与浏览器扩展共用 localhost API, 无需配对令牌.</p>
        <div class="settings-block">
          <div class="settings-block-title">通用</div>
          <div class="settings-row">
            <div>
              <div class="settings-label">默认下载目录</div>
              <div class="settings-hint">新任务默认写到这里, 创建时可改</div>
            </div>
            <span class="mono-chip">{boot.default_dir}</span>
          </div>
          <div class="settings-row">
            <div>
              <div class="settings-label">同时下载 / 每任务连接数</div>
              <div class="settings-hint">当前版本固定 3 / 8, 可视化配置在 v1.1</div>
            </div>
            <span class="mono-chip">3 / 8</span>
          </div>
        </div>
        <div class="settings-block">
          <div class="settings-block-title">浏览器扩展</div>
          <div class="settings-row">
            <div>
              <div class="settings-label">API</div>
              <div class="settings-hint">仅绑定回环地址. 扩展加载后, app 在跑即可接管</div>
            </div>
            <span class="mono-chip">127.0.0.1:{boot.port}</span>
          </div>
          <div class="settings-row">
            <div>
              <div class="settings-label">核心版本</div>
            </div>
            <span class="mono-chip">v{boot.version}</span>
          </div>
        </div>
      </div>
    </div>
  );
}
