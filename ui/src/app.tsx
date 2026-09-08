import type { JSX } from "preact";
import { useEffect, useMemo, useState } from "preact/hooks";
import * as api from "./api";
import type { Boot, TaskInfo, TorrentInfo } from "./api";
import { MAX_CONN } from "./api";
import { SettingsPage, updatePhaseText } from "./settings";
import { NumStep } from "./fields";
import { LANG, t } from "./i18n";
import { FilePickModal, NewTaskPage, ResolveModal, TorrentPane } from "./torrent-ui";
import {
  FILE_TYPE_ORDER, fileType, fmtBytes, fmtEta, fmtRate, fmtSpeed, fmtTime, pct, STATE_META, TYPE_LABEL,
} from "./util";
import type { FileType } from "./util";
import {
  IcoAlert, IcoCheck, IcoClock, IcoDown, IcoGear, IcoMoon, IcoOpen, IcoPause,
  IcoPlay, IcoPlus, IcoQueue, IcoSearch, IcoSun, IcoTrash, IcoX, TypeIcon,
} from "./icons";

type Filter = "all" | "active" | "queued" | "completed" | "failed" | `type:${FileType}`;
type SortKey = "name" | "size" | "state" | "speed" | "created" | "time";
const tx = t;

const SIDE_STATES: { key: Filter; label: string; icon: (p: { size?: number }) => JSX.Element }[] = [
  { key: "all", label: t("全部", "All"), icon: IcoQueue },
  { key: "active", label: t("下载中", "Downloading"), icon: IcoDown },
  { key: "queued", label: t("等待中", "Queued"), icon: IcoClock },
  { key: "completed", label: t("已完成", "Completed"), icon: IcoCheck },
  { key: "failed", label: t("失败", "Failed"), icon: IcoAlert },
];



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
  key: string;
}

type Row = {
  key: string;
  kind: "http" | "bt";
  id: number;
  name: string;
  dir: string;
  size: number | null;
  done: number;
  speed: number;
  up_speed: number;
  peers: number;
  connecting: number;
  seen: number;
  phase: string;
  state: string;
  created_at: number;
  completed_at: number | null;
  url: string;
  fileN: number;
};

function rowsOf(tasks: TaskInfo[], torrents: TorrentInfo[]): Row[] {
  return [
    ...tasks.map((t): Row => ({
      key: "h-" + t.id, kind: "http", id: t.id, name: t.name || t.url, dir: t.dir,
      size: t.size, done: t.done, speed: t.speed, up_speed: 0, peers: 0, connecting: 0, seen: 0, phase: "",
      state: t.state, created_at: t.created_at, completed_at: t.completed_at, url: t.url,
      fileN: 1,
    })),
    ...torrents.filter((t) => t.state !== "resolving").map((t): Row => ({
      key: "b-" + t.id, kind: "bt", id: t.id, name: t.name, dir: t.dir,
      size: t.size, done: t.done, speed: t.speed, up_speed: t.up_speed, peers: t.peers,
      connecting: t.connecting || 0, seen: t.seen || 0,
      phase: t.phase || "", state: t.state,
      created_at: t.created_at, completed_at: t.completed_at, url: t.source,
      fileN: t.files.length,
    })),
  ];
}

/// 单文件打开用户下载目录; 多文件 BT 打开种子名那一层.
function rowFolder(t: Row): string {
  if (t.kind === "bt" && t.fileN >= 2) return `${t.dir}/${t.name}`;
  return t.dir;
}

