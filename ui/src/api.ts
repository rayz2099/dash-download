// localhost API 客户端: UI 与扩展共用同一套 REST/WS (ADR 0003)
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";

export type TaskState =
  | "queued" | "probing" | "active" | "paused" | "completed" | "failed" | "canceled";

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
  | { type: "snapshot"; tasks: TaskInfo[] }
  | { type: "task_added"; task: TaskInfo }
  | { type: "task_updated"; task: TaskInfo }
  | { type: "task_removed"; id: number }
  | { type: "progress"; tasks: TaskProgress[] };

export interface Boot {
  port: number;
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
  if (!boot) throw new Error("boot 未初始化");
  return boot;
}

async function req<T>(method: string, path: string, body?: unknown): Promise<T> {
  const b = getBoot();
  const resp = await fetch(`http://127.0.0.1:${b.port}${path}`, {
    method,
    headers: {
      "x-dd-client": "ui",
      ...(body !== undefined ? { "content-type": "application/json" } : {}),
    },
    body: body !== undefined ? JSON.stringify(body) : undefined,
  });
  if (!resp.ok) {
    const err = await resp.json().catch(() => ({ error: resp.statusText }));
    throw new Error((err as { error?: string }).error || `HTTP ${resp.status}`);
  }
  if (resp.status === 204) return undefined as T;
  return resp.json() as Promise<T>;
}

export const addTask = (r: AddTaskReq) => req<TaskInfo>("POST", "/api/tasks", r);
export const pauseTask = (id: number) => req<void>("POST", `/api/tasks/${id}/pause`);
export const resumeTask = (id: number) => req<void>("POST", `/api/tasks/${id}/resume`);
export const cancelTask = (id: number) => req<void>("POST", `/api/tasks/${id}/cancel`);
export const redownloadTask = (id: number) => req<void>("POST", `/api/tasks/${id}/redownload`);
export const setConnections = (id: number, n: number) =>
  req<void>("POST", `/api/tasks/${id}/connections`, { n });
export const removeTask = (id: number, deleteFile = true) =>
  req<void>("DELETE", `/api/tasks/${id}?delete_file=${deleteFile}`);
export const pauseAll = () => req<void>("POST", "/api/pause-all");
export const resumeAll = () => req<void>("POST", "/api/resume-all");
export const revealFile = (path: string) => invoke("reveal", { path });
export const openPath = (path: string) => invoke("open_path", { path });

/// WS 事件流: 断线 2s 自动重连, 重连后服务端会重发快照对齐状态
export function connectEvents(onEvent: (ev: EngineEvent) => void): () => void {
  let ws: WebSocket | null = null;
  let closed = false;
  const connect = () => {
    if (closed) return;
    const b = getBoot();
    ws = new WebSocket(`ws://127.0.0.1:${b.port}/api/ws`);
    ws.onmessage = (e) => {
      try {
        onEvent(JSON.parse(e.data as string) as EngineEvent);
      } catch {
        /* 忽略坏帧 */
      }
    };
    ws.onclose = () => {
      if (!closed) setTimeout(connect, 2000);
    };
  };
  connect();
  return () => {
    closed = true;
    ws?.close();
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
}

export const MAX_CONN = 128;

export const getSettings = () => req<EngineSettings>("GET", "/api/settings");
export const putSettings = (s: EngineSettings) =>
  req<EngineSettings>("PUT", "/api/settings", s);

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
  req<ProxyProbe>("POST", "/api/proxy-test", { url, proxy });
