// UI 控制面: invoke ctl, 事件走 tauri engine channel. 不再打 loopback HTTP.
import { t } from "./i18n";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";

export type TaskState =
  | "queued" | "probing" | "active" | "paused" | "completed" | "failed" | "canceled";

export type TorrentState =
  | "resolving" | "awaiting_selection" | "queued" | "active" | "seeding" | "paused" | "failed";

export interface TorrentPeer {
  addr: string;
  client: string;
  state: string;
  down: number;
  up: number;
  kind: string;
  chunks?: number;
  pieces?: number;
  piece_ms?: number;
  conn_ms?: number;
  attempts?: number;
  errors?: number;
  incoming?: boolean;
}

export interface TorrentFile {
  idx: number;
  path: string;
  size: number;
  selected: boolean;
}

export interface TorrentInfo {
  id: number;
  infohash: string;
  source: string;
  name: string;
  dir: string;
  state: TorrentState;
  done: number;
  size: number | null;
  speed: number;
  up_speed: number;
  error: string;
  files: TorrentFile[];
  peers: number;
  seen?: number;
  connecting?: number;
  peer_list?: TorrentPeer[];
  phase?: string;
  bt_direct: boolean;
  created_at: number;
  completed_at: number | null;
}

export interface TorrentProgress {
  id: number;
  done: number;
  speed: number;
  up_speed: number;
  peers: number;
  seen?: number;
  connecting?: number;
  phase?: string;
  peer_list?: TorrentPeer[];
}

export interface SegmentInfo {
  idx: number;
  start: number;
  end: number;
  done: number;
}

export interface TaskInfo {
  id: number;
  url: string;
  final_url: string;
  name: string;
  dir: string;
  size: number | null;
  resumable: boolean;
  http_status: number;
  range_ignored: boolean;
  state: TaskState;
  done: number;
  speed: number;
  error: string;
  segments: SegmentInfo[];
  max_segments: number;
  created_at: number;
  completed_at: number | null;
}

export interface TaskProgress {
  id: number;
  done: number;
  speed: number;
  seg_done: number[];
}

export type EngineEvent =
  | { type: "snapshot"; tasks: TaskInfo[]; torrents?: TorrentInfo[] }
  | { type: "task_added"; task: TaskInfo }
  | { type: "task_updated"; task: TaskInfo }
  | { type: "task_removed"; id: number }
  | { type: "progress"; tasks: TaskProgress[] }
  | { type: "torrent_added"; torrent: TorrentInfo }
  | { type: "torrent_updated"; torrent: TorrentInfo }
  | { type: "torrent_removed"; id: number }
  | { type: "torrent_progress"; torrents: TorrentProgress[] }
  | { type: "resolving"; torrent: TorrentInfo }
  | { type: "resolve_failed"; id: number; source: string; error: string };

export interface Boot {
  default_dir: string;
  version: string;
}

export interface AddTaskReq {
  url: string;
  dir?: string;
  name?: string;
  segments?: number;
  queue_only?: boolean;
  headers?: [string, string][];
}

let boot: Boot | null = null;

export async function init(): Promise<Boot> {
  boot = await invoke<Boot>("bootstrap");
  return boot;
}

export function getBoot(): Boot {
  if (!boot) throw new Error(t("boot 未初始化", "Boot data is not initialized"));
  return boot;
}

async function ctl<T>(op: string, extra: Record<string, unknown> = {}): Promise<T> {
  return invoke<T>("ctl", { req: { op, ...extra } });
}

export const addTask = (r: AddTaskReq) => ctl<TaskInfo>("add_task", { ...r });
export const pauseTask = (id: number) => ctl<void>("pause_task", { id });
export const resumeTask = (id: number) => ctl<void>("resume_task", { id });
export const cancelTask = (id: number) => ctl<void>("cancel_task", { id });
export const redownloadTask = (id: number) => ctl<void>("redownload_task", { id });
export const setConnections = (id: number, n: number) =>
  ctl<void>("set_connections", { id, n });
export const removeTask = (id: number, deleteFile = true) =>
  ctl<void>("remove_task", { id, delete_file: deleteFile });
