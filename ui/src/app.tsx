import type { JSX } from "preact";
import { useEffect, useMemo, useState } from "preact/hooks";
import * as api from "./api";
import type { Boot, TaskInfo } from "./api";
import {
  fileType, fmtBytes, fmtEta, fmtSpeed, fmtTime, pct, STATE_META, TYPE_LABEL,
} from "./util";
import type { FileType } from "./util";
import {
  IcoAlert, IcoCheck, IcoCopy, IcoDown, IcoFolder, IcoGear, IcoMoon, IcoPause,
  IcoPlay, IcoPlus, IcoQueue, IcoSearch, IcoSun, IcoTrash, IcoX, TypeIcon,
} from "./icons";

type Filter = "all" | "active" | "queued" | "completed" | "failed" | `type:${FileType}`;
type SortKey = "name" | "size" | "state" | "speed" | "time";

const SIDE_STATES: { key: Filter; label: string; icon: (p: { size?: number }) => JSX.Element }[] = [
  { key: "all", label: "全部", icon: IcoQueue },
  { key: "active", label: "下载中", icon: IcoDown },
  { key: "queued", label: "等待中", icon: IcoQueue },
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
  const [detailOpen, setDetailOpen] = useState(false);
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

  // 右键菜单: 点击任意处 / Escape 关闭
  useEffect(() => {
    if (!menu) return;
    const close = () => setMenu(null);
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && close();
    window.addEventListener("click", close);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("click", close);
      window.removeEventListener("keydown", onKey);
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
    { key: "speed", label: "带宽" },
    { key: null, label: "剩余时间" },
    { key: "time", label: "最后尝试" },
  ];

  return (
    <div class="app-window">
      <div class="sidebar">
        <div class="side-group">
          <div class="side-title">下载</div>
          {SIDE_STATES.map((s) => {
            const count = s.key === "all"
              ? tasks.length
              : s.key === "active"
                ? tasks.filter((t) => ["active", "probing", "paused"].includes(t.state)).length
                : tasks.filter((t) => t.state === s.key).length;
            return (
              <div class={"side-item" + (filter === s.key ? " on" : "")} onClick={() => setFilter(s.key)}>
                <span class="side-ico"><s.icon size={14} /></span>
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
              <div class={"side-item" + (filter === `type:${ft}` ? " on" : "")}
                onClick={() => setFilter(`type:${ft}` as Filter)}>
                <span class="side-ico"><TypeIcon type={ft} size={14} /></span>
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
          <div class="ext-pill" style={{ justifyContent: "space-between" }}>
            <span style={{ display: "flex", alignItems: "center", gap: 7 }}>
              <span class={"ext-dot" + (boot ? "" : " off")}></span>
              {boot ? `核心运行中 v${boot.version}` : "连接中…"}
            </span>
            <button class="icon-btn" style={{ width: 24, height: 24 }} title="设置"
              onClick={() => setShowSettings(true)}>
              <IcoGear size={14} />
            </button>
          </div>
        </div>
      </div>

      <div class="main">
        <div class="toolbar">
          <span class="tb-title">{filterTitle}</span>
          <span class="tb-sub">{visible.length} 项</span>
          <button class="btn primary" style={{ marginLeft: 10 }} onClick={() => setShowNew(true)}>
            <IcoPlus size={13} /> 新建下载
          </button>
          <button class="btn" disabled={!selected || selected.state === "completed" || isRunning(selected) || selected.state === "queued"}
            onClick={() => selected && api.resumeTask(selected.id).catch(console.error)}>
            <IcoPlay size={13} /> 继续
          </button>
          <button class="btn" disabled={!selected || !(isRunning(selected) || selected.state === "queued")}
            onClick={() => selected && api.pauseTask(selected.id).catch(console.error)}>
            <IcoPause size={13} /> 暂停
          </button>
          <button class="btn" disabled={!selected}
            onClick={() => selected && api.removeTask(selected.id).catch(console.error)}>
            <IcoTrash size={13} /> 删除
          </button>
          <div class="spacer"></div>
          <button class="btn" onClick={() => (hasActive ? api.pauseAll() : api.resumeAll()).catch(console.error)}>
            {hasActive ? <IcoPause size={13} /> : <IcoPlay size={13} />}
            {hasActive ? "全部暂停" : "全部开始"}
          </button>
          <div class="search">
            <IcoSearch size={13} />
            <input placeholder="搜索" value={query}
              onInput={(e) => setQuery((e.target as HTMLInputElement).value)} />
          </div>
          <button class="icon-btn" title="切换主题" onClick={() => setTheme(theme === "light" ? "dark" : "light")}>
            {theme === "light" ? <IcoMoon size={15} /> : <IcoSun size={15} />}
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
              <IcoDown size={28} />
              <span>没有{filterTitle === "全部" ? "" : filterTitle}任务</span>
            </div>
          ) : (
            visible.map((t) => (
              <TaskRow key={t.id} t={t} selected={t.id === selectedId}
                onSelect={() => setSelectedId(t.id)}
                onOpenDetail={() => { setSelectedId(t.id); setDetailOpen(true); }}
                onMenu={(x, y) => { setSelectedId(t.id); setMenu({ x, y, id: t.id }); }} />
            ))
          )}
        </div>
      </div>

      {detailOpen && selected && (
        <DetailPanel t={selected} onToggle={() => toggleTask(selected)}
          onRemove={() => { api.removeTask(selected.id).catch(console.error); setDetailOpen(false); }}
          onClose={() => setDetailOpen(false)} />
      )}

      {menu && menuTask && <ContextMenu menu={menu} t={menuTask}
        onDetail={() => setDetailOpen(true)} />}

      {showNew && boot && (
        <NewTaskModal defaultDir={boot.default_dir} onClose={() => setShowNew(false)}
          onCreated={(id) => { setShowNew(false); setSelectedId(id); }} />
      )}
      {showSettings && boot && <SettingsModal boot={boot} onClose={() => setShowSettings(false)} />}
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
  const running = isRunning(t);
  const fillCls = t.state === "failed" ? "err" : running ? "" : "paused";
  const statusCell = t.state === "active"
    ? `${(p * 100).toFixed(1)}%`
    : STATE_META[t.state].label;

  const onDblClick = () => {
    if (t.state === "completed") api.openPath(`${t.dir}/${t.name}`);
    else props.onOpenDetail();
  };

  return (
    <div class={"trow" + (props.selected ? " selected" : "")}
      onClick={props.onSelect}
      onDblClick={onDblClick}
      onContextMenu={(e) => {
        e.preventDefault();
        props.onMenu(Math.min(e.clientX, window.innerWidth - 190), Math.min(e.clientY, window.innerHeight - 300));
      }}>
      <div class="cell-name">
        <span class="mini-ico"><TypeIcon type={fileType(t.name)} size={13} /></span>
        <span class="nm">{t.name || t.url}</span>
      </div>
      <div class="num">{t.size ? fmtBytes(t.size) : t.state === "completed" ? fmtBytes(t.done) : "—"}</div>
      <div class="cell-status">
        <span class={"state-badge " + STATE_META[t.state].cls} style={{ width: "fit-content" }}>{statusCell}</span>
        {t.state !== "completed" && t.done > 0 && (
          <div class="bar"><div class={"bar-fill " + fillCls} style={{ width: `${(p * 100).toFixed(2)}%` }}></div></div>
        )}
      </div>
      <div class="num">{t.state === "active" ? fmtSpeed(t.speed) : "—"}</div>
      <div class="num">{t.state === "active" ? fmtEta(t) : "—"}</div>
      <div class="num">{fmtTime(lastTry(t))}</div>
    </div>
  );
}

function ContextMenu(props: { menu: CtxMenu; t: TaskInfo; onDetail: () => void }) {
  const { t } = props;
  const running = isRunning(t) || t.state === "queued";
  const filePath = `${t.dir}/${t.name}`;
  const item = (label: string, action: () => void, cls = "") => (
    <div class={"ctx-item " + cls} onClick={action}>{label}</div>
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
      {item("属性", props.onDetail)}
      <div class="ctx-sep"></div>
      {item("删除", () => api.removeTask(t.id).catch(console.error), "danger")}
    </div>
  );
}

function DetailPanel(props: { t: TaskInfo; onToggle: () => void; onRemove: () => void; onClose: () => void }) {
  const { t } = props;
  const p = pct(t);
  const running = isRunning(t);
  return (
    <div class="detail">
      <div class="detail-head">
        <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
          <span class={"state-badge " + STATE_META[t.state].cls}>{STATE_META[t.state].label}</span>
          <button class="icon-btn" onClick={props.onClose}><IcoX size={13} /></button>
        </div>
        <div class="detail-name">{t.name || t.url}</div>
        <div style={{ fontSize: 11.5, color: "var(--text-3)" }}>
          {TYPE_LABEL[fileType(t.name)]} · {t.size ? fmtBytes(t.size) : "大小未知"}
          {t.resumable ? "" : " · 不支持断点"}
        </div>
      </div>

      {t.state !== "completed" && t.segments.length > 0 && (
        <div class="seg-section">
          <div class="seg-title">
            <span>分段 ({t.segments.length})</span>
            <span>{(p * 100).toFixed(1)}%</span>
          </div>
          <div class="seg-map">
            {t.segments.map((s) => {
              const len = s.end > s.start ? s.end - s.start : Math.max(s.done, 1);
              return (
                <div class="seg-cell" style={{ flex: len }}>
                  <div class="seg-cell-fill" style={{ width: `${Math.min(100, (s.done / len) * 100).toFixed(1)}%` }}></div>
                </div>
              );
            })}
          </div>
          {t.state === "failed" && <div class="detail-err">{t.error}</div>}
        </div>
      )}

      <div class="stat-grid">
        <div><div class="stat-label">已下载</div><div class="stat-value">{fmtBytes(t.done)}</div></div>
        <div><div class="stat-label">总大小</div><div class="stat-value">{t.size ? fmtBytes(t.size) : "—"}</div></div>
        <div><div class="stat-label">速度</div><div class="stat-value">{t.state === "active" ? fmtSpeed(t.speed) : "—"}</div></div>
        <div><div class="stat-label">剩余时间</div><div class="stat-value">{t.state === "active" ? fmtEta(t) : "—"}</div></div>
        <div><div class="stat-label">创建时间</div><div class="stat-value">{fmtTime(t.created_at)}</div></div>
        <div><div class="stat-label">完成时间</div><div class="stat-value">{fmtTime(t.completed_at) || "—"}</div></div>
      </div>

      <div class="kv-section">
        <div>
          <div class="kv-label">
            链接 (永久保留, 可重新下载)
            <button class="icon-btn" style={{ width: 20, height: 20 }}
              onClick={() => navigator.clipboard.writeText(t.url)}>
              <IcoCopy size={12} />
            </button>
          </div>
          <div class="kv-value">{t.url}</div>
        </div>
        {t.final_url !== t.url && (
          <div>
            <div class="kv-label">实际地址 (重定向后)</div>
            <div class="kv-value">{t.final_url}</div>
          </div>
        )}
        <div>
          <div class="kv-label">保存位置</div>
          <div class="kv-value">{t.dir}/{t.name}</div>
        </div>
      </div>

      <div class="detail-actions">
        {t.state === "completed" ? (
          <button class="btn" style={{ flex: 1 }} onClick={() => api.revealFile(`${t.dir}/${t.name}`)}>
            <IcoFolder size={13} /> 在 Finder 中显示
          </button>
        ) : (
          <button class="btn" style={{ flex: 1 }} onClick={props.onToggle}>
            {running || t.state === "queued" ? <IcoPause size={13} /> : <IcoPlay size={13} />}
            {running || t.state === "queued" ? "暂停" : t.state === "failed" ? "重试" : "开始"}
          </button>
        )}
        <button class="btn danger" onClick={props.onRemove}><IcoTrash size={13} /> 删除</button>
      </div>
    </div>
  );
}

function NewTaskModal(props: { defaultDir: string; onClose: () => void; onCreated: (id: number) => void }) {
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
    <div class="overlay" onClick={props.onClose}>
      <div class="modal" onClick={(e) => e.stopPropagation()}>
        <h2>新建下载</h2>
        <div class="field">
          <label>下载链接 (支持多行批量)</label>
          <textarea rows={3} autofocus placeholder="https://…" value={url}
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
          <button class="btn" onClick={props.onClose}>取消</button>
          <button class="btn" disabled={!urls.length || busy} onClick={() => create(true)}>加入队列</button>
          <button class="btn primary" disabled={!urls.length || busy} onClick={() => create(false)}>
            <IcoDown size={13} /> 开始下载
          </button>
        </div>
      </div>
    </div>
  );
}

function SettingsModal(props: { boot: Boot; onClose: () => void }) {
  const { boot } = props;
  const [copied, setCopied] = useState(false);
  const copyToken = () => {
    navigator.clipboard.writeText(boot.token);
    setCopied(true);
    setTimeout(() => setCopied(false), 1500);
  };
  return (
    <div class="overlay" onClick={props.onClose}>
      <div class="modal" style={{ width: 520 }} onClick={(e) => e.stopPropagation()}>
        <h2>设置</h2>
        <div>
          <div class="side-title" style={{ padding: "0 0 2px" }}>通用</div>
          <div class="settings-row">
            <div><div class="settings-label">默认下载目录</div></div>
            <span class="mono-chip">{boot.default_dir}</span>
          </div>
          <div class="settings-row">
            <div>
              <div class="settings-label">同时下载任务数 / 每任务连接数</div>
              <div class="settings-hint">当前版本为固定值 3 / 8, 可视化配置在 v1.1 提供</div>
            </div>
            <span class="mono-chip">3 / 8</span>
          </div>
        </div>
        <div>
          <div class="side-title" style={{ padding: "6px 0 2px" }}>浏览器扩展</div>
          <div class="settings-row">
            <div><div class="settings-label">API 地址</div></div>
            <span class="mono-chip">127.0.0.1:{boot.port}</span>
          </div>
          <div class="settings-row">
            <div>
              <div class="settings-label">访问令牌</div>
              <div class="settings-hint">粘贴到扩展设置中完成配对</div>
            </div>
            <span class="mono-chip">
              {copied ? "已复制" : boot.token.slice(0, 10) + "···"}
              <button onClick={copyToken}><IcoCopy size={12} /></button>
            </span>
          </div>
          <div class="settings-row">
            <div><div class="settings-label">核心版本</div></div>
            <span class="mono-chip">v{boot.version}</span>
          </div>
        </div>
        <div class="modal-foot">
          <button class="btn primary" onClick={props.onClose}>完成</button>
        </div>
      </div>
    </div>
  );
}
