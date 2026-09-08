import type { TaskInfo } from "./api";
import { t } from "./i18n";

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
  if (s < 60) return t(`${s} 秒`, `${s} sec`);
  if (s < 3600) return t(`${Math.floor(s / 60)} 分 ${s % 60} 秒`, `${Math.floor(s / 60)} min ${s % 60} sec`);
  return t(
    `${Math.floor(s / 3600)} 小时 ${Math.floor((s % 3600) / 60)} 分`,
    `${Math.floor(s / 3600)} hr ${Math.floor((s % 3600) / 60)} min`,
  );
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
  if (sameDay) return t(`今天 ${hm}`, `Today ${hm}`);
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
  video: t("视频", "Video"),
  audio: t("音频", "Audio"),
  image: t("图片", "Images"),
  doc: t("文档", "Documents"),
  archive: t("压缩包", "Archives"),
  app: t("软件", "Apps"),
  other: t("其他", "Other"),
};

export const STATE_META: Record<string, { label: string; cls: string }> = {
  queued: { label: t("等待中", "Queued"), cls: "" },
  probing: { label: t("连接中", "Connecting"), cls: "active" },
  active: { label: t("下载中", "Downloading"), cls: "active" },
  paused: { label: t("已暂停", "Paused"), cls: "" },
  completed: { label: t("已完成", "Completed"), cls: "completed" },
  failed: { label: t("失败", "Failed"), cls: "failed" },
  canceled: { label: t("已取消", "Canceled"), cls: "" },
  resolving: { label: t("解析中", "Resolving"), cls: "active" },
  awaiting_selection: { label: t("待选文件", "Select files"), cls: "" },
  seeding: { label: t("做种中", "Seeding"), cls: "completed" },
};