export const pauseAll = () => ctl<void>("pause_all");
export const resumeAll = () => ctl<void>("resume_all");

export const addTorrent = (r: {
  magnet?: string;
  torrent_b64?: string;
  torrent_url?: string;
  dir?: string;
  headers?: [string, string][];
}) => ctl<TorrentInfo>("add_torrent", { ...r });
export const pauseTorrent = (id: number) => ctl<void>("pause_torrent", { id });
export const resumeTorrent = (id: number) => ctl<void>("resume_torrent", { id });
export const selectTorrentFiles = (id: number, selected: number[]) =>
  ctl<TorrentInfo>("select_files", { id, selected });
export const removeTorrent = (id: number, deleteFile = true) =>
  ctl<void>("remove_torrent", { id, delete_file: deleteFile });
export const revealFile = (path: string, fallback?: string) =>
  invoke("reveal", { path, fallback: fallback ?? null });
export const openPath = (path: string, fallback?: string) =>
  invoke("open_path", { path, fallback: fallback ?? null });

/// 引擎事件: 先拉快照再订 tauri event. 断线由进程生命周期决定, 不再重连 loopback WS.
export function connectEvents(onEvent: (ev: EngineEvent) => void): () => void {
  let closed = false;
  let unlisten: (() => void) | undefined;
  void (async () => {
    try {
      const tasks = await ctl<TaskInfo[]>("list_tasks");
      const torrents = await ctl<TorrentInfo[]>("list_torrents");
      if (!closed) onEvent({ type: "snapshot", tasks, torrents });
      unlisten = await listen<EngineEvent>("engine", (e) => {
        if (!closed) onEvent(e.payload);
      });
    } catch {
      /* bootstrap 失败由 init 表面 */
    }
  })();
  return () => {
    closed = true;
    unlisten?.();
  };
}

export type UpdatePhase =
  | "idle" | "checking" | "up_to_date" | "available"
  | "downloading" | "waiting" | "installing" | "error";

export interface UpdateStatus {
  auto_update: boolean;
  current: string;
  latest: string | null;
  phase: UpdatePhase;
  done: number;
  total: number | null;
  error: string | null;
}

export const updateStatus = () => invoke<UpdateStatus>("update_status");
export const checkUpdate = () => invoke<UpdateStatus>("check_update");
export const checkNow = () => invoke<UpdateStatus>("check_now");
export const setAutoUpdate = (enabled: boolean) =>
  invoke<UpdateStatus>("set_auto_update", { enabled });
export const autoStartOn = () => invoke<boolean>("auto_start_on");
export const setAutoStart = (enabled: boolean) =>
  invoke<boolean>("set_auto_start", { enabled });

export type ProxyKind = "direct" | "no_proxy" | "http" | "socks5";

export interface ProxyCfg {
  kind: ProxyKind;
  host: string;
  port: number;
  auth: boolean;
  user: string;
  pass: string;
  pass_set?: boolean;
}

export interface EngineSettings {
  default_dir: string;
  max_concurrent: number;
  max_segments: number;
  proxy: ProxyCfg;
  max_bt_active: number;
  max_bt_seed: number;
  listen_port: number;
  upnp: boolean;
  extra_trackers: boolean;
  resolve_secs: number;
  p2p: boolean;
  bt_direct?: boolean;
}

export const MAX_CONN = 128;

export const getSettings = () => ctl<EngineSettings>("get_settings");
export const putSettings = (s: EngineSettings) =>
  ctl<EngineSettings>("put_settings", { ...s });

/// 系统目录面板. 取消返回 null, 不要把空路径写进 prefs.
export async function pickDir(current: string): Promise<string | null> {
  const sel = await open({ directory: true, multiple: false, defaultPath: current });
  if (typeof sel === "string" && sel.length > 0) return sel;
  return null;
}

export interface ProxyProbe {
  status: number;
  ms: number;
  final_url: string;
}

export const testProxy = (url: string, proxy: ProxyCfg) =>
  ctl<ProxyProbe>("test_proxy", { url, proxy });
