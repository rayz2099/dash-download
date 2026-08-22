// 模拟数据与格式化工具
const KB = 1024, MB = KB * 1024, GB = MB * 1024;

function fmtBytes(n) {
  if (n >= GB) return (n / GB).toFixed(2) + " GB";
  if (n >= MB) return (n / MB).toFixed(1) + " MB";
  if (n >= KB) return (n / KB).toFixed(0) + " KB";
  return n + " B";
}
function fmtSpeed(bps) {
  if (bps <= 0) return "—";
  if (bps >= MB) return (bps / MB).toFixed(1) + " MB/s";
  return (bps / KB).toFixed(0) + " KB/s";
}
function fmtEta(task) {
  if (task.speed <= 0) return "—";
  const s = Math.ceil((task.size - task.done) / task.speed);
  if (s < 60) return s + " 秒";
  if (s < 3600) return Math.floor(s / 60) + " 分 " + (s % 60) + " 秒";
  return Math.floor(s / 3600) + " 小时 " + Math.floor((s % 3600) / 60) + " 分";
}
function pct(task) { return task.size ? task.done / task.size : 0; }

// 生成 n 个 Segment, 各段完成度围绕整体进度做扰动, 模拟真实分段差异
function makeSegments(size, n, progress, seed = 1) {
  const segs = [];
  const per = Math.floor(size / n);
  let rnd = seed;
  const rand = () => { rnd = (rnd * 9301 + 49297) % 233280; return rnd / 233280; };
  for (let i = 0; i < n; i++) {
    const start = i * per;
    const end = i === n - 1 ? size : start + per;
    const jitter = Math.min(1, Math.max(0, progress + (rand() - 0.5) * 0.5));
    segs.push({ start, end, done: Math.floor((end - start) * jitter) });
  }
  return segs;
}
const segDone = (segs) => segs.reduce((a, s) => a + s.done, 0);

const TYPE_META = {
  video:   { label: "视频",   icon: "IcoFilm" },
  doc:     { label: "文档",   icon: "IcoDoc" },
  archive: { label: "压缩包", icon: "IcoBox" },
  app:     { label: "软件",   icon: "IcoApp" },
  audio:   { label: "音频",   icon: "IcoMusic" },
  other:   { label: "其他",   icon: "IcoFile" },
};
const STATE_META = {
  active:    { label: "下载中", cls: "active" },
  paused:    { label: "已暂停", cls: "" },
  queued:    { label: "等待中", cls: "" },
  completed: { label: "已完成", cls: "completed" },
  failed:    { label: "失败",   cls: "failed" },
};

function task(o) {
  const segs = o.segments != null
    ? o.segments
    : makeSegments(o.size, o.segN || 8, o.progress || 0, o.id.charCodeAt(0));
  return {
    dir: "~/Downloads", referer: "", speed: 0, error: "",
    ...o,
    segments: segs,
    done: o.state === "completed" ? o.size : segDone(segs),
  };
}

const INITIAL_TASKS = [
  task({
    id: "t1", name: "QQ9.7.17.29225.exe", type: "app", size: 209 * MB,
    state: "active", progress: 0.46, speed: 11.8 * MB,
    url: "https://dldir1.qq.com/qqfile/qq/PCQQ9.7.17/QQ9.7.17.29225.exe",
    referer: "https://im.qq.com/pcqq",
  }),
  task({
    id: "t2", name: "iPhone_4.7_12.1.4_16D57_Restore.ipsw", type: "archive", size: 2.91 * GB,
    state: "active", progress: 0.13, speed: 6.2 * MB,
    url: "http://updates-http.cdn-apple.com/2019WinterFCS/fullrestores/041-39257/32129B6C-292C-11E9-9E72-4511412B0A59/iPhone_4.7_12.1.4_16D57_Restore.ipsw",
  }),
  task({
    id: "t3", name: "xuexi_android_10002068.apk", type: "app", size: 293 * MB,
    state: "paused", progress: 0.62, segN: 8,
    url: "https://wirelesscdn-download.xuexi.cn/publish/xuexi_android/latest/xuexi_android_10002068.apk",
  }),
  task({
    id: "t4", name: "sgame_8.2.1.9.apk", type: "app", size: 1.92 * GB,
    state: "queued", progress: 0, speed: 0,
    url: "https://dlied4.myapp.com/myapp/1104466820/cos.release-40109/10040714_com.tencent.tmgp.sgame_a2480356_8.2.1.9_F0BvnI.apk",
  }),
  task({
    id: "t5", name: "zju-speedtest-1000M.bin", type: "other", size: 1000 * MB,
    state: "failed", progress: 0.08, error: "连接超时: speedtest.zju.edu.cn 无响应 (重试 5 次后放弃)",
    url: "http://speedtest.zju.edu.cn/1000M",
  }),
  task({
    id: "t6", name: "WWDC25_Platforms_State_of_the_Union.mp4", type: "video", size: 683 * MB,
    state: "completed", completedAt: "今天 10:24",
    url: "https://devstreaming-cdn.apple.com/videos/wwdc/2025/sotu/platforms_sotu_hd.mp4",
  }),
  task({
    id: "t7", name: "Designing Data-Intensive Applications 2nd.pdf", type: "doc", size: 28.4 * MB,
    state: "completed", completedAt: "今天 09:41",
    url: "https://cdn.oreillystatic.com/books/ddia-2nd-preview.pdf",
  }),
  task({
    id: "t8", name: "Bach - Goldberg Variations (Gould 1981) [FLAC].zip", type: "audio", size: 412 * MB,
    state: "completed", completedAt: "昨天 22:03",
    url: "https://archive.org/download/goldberg-gould-1981/goldberg_flac.zip",
  }),
];

const SIDE_STATES = [
  { key: "all",       label: "全部",   icon: "IcoQueue" },
  { key: "active",    label: "下载中", icon: "IcoDown" },
  { key: "queued",    label: "等待中", icon: "IcoQueue" },
  { key: "completed", label: "已完成", icon: "IcoCheck" },
  { key: "failed",    label: "失败",   icon: "IcoAlert" },
];

Object.assign(window, {
  KB, MB, GB, fmtBytes, fmtSpeed, fmtEta, pct, makeSegments, segDone, task,
  TYPE_META, STATE_META, INITIAL_TASKS, SIDE_STATES,
});