export function App() {
  const [boot, setBoot] = useState<Boot | null>(null);
  const [bootErr, setBootErr] = useState("");
  const [tasks, setTasks] = useState<TaskInfo[]>([]);
  const [torrents, setTorrents] = useState<TorrentInfo[]>([]);
  const [filter, setFilter] = useState<Filter>("all");
  const [query, setQuery] = useState("");
  const [selectedId, setSelectedId] = useState<number | null>(null);
  const [detail, setDetail] = useState<{ kind: "http" | "bt"; id: number } | null>(null);
  const [menu, setMenu] = useState<CtxMenu | null>(null);
  const [sort, setSort] = useState<{ key: SortKey; asc: boolean }>({ key: "time", asc: false });
  const [showNew, setShowNew] = useState(false);
  const [showSettings, setShowSettings] = useState(false);
  const [theme, setTheme] = useState(initTheme);
  const [picked, setPicked] = useState<Set<string>>(() => new Set());
  const [delAsk, setDelAsk] = useState<{ keys: string[]; delFile: boolean } | null>(null);
  const [pickFiles, setPickFiles] = useState<TorrentInfo | null>(null);
  const [resolve, setResolve] = useState<{
    id: number; source: string; name: string; phase: "work" | "fail"; error: string;
  } | null>(null);
  const [toast, setToast] = useState("");
  const [paneOn, setPaneOn] = useState(false);
  const [eng, setEng] = useState<api.EngineSettings | null>(null);

  useEffect(() => {
    document.documentElement.dataset.theme = theme;
    localStorage.setItem("dd_theme", theme);
  }, [theme]);

  useEffect(() => {
    if (!toast) return;
    const t = window.setTimeout(() => setToast(""), 2500);
    return () => window.clearTimeout(t);
  }, [toast]);

  const openFile = (path: string) => {
    api.openPath(path).catch((e) => {
      const msg = String(e).replace(/^Error: /, "");
      setToast(msg || t("文件不存在", "File not found"));
    });
  };

  useEffect(() => {
    let dispose: (() => void) | undefined;
    api.init()
      .then(async (b) => {
        setBoot(b);
        setEng(await api.getSettings());
        dispose = api.connectEvents((ev) => {
          switch (ev.type) {
            case "snapshot":
              setTasks(ev.tasks);
              setTorrents(ev.torrents || []);
              break;
            case "task_added":
              setTasks((p) => [ev.task, ...p.filter((t) => t.id !== ev.task.id)]);
              setSelectedId(ev.task.id);
              setPicked(new Set(["h-" + ev.task.id]));
              setMenu(null);
              setDetail({ kind: "http", id: ev.task.id });
              setPaneOn(true);
              break;
            case "task_updated":
              setTasks((p) => p.map((t) => (t.id === ev.task.id ? ev.task : t)));
              break;
            case "task_removed":
              setTasks((p) => p.filter((t) => t.id !== ev.id));
              setPicked((prev) => {
                const k = "h-" + ev.id;
                if (!prev.has(k)) return prev;
                const n = new Set(prev);
                n.delete(k);
                return n;
              });
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
            case "resolving":
              setShowNew(false);
              setResolve({
                id: ev.torrent.id,
                source: ev.torrent.source,
                name: ev.torrent.name,
                phase: "work",
                error: "",
              });
              break;
            case "torrent_added":
              setResolve((cur) => (cur && cur.id === ev.torrent.id ? null : cur));
              setTorrents((p) => [ev.torrent, ...p.filter((t) => t.id !== ev.torrent.id)]);
              setPicked(new Set(["b-" + ev.torrent.id]));
              setSelectedId(ev.torrent.id);
              setDetail({ kind: "bt", id: ev.torrent.id });
              setPaneOn(true);
              setMenu(null);
              if (ev.torrent.state === "awaiting_selection") setPickFiles(ev.torrent);
              break;
            case "torrent_updated":
              setTorrents((p) => p.map((t) => (t.id === ev.torrent.id ? ev.torrent : t)));
              if (ev.torrent.state === "awaiting_selection") {
                setPickFiles((cur) => (cur && cur.id === ev.torrent.id ? ev.torrent : cur || ev.torrent));
              }
              if (ev.torrent.state !== "awaiting_selection") {
                setPickFiles((cur) => (cur && cur.id === ev.torrent.id ? null : cur));
              }
              break;
            case "torrent_removed":
              setTorrents((p) => p.filter((t) => t.id !== ev.id));
              setPicked((prev) => {
                const k = "b-" + ev.id;
                if (!prev.has(k)) return prev;
                const n = new Set(prev);
                n.delete(k);
                return n;
              });
              break;
            case "torrent_progress":
              setTorrents((p) =>
                p.map((t) => {
                  const pr = ev.torrents.find((x) => x.id === t.id);
                  if (!pr) return t;
                  return {
                    ...t,
                    done: pr.done,
                    speed: pr.speed,
                    up_speed: pr.up_speed,
                    peers: pr.peers,
                    connecting: pr.connecting ?? t.connecting,
                    seen: pr.seen ?? t.seen,
                    phase: pr.phase || t.phase,
                    peer_list: pr.peer_list ?? t.peer_list,
                  };
                }),
              );
              break;
            case "resolve_failed":
              setShowNew(false);
              setResolve({
                id: ev.id,
                source: ev.source,
                name: "",
                phase: "fail",
                error: ev.error,
              });
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
      if (resolve) {
        if (resolve.phase === "work") api.removeTorrent(resolve.id, false).catch(() => {});
        setResolve(null);
        return;
      }
      if (showNew) { setShowNew(false); return; }
      if (showSettings) { setShowSettings(false); return; }
      if (paneOn) { setPaneOn(false); return; }
      if (detail != null) setDetail(null);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [menu, resolve, showNew, showSettings, paneOn, detail]);

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

  const allRows = useMemo(() => rowsOf(tasks, torrents), [tasks, torrents]);
  const visible = useMemo(() => {
    let list = allRows;
    if (filter === "active") {
      list = list.filter((t) => ["active", "probing", "paused", "resolving", "seeding"].includes(t.state));
    } else if (filter === "completed") {
      list = list.filter((t) => t.state === "completed" || t.state === "seeding");
    } else if (filter.startsWith("type:")) {
      list = list.filter((t) => fileType(t.name) === filter.slice(5));
    } else if (filter !== "all") {
      list = list.filter((t) => t.state === filter);
    }
    if (query) list = list.filter((t) => t.name.toLowerCase().includes(query.toLowerCase()));
    const dir = sort.asc ? 1 : -1;
    return [...list].sort((a, b) => {
      switch (sort.key) {
        case "name": return dir * a.name.localeCompare(b.name, LANG);
        case "size": return dir * ((a.size ?? 0) - (b.size ?? 0));
        case "state": return dir * a.state.localeCompare(b.state);
        case "speed": return dir * (a.speed - b.speed);
        case "created": return dir * (a.created_at - b.created_at);
        default: return dir * ((a.completed_at ?? a.created_at) - (b.completed_at ?? b.created_at));
      }
    });
  }, [allRows, filter, query, sort]);

  const globalSpeed = allRows.reduce((a, t) => a + (t.state === "active" || t.state === "seeding" ? t.speed : 0), 0);
  const selected = visible.find((t) => t.id === selectedId && picked.has(t.key)) || visible.find((t) => t.key === [...picked][0]) || null;
  const httpDetail = detail?.kind === "http" ? tasks.find((t) => t.id === detail.id) : undefined;
  const btDetail = detail?.kind === "bt" ? torrents.find((t) => t.id === detail.id) : undefined;
  const menuRow = menu ? allRows.find((t) => t.key === menu.key) || null : null;
  const hasActive = tasks.some((t) => isRunning(t) || t.state === "queued")
    || torrents.some((t) => t.state === "active" || t.state === "queued" || t.state === "seeding");

  const toggleRow = (r: Row) => {
    const running = r.kind === "bt"
      ? ["active", "queued", "resolving", "seeding"].includes(r.state)
      : ["active", "probing", "queued"].includes(r.state);
    const op = r.kind === "bt"
      ? (running ? api.pauseTorrent(r.id) : api.resumeTorrent(r.id))
      : (running ? api.pauseTask(r.id) : api.resumeTask(r.id));
    op.catch(console.error);
  };

  /// 先关确认框，避免批量删除等待/部分失败时弹窗卡住。
  const removeMany = (keys: string[], delFile: boolean) => {
    if (!keys.length) return;
    setDelAsk(null);
    setPicked(new Set());
    setMenu(null);
    Promise.allSettled(keys.map((k) => {
      const id = Number(k.slice(2));
      return k.startsWith("b-") ? api.removeTorrent(id, delFile) : api.removeTask(id, delFile);
    })).then((rs) => {
      const fail = rs.find((r) => r.status === "rejected");
      if (fail && fail.status === "rejected") console.error(fail.reason);
    });
  };

  const onRowClick = (t: Row, e: MouseEvent) => {
    const keys = visible.map((x) => x.key);
    if (e.shiftKey && picked.size) {
      const last = [...picked][picked.size - 1];
      const a = keys.indexOf(last);
      const b = keys.indexOf(t.key);
      if (a >= 0 && b >= 0) {
        setPicked(new Set(keys.slice(Math.min(a, b), Math.max(a, b) + 1)));
        setSelectedId(t.id);
        setDetail({ kind: t.kind, id: t.id });
        setPaneOn(true);
        return;
      }
    }
    if (e.metaKey || e.ctrlKey) {
      setPicked((prev) => {
        const n = new Set(prev);
        if (n.has(t.key)) n.delete(t.key); else n.add(t.key);
        return n;
      });
      setSelectedId(t.id);
      return;
    }
    setPicked(new Set([t.key]));
    setSelectedId(t.id);
    setDetail({ kind: t.kind, id: t.id });
    setPaneOn(true);
    if (t.kind === "bt" && t.state === "awaiting_selection") {
      const tr = torrents.find((x) => x.id === t.id);
      if (tr) setPickFiles(tr);
    }
  };

  const visIds = visible.map((x) => x.key);
  const pickN = visIds.filter((id) => picked.has(id)).length;
  const allOn = visIds.length > 0 && pickN === visIds.length;
  const mid = pickN > 0 && !allOn;

  const clickHeader = (key: SortKey) =>
    setSort((s) => ({ key, asc: s.key === key ? !s.asc : key === "name" }));

  const filterTitle = filter.startsWith("type:")
    ? TYPE_LABEL[filter.slice(5) as FileType]
    : SIDE_STATES.find((s) => s.key === filter)?.label || t("全部", "All");

  if (bootErr) {
    return (
      <div class="boot-fail">
        <IcoAlert size={28} />
        <div>{t("核心未启动", "Core failed to start")}: {bootErr}</div>
      </div>
    );
  }

  const HEADERS: { key: SortKey | null; label: string }[] = [
    { key: "name", label: t("文件名", "Name") },
    { key: "size", label: t("大小", "Size") },
    { key: "state", label: t("状态", "Status") },
    { key: "created", label: t("创建时间", "Created") },
    { key: "time", label: t("最后尝试", "Last attempt") },
  ];

  return (
    <div class="app-window">
      <div class="sidebar">
        {/* 窗口标题已经是产品名, 侧栏再放标+字会和 titlebar 重复 */}
        <div class="side-group">
          <div class="side-title">{t("下载", "Downloads")}</div>
          {SIDE_STATES.map((s) => {
            const count = s.key === "all"
              ? allRows.length
              : s.key === "active"
                ? allRows.filter((t) => ["active", "probing", "paused", "resolving", "seeding"].includes(t.state)).length
                : s.key === "completed"
                  ? allRows.filter((t) => t.state === "completed" || t.state === "seeding").length
                  : allRows.filter((t) => t.state === s.key).length;
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
          <div class="side-title">{t("分类", "Categories")}</div>
          {FILE_TYPE_ORDER.map((ft) => {
            const count = allRows.filter((t) => fileType(t.name) === ft).length;
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
            <small>{globalSpeed > 0 ? fmtSpeed(globalSpeed).split(" ")[1] + t(" 总速度", " total") : t("空闲", "Idle")}</small>
          </div>
          <div class="ext-pill">
            <span class={"ext-dot" + (boot ? "" : " off")}></span>
            {boot ? t(`核心运行中 v${boot.version}`, `Core running v${boot.version}`) : t("连接中…", "Connecting…")}
          </div>
          <div class={"side-item" + (showSettings ? " on" : "")}
            onClick={() => { setShowSettings(true); setShowNew(false); setMenu(null); }}>
            <span class="side-ico"><IcoGear size={18} /></span>
            <span class="side-label">{t("设置", "Settings")}</span>
          </div>
        </div>
      </div>

      {showSettings && boot && eng ? (
        <SettingsPage boot={boot} eng={eng} onEng={(s) => {
          setEng(s);
          setBoot({ ...boot, default_dir: s.default_dir });
        }} />
      ) : showNew && boot && eng ? (
        <NewTaskPage defaultDir={eng.default_dir} defaultConn={eng.max_segments} onClose={() => setShowNew(false)}
          onCreated={(t) => {
            setShowNew(false);
            setMenu(null);
            if (!t) return;
            if (t.state === "resolving") {
              setResolve({ id: t.id, source: t.source, name: t.name, phase: "work", error: "" });
            }
            if (t.state === "awaiting_selection") setPickFiles(t);
          }} />
      ) : (
        <div class="workspace">
          <div class="main">
            {toast && (
              <div class="update-bar" style={{ color: "var(--red)" }} onClick={() => setToast("")}>
                {toast}
              </div>
            )}
            <UpdateBar />
            <div class="page-head">
              <div>
                <h1 class="page-title">{filterTitle}</h1>
                <p class="page-sub">{t(`${visible.length} 个任务`, `${visible.length} tasks`)}</p>
              </div>
              <div class="page-head-right">
                <div class="search">
                  <IcoSearch size={16} />
                  <input placeholder={t("搜索文件名", "Search files")} value={query}
                    onInput={(e) => setQuery((e.target as HTMLInputElement).value)} />
                </div>
                <button class="icon-btn" title={t("切换主题", "Toggle theme")} onClick={() => setTheme(theme === "light" ? "dark" : "light")}>
                  {theme === "light" ? <IcoMoon size={18} /> : <IcoSun size={18} />}
                </button>
              </div>
            </div>
            <div class="toolbar">
              <button class="btn primary" onClick={() => { setShowNew(true); setShowSettings(false); setMenu(null); }}>
                <IcoPlus size={16} /> {t("新建下载", "New download")}
              </button>
              <button class="btn" disabled={!selected || ["completed", "active", "probing", "queued", "resolving", "seeding"].includes(selected.state)}
                onClick={() => selected && toggleRow(selected)}>
                <IcoPlay size={16} /> {t("继续", "Resume")}
              </button>
              <button class="btn" disabled={!selected || !["active", "probing", "queued", "resolving", "seeding"].includes(selected.state)}
                onClick={() => selected && toggleRow(selected)}>
                <IcoPause size={16} /> {t("暂停", "Pause")}
              </button>
              <button class="btn danger" disabled={picked.size === 0}
                onClick={() => setDelAsk({ keys: [...picked], delFile: true })}>
                <IcoTrash size={16} /> {t("删除", "Delete")}
              </button>
              <div class="spacer"></div>
              <button class="btn" onClick={() => (hasActive ? api.pauseAll() : api.resumeAll()).catch(console.error)}>
                {hasActive ? <IcoPause size={16} /> : <IcoPlay size={16} />}
                {hasActive ? t("全部暂停", "Pause all") : t("全部开始", "Start all")}
              </button>
            </div>

            <div class="table" onClick={(e) => {
              const el = e.target as HTMLElement;
              if (el.closest(".trow, .chk, .sortable, .thead")) return;
              setPaneOn(false);
            }}>
              <div class="thead">
                <div>
                  <button type="button" class={"chk" + (allOn ? " on" : mid ? " mid" : "")}
                    title={t("全选", "Select all")} disabled={!visible.length}
                    onClick={(e) => {
                      e.stopPropagation();
                      setPicked(allOn ? new Set() : new Set(visIds));
                    }} />
                </div>
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
                  <span>{filter === "all"
                    ? t("没有任务", "No tasks")
                    : t(`没有${filterTitle}任务`, `No ${filterTitle.toLowerCase()} tasks`)}</span>
                </div>
              ) : (
                visible.map((t) => (
                  <TaskRow key={t.key} t={t} selected={picked.has(t.key)}
                    onSelect={(e) => onRowClick(t, e)}
                    onTogglePick={() => {
                      setPicked((prev) => {
                        const n = new Set(prev);
                        if (n.has(t.key)) n.delete(t.key); else n.add(t.key);
                        return n;
                      });
                      setSelectedId(t.id);
                    }}
                    onOpenDetail={() => {
                      setSelectedId(t.id); setMenu(null);
                      setDetail({ kind: t.kind, id: t.id });
                      setPaneOn(true);
                      if (t.kind === "bt" && t.state === "awaiting_selection") {
                        const tr = torrents.find((x) => x.id === t.id);
                        if (tr) setPickFiles(tr);
                      }
                    }}
                    onOpenFile={() => openFile(`${t.dir}/${t.name}`)}
                    onMenu={(x, y) => { setSelectedId(t.id); setPicked(new Set([t.key])); setMenu({ x, y, key: t.key }); }} />
                ))
              )}
            </div>
          </div>

          {httpDetail && (
            <DetailPane
              t={httpDetail}
              collapsed={!paneOn}
              onToggle={() => {
                const op = isRunning(httpDetail) || httpDetail.state === "queued"
                  ? api.pauseTask(httpDetail.id) : api.resumeTask(httpDetail.id);
                op.catch(console.error);
              }}
              onCancel={() => { api.cancelTask(httpDetail.id).catch(console.error); }}
              onHide={() => { setMenu(null); setPaneOn(false); }}
            />
          )}
          {btDetail && (
            <TorrentPane
              t={btDetail}
              collapsed={!paneOn}
              onToggle={() => {
                const running = btDetail.state === "active" || btDetail.state === "seeding"
                  || btDetail.state === "queued";
                const op = running ? api.pauseTorrent(btDetail.id) : api.resumeTorrent(btDetail.id);
                op.catch(console.error);
              }}
              onPickFiles={() => setPickFiles(btDetail)}
              onHide={() => { setMenu(null); setPaneOn(false); }}
            />
          )}
        </div>
      )}

      {menu && menuRow && !showNew && !showSettings && (
        <ContextMenu menu={menu} t={menuRow}
          onClose={() => setMenu(null)}
          onOpenFile={() => openFile(`${menuRow.dir}/${menuRow.name}`)}
          onDetail={() => {
            setMenu(null);
            setDetail({ kind: menuRow.kind, id: menuRow.id });
            setPaneOn(true);
          }}
          onDelete={() => setDelAsk({ keys: [menuRow.key], delFile: true })} />
      )}

      {delAsk && (
        <div class="overlay" onClick={() => setDelAsk(null)}>
          <div class="modal" onClick={(e) => e.stopPropagation()}>
            <h2 class="page-title" style={{ fontSize: 16 }}>
              {t(`删除 ${delAsk.keys.length} 项？`, `Delete ${delAsk.keys.length} items?`)}
            </h2>
            <label class="settings-hint" style={{ display: "flex", gap: 8, alignItems: "center", margin: "12px 0" }}>
              <input type="checkbox" checked={delAsk.delFile}
                onChange={(e) => setDelAsk({ ...delAsk, delFile: (e.target as HTMLInputElement).checked })} />
              {t("删除本地文件", "Delete local files")}
            </label>
            <div class="modal-foot">
              <button class="btn" onClick={() => setDelAsk(null)}>{t("取消", "Cancel")}</button>
              <button class="btn danger" onClick={() => removeMany(delAsk.keys, delAsk.delFile)}>{t("删除", "Delete")}</button>
            </div>
          </div>
        </div>
      )}

      {resolve && (
        <ResolveModal
          info={resolve}
          onClose={() => {
            if (resolve.phase === "work") api.removeTorrent(resolve.id, false).catch(() => {});
            setResolve(null);
          }}
        />
      )}

      {pickFiles && (
        <FilePickModal t={pickFiles} onClose={() => setPickFiles(null)} />
      )}
    </div>
  );
}

function TaskRow(props: {
  t: Row; selected: boolean;
  onSelect: (e: MouseEvent) => void;
  onTogglePick: () => void;
  onOpenDetail: () => void;
  onOpenFile: () => void;
  onMenu: (x: number, y: number) => void;
}) {
  const { t } = props;
  const p = t.size ? t.done / t.size : 0;
  const meta = STATE_META[t.state] || { label: t.state, cls: "" };
  const checking = t.kind === "bt" && t.phase === "initializing";
  const statusCell = checking
    ? tx("校验中", "Verifying")
    : ["failed", "queued", "canceled", "resolving", "awaiting_selection", "seeding"].includes(t.state)
      ? meta.label
      : `${Math.floor(p * 100)}%`;
  const showProg = t.kind === "bt" && t.state !== "awaiting_selection"
    && (checking || ["active", "paused", "queued", "seeding", "failed"].includes(t.state));
  const progMeta = checking
    ? tx(
        `${fmtBytes(t.done)}${t.size ? " / " + fmtBytes(t.size) : ""} · 校验本地文件`,
        `${fmtBytes(t.done)}${t.size ? " / " + fmtBytes(t.size) : ""} · Verifying local files`,
      )
    : [
        t.size != null ? `${fmtBytes(t.done)} / ${fmtBytes(t.size)}` : fmtBytes(t.done),
        fmtRate(t.speed),
        t.kind === "bt"
          ? tx(
              `${t.peers} 已连接${t.connecting ? ` · ${t.connecting} 连接中` : ""}${!t.peers && t.seen ? ` · ${t.seen} 已知` : ""}`,
              `${t.peers} connected${t.connecting ? ` · ${t.connecting} connecting` : ""}${!t.peers && t.seen ? ` · ${t.seen} known` : ""}`,
            )
          : "",
        t.up_speed > 0 ? "↑ " + fmtRate(t.up_speed) : "",
      ].filter(Boolean).join(" · ");

  const onDblClick = () => {
    if (t.state === "completed" || t.state === "seeding") props.onOpenFile();
    else props.onOpenDetail();
  };

  return (
    <div class={"trow" + (props.selected ? " selected" : "")}
      onClick={(e) => props.onSelect(e)}
      onDblClick={onDblClick}
      onContextMenu={(e) => {
        e.preventDefault();
        props.onMenu(Math.min(e.clientX, window.innerWidth - 190), Math.min(e.clientY, window.innerHeight - 300));
      }}>
      <div class="cell-chk" onClick={(e) => { e.stopPropagation(); props.onTogglePick(); }}>
        <button type="button" class={"chk" + (props.selected ? " on" : "")} />
      </div>
      <div class="cell-name">
        <span class="mini-ico"><TypeIcon type={fileType(t.name)} size={16} /></span>
        <div class="nm-wrap">
          <div class="nm-line">
            <span class="nm">{t.name || t.url}</span>
            {t.kind === "bt" && <span class="bt-tag">BT</span>}
            {(t.state === "completed" || t.state === "seeding") && (
              <button type="button" class="row-open" title={tx("打开", "Open")}
                onClick={(e) => { e.stopPropagation(); props.onOpenFile(); }}>
                <IcoOpen size={14} />
              </button>
            )}
          </div>
          {showProg && (
            <div class="nm-prog">
              <div class="nm-prog-bar"><div style={{ width: `${Math.min(100, p * 100).toFixed(1)}%` }}></div></div>
              <span class="nm-prog-meta">{progMeta}</span>
            </div>
          )}
        </div>
      </div>
      <div class="num">{t.size ? fmtBytes(t.size) : t.state === "completed" ? fmtBytes(t.done) : "—"}</div>
      <div class="num">{statusCell}</div>
      <div class="num">{fmtTime(t.created_at)}</div>
      <div class="num">{fmtTime(t.completed_at ?? t.created_at)}</div>
    </div>
  );
}

function ContextMenu(props: {
  menu: CtxMenu; t: Row;
  onClose: () => void;
  onOpenFile: () => void;
  onDetail: () => void;
  onDelete: () => void;
}) {
  const { t } = props;
  const running = ["active", "probing", "resolving", "queued", "seeding"].includes(t.state);
  const pause = () => (t.kind === "bt" ? api.pauseTorrent(t.id) : api.pauseTask(t.id)).catch(console.error);
  const resume = () => (t.kind === "bt" ? api.resumeTorrent(t.id) : api.resumeTask(t.id)).catch(console.error);
  const item = (label: string, action: () => void, cls = "") => (
    <div class={"ctx-item " + cls} onClick={() => { props.onClose(); action(); }}>{label}</div>
  );
  return (
    <div class="ctx-menu" style={{ left: props.menu.x, top: props.menu.y }}>
      {t.state !== "completed" && (running ? item(tx("暂停", "Pause"), pause) : item(tx("继续", "Resume"), resume))}
      {t.kind === "http" && item(tx("重新下载", "Redownload"), () => api.redownloadTask(t.id).catch(console.error))}
      <div class="ctx-sep"></div>
      {item(tx("打开文件夹", "Open folder"), () => api.openPath(rowFolder(t), t.dir))}
      {(t.state === "completed" || t.state === "seeding") && item(tx("打开", "Open"), props.onOpenFile)}
      {item(tx("复制链接地址", "Copy link"), () => navigator.clipboard.writeText(t.url))}
      {item(tx("进度", "Details"), props.onDetail)}
      {t.kind === "http" && t.state !== "completed" && t.state !== "canceled" && (
        item(tx("取消", "Cancel"), () => api.cancelTask(t.id).catch(console.error))
      )}
      <div class="ctx-sep"></div>
      {item(tx("删除", "Delete"), props.onDelete, "danger")}
    </div>
  );
}

function DetailPane(props: { t: TaskInfo; collapsed: boolean; onToggle: () => void; onCancel: () => void; onHide: () => void }) {
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
    ? tx("未知", "Unknown")
    : t.resumable ? tx("是", "Yes") : tx("否", "No");

  return (
    <aside class={"detail" + (props.collapsed ? " collapsed" : "")}>
      <div class="detail-inner">
      <div class="detail-top">
        <div class="prog-title">{pctLabel} {t.name || t.url}</div>
        <button class="icon-btn" title={tx("关闭", "Close")} onClick={props.onHide}><IcoX size={16} /></button>
      </div>
      <div class="prog-tabs">
        {(["download", "options", "connections"] as const).map((k) => (
          <button class={"prog-tab" + (tab === k ? " on" : "")} onClick={() => setTab(k)}>
            {k === "download" ? tx("下载", "Download") : k === "options" ? tx("选项", "Options") : tx("连接", "Connections")}
          </button>
        ))}
      </div>
        <div class="prog-body">
          {tab === "download" && (
            <>
              {line("URL", t.url)}
              {line(tx("状态", "Status"), STATE_META[t.state].label + (t.error ? ` — ${t.error}` : ""))}
              {line(tx("文件大小", "File size"), t.size ? fmtBytes(t.size) : tx("未知", "Unknown"))}
              {line(tx("已下载", "Downloaded"), `${fmtBytes(t.done)} ( ${pctLabel} )`)}
              {line(tx("带宽", "Bandwidth"), t.state === "active" ? fmtSpeed(t.speed) : "0 B/s")}
              {line(tx("剩余时间", "Remaining"), t.state === "active" ? fmtEta(t) : tx("未知", "Unknown"))}
              {line(tx("可续传", "Resumable"), resumable)}
              {line(tx("HTTP 探测", "HTTP probe"), t.http_status ? String(t.http_status) : "—")}
              {line(tx("Range 已忽略", "Range ignored"), t.range_ignored ? tx("是", "Yes") : tx("否", "No"))}
              <div class="prog-bar"><div style={{ width: `${(p * 100).toFixed(2)}%` }}></div></div>
              <div class="seg-title" style={{ margin: 0 }}>
                <span>{tx("分段", "Segments")}: {t.segments.length}</span>
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
                <span class="k">{tx("连接数", "Connections")}</span>
                <span class="v">
                  <NumStep value={conn} min={1} max={MAX_CONN}
                    onChange={(n) => api.setConnections(t.id, n).catch(console.error)} />
                </span>
              </div>
              <div class="settings-hint">
                {isRunning(t)
                  ? tx("修改会在暂停后生效。", "Changes apply after pausing.")
                  : tx("立即重切剩余分段。", "Remaining segments update immediately.")}
              </div>
            </>
          )}
          {tab === "connections" && (
            <>
              {t.segments.length === 0 && <div class="settings-hint">{tx("尚未开始分段", "No segments yet")}</div>}
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
              {running ? tx("暂停", "Pause") : t.state === "failed" ? tx("重试", "Retry") : tx("继续", "Resume")}
            </button>
          )}
          {t.state !== "completed" && t.state !== "canceled" && (
            <button class="btn danger" onClick={props.onCancel}>{tx("取消", "Cancel")}</button>
          )}
        </div>
      </div>
    </aside>
  );
}

function UpdateBar() {
  const [st, setSt] = useState<api.UpdateStatus | null>(null);
  useEffect(() => {
    let on = true;
    const tick = () => { api.updateStatus().then((s) => on && setSt(s)); };
    tick();
    const id = setInterval(tick, 1500);
    return () => { on = false; clearInterval(id); };
  }, []);
  if (!st) return null;
  // 任务列表顶栏只报可用/下载进度, error 留给设置页, 避免两处同时刷失败文案.
  if (st.phase === "error") return null;
  const live = st.phase === "downloading" || st.phase === "waiting" || st.phase === "installing";
  if (!live && st.phase !== "available") return null;
  return (
    <div class="update-bar">
      {updatePhaseText(st)}
      {st.phase === "available" && (
        <button class="btn" onClick={() => api.checkNow().then(setSt)}>{t("立即更新", "Update now")}</button>
      )}
    </div>
  );
}
