import type { TaskInfo } from "./api";

const KB = 1024, MB = KB * 1024, GB = MB * 1024;

export function fmtBytes(n: number): string {
  if (n >= GB) return (n / GB).toFixed(2) + " GB";
  if (n >= MB) return (n / MB).toFixed(1) + " MB";
  if (n >= KB) return (n / KB).toFixed(0) + " KB";
  return n + " B";
}

export function fmtSpeed(bps: number): string {
  if (bps <= 0) return "—";
  if (bps >= MB) return (bps / MB).toFixed(1) + " MB/s";
  return (bps / KB).toFixed(0) + " KB/s";
}

export function fmtEtaBy(size: number | null, done: number, speed: number): string {
  if (!size || speed <= 0) return "—";
  const s = Math.ceil((size - done) / speed);
  if (s < 60) return s + " 秒";
  if (s < 3600) return Math.floor(s / 60) + " 分 " + (s % 60) + " 秒";
  return Math.floor(s / 3600) + " 小时 " + Math.floor((s % 3600) / 60) + " 分";
}

export function fmtEta(t: TaskInfo): string {
  return fmtEtaBy(t.size, t.done, t.speed);
}

export function fmtRate(bps: number): string {
  if (bps <= 0) return "0 B/s";
  return fmtSpeed(bps);
}

export function fmtTime(ts: number | null): string {
  if (!ts) return "";
  const d = new Date(ts * 1000);
  const today = new Date();
  const sameDay = d.toDateString() === today.toDateString();
  const hm = `${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}`;
  if (sameDay) return `今天 ${hm}`;
  return `${d.getMonth() + 1}/${d.getDate()} ${hm}`;
}

export function pct(t: TaskInfo): number {
  return t.size ? t.done / t.size : 0;
}

export type FileType = "video" | "audio" | "image" | "doc" | "archive" | "app" | "other";

export const FILE_TYPE_ORDER: FileType[] = [
  "video", "audio", "image", "doc", "archive", "app", "other",
];

/// 服务端不存类型, 客户端按扩展名归类 (纯展示用途)
export function fileType(name: string): FileType {
  const ext = name.split(".").pop()?.toLowerCase() || "";
  if (["mp4", "mkv", "mov", "avi", "webm", "flv", "ts", "m4v", "wmv", "rmvb"].includes(ext)) return "video";
  if (["mp3", "flac", "aac", "wav", "ogg", "m4a", "wma", "ape"].includes(ext)) return "audio";
  if (["jpg", "jpeg", "png", "gif", "webp", "bmp", "svg", "heic", "avif"].includes(ext)) return "image";
  if (["pdf", "doc", "docx", "epub", "txt", "md", "ppt", "pptx", "xls", "xlsx", "nfo", "md5", "sha1", "sha256"].includes(ext)) return "doc";
  if (["zip", "7z", "rar", "tar", "gz", "xz", "bz2", "ipsw", "iso", "img"].includes(ext)) return "archive";
  if (["exe", "dmg", "apk", "pkg", "msi", "deb", "rpm", "appimage"].includes(ext)) return "app";
  return "other";
}

export const TYPE_LABEL: Record<FileType, string> = {
  video: "视频",
  audio: "音频",
  image: "图片",
  doc: "文档",
  archive: "压缩包",
  app: "软件",
  other: "其他",
};

export const STATE_META: Record<string, { label: string; cls: string }> = {
  queued: { label: "等待中", cls: "" },
  probing: { label: "连接中", cls: "active" },
  active: { label: "下载中", cls: "active" },
  paused: { label: "已暂停", cls: "" },
  completed: { label: "已完成", cls: "completed" },
  failed: { label: "失败", cls: "failed" },
  canceled: { label: "已取消", cls: "" },
  resolving: { label: "解析中", cls: "active" },
  awaiting_selection: { label: "待选文件", cls: "" },
  seeding: { label: "做种中", cls: "completed" },
};
