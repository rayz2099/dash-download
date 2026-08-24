import { useEffect, useMemo, useState } from "preact/hooks";
import * as api from "./api";
import type { TorrentInfo, TorrentPeer } from "./api";
import { MAX_CONN } from "./api";
import {
  FILE_TYPE_ORDER, fileType, fmtBytes, fmtEtaBy, fmtRate, STATE_META, TYPE_LABEL,
} from "./util";
import type { FileType } from "./util";
import { DirPick, NumStep } from "./fields";
import { IcoAlert, IcoDown, IcoX, TypeIcon } from "./icons";

export function ResolveModal(props: {
  info: { id: number; source: string; name: string; phase: "work" | "fail"; error: string };
  onClose: () => void;
}) {
  const { info } = props;
  const hash = info.source.replace(/^magnet:\?xt=urn:btih:/i, "").slice(0, 40);
  return (
    <div class="overlay" onClick={props.onClose}>
      <div class="modal resolve-modal" onClick={(e) => e.stopPropagation()}>
        <div class="resolve-head">
          <h2 class="page-title" style={{ fontSize: 16, margin: 0 }}>
            {info.phase === "fail" ? "解析失败" : "解析磁力"}
          </h2>
          <button class="icon-btn" title="关闭" onClick={props.onClose}><IcoX size={16} /></button>
        </div>
        {info.phase === "work" ? (
          <div class="resolve-body">
            <div class="spin" />
            <div>
              <div>正在拉取文件列表…</div>
              <div class="page-sub" style={{ marginTop: 6 }}>{info.name || hash}</div>
            </div>
          </div>
        ) : (
          <div class="resolve-body">
            <IcoAlert size={22} />
            <div>
              <div>解析失败</div>
              <div class="page-sub" style={{ marginTop: 6 }}>{info.error || "DHT / Tracker 无响应"}</div>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}

/// 迅雷式选文件: 分类跟侧栏同一套, 点类型过滤列表, 全选/取消只动当前可见项.
export function FilePickModal(props: { t: TorrentInfo; onClose: () => void }) {
  const files = props.t.files;
  const [sel, setSel] = useState<Set<number>>(() => new Set(files.filter((f) => f.selected).map((f) => f.idx)));
  const [cat, setCat] = useState<FileType | "all">("all");
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState("");

  const counts = useMemo(() => {
    const m = new Map<FileType, number>();
    for (const f of files) {
      const t = fileType(f.path);
      m.set(t, (m.get(t) || 0) + 1);
    }
    return m;
  }, [files]);

  const visible = useMemo(
    () => cat === "all" ? files : files.filter((f) => fileType(f.path) === cat),
    [files, cat],
  );

  const visOn = visible.filter((f) => sel.has(f.idx)).length;
  const allOn = visible.length > 0 && visOn === visible.length;
  const someOn = visOn > 0 && !allOn;
  const selSize = files.reduce((a, f) => a + (sel.has(f.idx) ? f.size : 0), 0);

  const toggle = (idx: number) => {
    setSel((p) => {
      const n = new Set(p);
      if (n.has(idx)) n.delete(idx); else n.add(idx);
      return n;
    });
  };
  const selectVisible = (on: boolean) => {
    setSel((p) => {
      const n = new Set(p);
      for (const f of visible) {
        if (on) n.add(f.idx); else n.delete(f.idx);
      }
      return n;
    });
  };

  const go = async () => {
    if (!sel.size) return;
    setBusy(true);
    try {
      await api.selectTorrentFiles(props.t.id, [...sel]);
      props.onClose();
    } catch (e) {
      setErr(String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div class="overlay" onClick={props.onClose}>
      <div class="modal pick-modal" onClick={(e) => e.stopPropagation()}>
        <div class="pick-head">
          <h2 class="page-title" style={{ fontSize: 16, margin: 0 }}>{props.t.name || "选择文件"}</h2>
          <p class="page-sub" style={{ margin: 0 }}>
            {files.length} 个文件 · 已选 {sel.size} · {fmtBytes(selSize)}
          </p>
        </div>
        <div class="pick-bar">
          <label class="pick-all">
            <input type="checkbox"
              checked={allOn}
              ref={(el) => { if (el) el.indeterminate = someOn; }}
              onChange={() => selectVisible(!allOn)} />
            全选
          </label>
          <button class="pick-link" type="button" onClick={() => selectVisible(false)} disabled={!visOn}>全部取消</button>
        </div>
        <div class="pick-cats">
          <button type="button" class={"pick-chip" + (cat === "all" ? " on" : "")} onClick={() => setCat("all")}>
            全部 <em>{files.length}</em>
          </button>
          {FILE_TYPE_ORDER.map((t) => {
            const n = counts.get(t) || 0;
            return (
              <button key={t} type="button" disabled={!n}
                class={"pick-chip" + (cat === t ? " on" : "")}
                onClick={() => setCat(cat === t ? "all" : t)}>
                <TypeIcon type={t} size={13} />
                {TYPE_LABEL[t]}
                <em>{n || ""}</em>
              </button>
            );
          })}
        </div>
        <div class="pick-list">
          {visible.map((f) => (
            <label key={f.idx} class={"pick-row" + (sel.has(f.idx) ? " on" : "")}>
              <input type="checkbox" checked={sel.has(f.idx)} onChange={() => toggle(f.idx)} />
              <span class="pick-ico"><TypeIcon type={fileType(f.path)} size={14} /></span>
              <span class="pick-name" title={f.path}>{f.path}</span>
              <span class="pick-size">{fmtBytes(f.size)}</span>
            </label>
          ))}
        </div>
        {err && <div class="detail-err">{err}</div>}
        <div class="modal-foot pick-foot">
          <span class="page-sub">已选 {sel.size}/{files.length} · {fmtBytes(selSize)}</span>
          <span class="pick-bar-gap" />
          <button class="btn" onClick={props.onClose}>取消</button>
          <button class="btn primary" disabled={!sel.size || busy} onClick={go}>开始下载</button>
        </div>
      </div>
    </div>
  );
}

function magnetHash(u: string): string {
  const m = /xt=urn:btih:([a-fA-F0-9]{40}|[a-zA-Z2-7]{32})/i.exec(u);
  return m ? m[1] : "";
}

export function NewTaskPage(props: {
  defaultDir: string; defaultConn: number; onClose: () => void;
  onCreated: (t?: api.TorrentInfo) => void;
}) {
  const [url, setUrl] = useState("");
  const [dir, setDir] = useState(props.defaultDir);
  const [conn, setConn] = useState(props.defaultConn);
  const [showAdv, setShowAdv] = useState(false);
  const [headers, setHeaders] = useState("");
  const [err, setErr] = useState("");
  const [busy, setBusy] = useState(false);

  // NDM 的 New URL 会自动读剪贴板: 打开弹窗时若剪贴板是 URL 则预填
  useEffect(() => {
    navigator.clipboard.readText()
      .then((text) => {
        if (/^(https?:\/\/|magnet:)\S+/i.test(text.trim())) setUrl(text.trim());
      })
      .catch(() => { /* 无剪贴板权限时忽略 */ });
  }, []);

  const urls = url.split("\n").map((s) => s.trim()).filter((s) => /^(https?:\/\/|magnet:)/i.test(s));
  const magnetOnly = urls.length > 0 && urls.every((u) => /^magnet:/i.test(u));
  const previewName = useMemo(() => {
    if (!urls.length) return "";
    if (/^magnet:/i.test(urls[0])) {
      const h = magnetHash(urls[0]);
      return h ? `磁力 ${h.slice(0, 8)}…` : "磁力链接";
    }
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
      let lastTorrent: api.TorrentInfo | undefined;
      for (const u of urls) {
        if (/^magnet:/i.test(u)) {
          lastTorrent = await api.addTorrent({ magnet: u, dir });
        } else {
          await api.addTask({
            url: u, dir, segments: conn, queue_only: queueOnly, headers: parseHeaders(),
          });
        }
      }
      props.onCreated(lastTorrent);
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
        <p class="page-sub">粘贴 http/https 或 magnet 链接, 支持多行. 也可选 .torrent 文件.</p>
        <div class="field">
          <label>下载链接</label>
          <textarea rows={4} autofocus placeholder="https://… 或 magnet:?xt=urn:btih:…" value={url}
            onInput={(e) => setUrl((e.target as HTMLTextAreaElement).value)} />
        </div>
        <div class="field">
          <label>或选择 .torrent</label>
          <input type="file" accept=".torrent,application/x-bittorrent" onChange={async (e) => {
            const f = (e.target as HTMLInputElement).files?.[0];
            if (!f) return;
            setBusy(true);
            setErr("");
            try {
              const buf = new Uint8Array(await f.arrayBuffer());
              let bin = "";
              for (const b of buf) bin += String.fromCharCode(b);
              await api.addTorrent({ torrent_b64: btoa(bin), dir });
              props.onCreated();
            } catch (er) {
              setErr(String(er));
            } finally {
              setBusy(false);
            }
          }} />
        </div>
        {previewName && (
          <div class="file-preview">
            <TypeIcon type={fileType(previewName)} size={15} />
            <span style={{ flex: 1, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
              {previewName}{urls.length > 1 ? ` 等 ${urls.length} 个文件` : ""}
            </span>
            <span style={{ color: "var(--text-3)", fontSize: 11 }}>
              {magnetOnly ? "点开始后解析文件列表" : "大小待探测"}
            </span>
          </div>
        )}
        <div class="field-row">
          <div class="field">
            <label>保存到</label>
            <DirPick value={dir} onChange={setDir} />
          </div>
          {!magnetOnly && (
            <div class="field">
              <label>连接数</label>
              <NumStep value={conn} min={1} max={MAX_CONN} onChange={setConn} />
            </div>
          )}
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

function peerState(s: string): string {
  if (s === "live") return "已连接";
  if (s === "connecting") return "连接中";
  if (s === "queued") return "排队";
  if (s === "dead") return "断开";
  if (s === "not_needed") return "空闲";
  return s;
}

function PeerInfo(props: { p: TorrentPeer }) {
  const { p } = props;
  const avg = p.pieces && p.piece_ms
    ? `${Math.round(p.piece_ms / p.pieces)} ms`
    : "—";
  const line = (k: string, v: string) => (
    <div class="kv-line"><span class="k">{k}</span><span class="v">{v}</span></div>
  );
  return (
    <div class="peer-info">
      {line("地址", p.addr)}
      {line("客户端", p.client || "—")}
      {line("状态", peerState(p.state))}
      {line("协议", p.kind || "—")}
      {line("方向", p.incoming ? "入站" : "出站")}
      {line("下载", fmtBytes(p.down))}
      {line("上传", fmtBytes(p.up))}
      {line("分片", `${p.pieces || 0} / ${p.chunks || 0} chunk`)}
      {line("分片耗时", avg)}
      {line("握手", p.conn_ms ? `${p.conn_ms} ms` : "—")}
      {line("尝试", `${p.attempts || 0} 次, 失败 ${p.errors || 0}`)}
    </div>
  );
}

export function TorrentPane(props: {
  t: TorrentInfo;
  collapsed: boolean;
  onToggle: () => void;
  onPickFiles: () => void;
  onHide: () => void;
}) {
  const { t } = props;
  const [sel, setSel] = useState<string | null>(null);
  const checking = t.phase === "initializing";
  const p = t.size ? t.done / t.size : 0;
  const pctLabel = `${Math.floor(p * 100)}%`;
  const running = t.state === "active" || t.state === "seeding" || t.state === "queued";
  const meta = STATE_META[t.state] || { label: t.state, cls: "" };
  const line = (k: string, v: string) => (
    <div class="kv-line"><span class="k">{k}</span><span class="v">{v}</span></div>
  );
  return (
    <aside class={"detail" + (props.collapsed ? " collapsed" : "")}>
      <div class="detail-inner">
        <div class="detail-top">
          <div class="prog-title">{checking ? "校验中" : pctLabel} {t.name}</div>
          <button class="icon-btn" title="关闭" onClick={props.onHide}><IcoX size={16} /></button>
        </div>
        <div class="prog-body">
          {line("状态", checking ? "正在校验本地文件" : meta.label + (t.error ? ` — ${t.error}` : ""))}
          {line("大小", t.size ? fmtBytes(t.size) : "未知")}
          {line(checking ? "已校验" : "已下载", `${fmtBytes(t.done)} ( ${pctLabel} )`)}
          {line("下载", running && !checking ? fmtRate(t.speed) : "0 B/s")}
          {line("上传", fmtRate(t.up_speed))}
          {line("节点", `${t.peers} 已连接 / ${t.connecting || 0} 连接中 / ${t.seen || 0} 已知`)}
          {line("剩余", checking ? "—" : fmtEtaBy(t.size, t.done, t.speed))}
          {line("Infohash", t.infohash)}
          {t.bt_direct && line("网络", "HTTP 代理下 BT 直连")}
          <div class="prog-bar"><div style={{ width: `${(p * 100).toFixed(2)}%` }}></div></div>
          <div class="seg-title" style={{ margin: 0 }}>
            <span>节点 {t.peer_list?.length || 0}</span>
          </div>
          {(t.peer_list && t.peer_list.length > 0) ? t.peer_list.map((peer) => (
            <div key={peer.addr}>
              <div class={"peer-row" + (sel === peer.addr ? " on" : "")}
                onClick={() => setSel(sel === peer.addr ? null : peer.addr)}>
                <span class="peer-kind">{peer.kind || peerState(peer.state)}</span>
                <span class="peer-addr" title={peer.addr}>{peer.addr}</span>
                <span class="peer-client">{peer.client || peerState(peer.state)}</span>
              </div>
              {sel === peer.addr && <PeerInfo p={peer} />}
            </div>
          )) : (
            <div class="settings-hint">{checking ? "校验完才会连 Peer" : "暂无已知节点"}</div>
          )}
          <div class="seg-title" style={{ margin: 0 }}>
            <span>文件 {t.files.filter((f) => f.selected).length}/{t.files.length}</span>
            {t.files.length > 1 && (
              <button class="btn" type="button" onClick={props.onPickFiles}>选择文件</button>
            )}
          </div>
          {t.files.map((f) => (
            <div class="conn-row" key={f.idx}>
              <span>{f.selected ? "●" : "○"}</span>
              <span>{f.path}</span>
              <span>{fmtBytes(f.size)}</span>
            </div>
          ))}
        </div>
        <div class="prog-foot">
          {t.state !== "awaiting_selection" && (
            <button class="btn" onClick={props.onToggle} disabled={checking}>
              {running ? "暂停" : t.state === "failed" ? "重试" : "继续"}
            </button>
          )}
        </div>
      </div>
    </aside>
  );
}
