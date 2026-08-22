const { useState, useEffect, useMemo, useRef } = React;

const IconByName = ({ name, size = 16 }) => {
  const C = window[name] || window.IcoFile;
  return <C size={size} />;
};

// ── 侧栏 ──
function Sidebar({ tasks, filter, setFilter, globalSpeed, onSettings }) {
  const countByState = (key) => {
    if (key === "all") return tasks.length;
    if (key === "active") return tasks.filter((t) => t.state === "active" || t.state === "paused").length;
    return tasks.filter((t) => t.state === key).length;
  };
  const countByType = (key) => tasks.filter((t) => t.type === key).length;
  return (
    <div className="sidebar">
      <div className="traffic">
        <span style={{ background: "#ff5f57" }}></span>
        <span style={{ background: "#febc2e" }}></span>
        <span style={{ background: "#28c840" }}></span>
      </div>
      <div className="side-group">
        <div className="side-title">下载</div>
        {SIDE_STATES.map((s) => (
          <div key={s.key} className={"side-item" + (filter === s.key ? " on" : "")}
            onClick={() => setFilter(s.key)}>
            <span className="side-ico"><IconByName name={s.icon} size={14} /></span>
            <span className="side-label">{s.label}</span>
            <span className="side-count">{countByState(s.key) || ""}</span>
          </div>
        ))}
      </div>
      <div className="side-group">
        <div className="side-title">分类</div>
        {Object.entries(TYPE_META).map(([key, m]) => (
          <div key={key} className={"side-item" + (filter === "type:" + key ? " on" : "")}
            onClick={() => setFilter("type:" + key)}>
            <span className="side-ico"><IconByName name={m.icon} size={14} /></span>
            <span className="side-label">{m.label}</span>
            <span className="side-count">{countByType(key) || ""}</span>
          </div>
        ))}
      </div>
      <div className="side-foot">
        <div className="global-speed">
          {fmtSpeed(globalSpeed).replace(/ .*/, "")}
          <small>{globalSpeed > 0 ? fmtSpeed(globalSpeed).replace(/^\S+ /, "") + " 总速度" : "空闲"}</small>
        </div>
        <div className="ext-pill" style={{ justifyContent: "space-between" }}>
          <span style={{ display: "flex", alignItems: "center", gap: 7 }}>
            <span className="ext-dot"></span>Chrome 扩展已连接
          </span>
          <button className="icon-btn" onClick={onSettings} title="设置" style={{ width: 24, height: 24 }}>
            <IcoGear size={14} />
          </button>
        </div>
      </div>
    </div>
  );
}

// ── 任务行 ──
function TaskRow({ t, selected, onSelect, onToggle, onRemove }) {
  const p = pct(t);
  const fillCls = t.state === "completed" ? "done" : t.state === "failed" ? "err" : t.state === "paused" || t.state === "queued" ? "paused" : "";
  const meta = [];
  if (t.state === "completed") {
    meta.push(fmtBytes(t.size), t.completedAt || "");
  } else if (t.state === "failed") {
    meta.push(fmtBytes(t.done) + " / " + fmtBytes(t.size), t.error.split(":")[0]);
  } else {
    meta.push(fmtBytes(t.done) + " / " + fmtBytes(t.size), t.segments.length + " 连接");
  }
  return (
    <div className={"task-row" + (selected ? " selected" : "")} onClick={() => onSelect(t.id)}>
      <div className="task-ico"><IconByName name={TYPE_META[t.type].icon} size={17} /></div>
      <div className="task-mid">
        <div className="task-name">{t.name}</div>
        <div className="task-meta">
          {meta.filter(Boolean).map((m, i) => (
            <span key={i} className={i > 0 ? "sep" : ""}>{m}</span>
          ))}
        </div>
        {t.state !== "completed" && (
          <div className="bar">
            <div className={"bar-fill " + fillCls} style={{ width: (p * 100).toFixed(2) + "%" }}></div>
          </div>
        )}
      </div>
      <div className="task-right">
        {t.state === "active" ? (
          <React.Fragment>
            <span className="task-speed">{fmtSpeed(t.speed)}</span>
            <span className="task-eta">剩余 {fmtEta(t)}</span>
          </React.Fragment>
        ) : (
          <span className={"state-badge " + STATE_META[t.state].cls}>{STATE_META[t.state].label}</span>
        )}
      </div>
      <div className="row-actions" onClick={(e) => e.stopPropagation()}>
        {(t.state === "active" || t.state === "paused" || t.state === "queued" || t.state === "failed") && (
          <button className="icon-btn" title={t.state === "active" ? "暂停" : "开始"} onClick={() => onToggle(t.id)}>
            {t.state === "active" ? <IcoPause size={14} /> : <IcoPlay size={14} />}
          </button>
        )}
        {t.state === "completed" && (
          <button className="icon-btn" title="在 Finder 中显示"><IcoFolder size={14} /></button>
        )}
        <button className="icon-btn" title="删除" onClick={() => onRemove(t.id)}><IcoTrash size={14} /></button>
      </div>
    </div>
  );
}

// ── 详情面板 ──
function DetailPanel({ t, onToggle, onRemove, onClose }) {
  if (!t) return null;
  const p = pct(t);
  return (
    <div className="detail" data-screen-label="任务详情">
      <div className="detail-head">
        <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
          <span className={"state-badge " + STATE_META[t.state].cls}>{STATE_META[t.state].label}</span>
          <button className="icon-btn" onClick={onClose}><IcoX size={13} /></button>
        </div>
        <div className="detail-name">{t.name}</div>
        <div style={{ fontSize: 11.5, color: "var(--text-3)" }}>
          {TYPE_META[t.type].label} · {fmtBytes(t.size)}
        </div>
      </div>

      {t.state !== "completed" && (
        <div className="seg-section">
          <div className="seg-title">
            <span>分段 ({t.segments.length})</span>
            <span>{(p * 100).toFixed(1)}%</span>
          </div>
          <div className="seg-map">
            {t.segments.map((s, i) => (
              <div key={i} className="seg-cell" style={{ flex: s.end - s.start }}>
                <div className="seg-cell-fill" style={{ width: ((s.done / (s.end - s.start)) * 100).toFixed(1) + "%" }}></div>
              </div>
            ))}
          </div>
          {t.state === "failed" && (
            <div style={{ marginTop: 10, fontSize: 11.5, color: "var(--red)", lineHeight: 1.5 }}>{t.error}</div>
          )}
        </div>
      )}

      <div className="stat-grid">
        <div><div className="stat-label">已下载</div><div className="stat-value">{fmtBytes(t.done)}</div></div>
        <div><div className="stat-label">总大小</div><div className="stat-value">{fmtBytes(t.size)}</div></div>
        <div><div className="stat-label">速度</div><div className="stat-value">{t.state === "active" ? fmtSpeed(t.speed) : "—"}</div></div>
        <div><div className="stat-label">剩余时间</div><div className="stat-value">{t.state === "active" ? fmtEta(t) : "—"}</div></div>
      </div>

      <div className="kv-section">
        <div>
          <div className="kv-label">链接 <IcoCopy size={12} /></div>
          <div className="kv-value">{t.url}</div>
        </div>
        {t.referer ? (
          <div>
            <div className="kv-label">Referer</div>
            <div className="kv-value">{t.referer}</div>
          </div>
        ) : null}
        <div>
          <div className="kv-label">保存位置</div>
          <div className="kv-value">{t.dir}/{t.name}</div>
        </div>
      </div>

      <div className="detail-actions">
        {t.state === "completed" ? (
          <button className="btn" style={{ flex: 1 }}><IcoFolder size={13} /> 在 Finder 中显示</button>
        ) : (
          <button className="btn" style={{ flex: 1 }} onClick={() => onToggle(t.id)}>
            {t.state === "active" ? <IcoPause size={13} /> : <IcoPlay size={13} />}
            {t.state === "active" ? "暂停" : t.state === "failed" ? "重试" : "开始"}
          </button>
        )}
        <button className="btn danger" onClick={() => onRemove(t.id)}><IcoTrash size={13} /> 删除</button>
      </div>
    </div>
  );
}

// ── 新建下载 ──
function NewTaskModal({ onClose, onCreate }) {
  const [url, setUrl] = useState("");
  const [conn, setConn] = useState(8);
  const [showAdv, setShowAdv] = useState(false);
  const [headers, setHeaders] = useState("");
  const name = useMemo(() => {
    try {
      const path = new URL(url.trim().split("\n")[0]).pathname;
      return decodeURIComponent(path.split("/").filter(Boolean).pop() || "") || "未命名文件";
    } catch { return ""; }
  }, [url]);
  return (
    <div className="overlay" onClick={onClose}>
      <div className="modal" data-screen-label="新建下载" onClick={(e) => e.stopPropagation()}>
        <h2>新建下载</h2>
        <div className="field">
          <label>下载链接 (支持多行批量)</label>
          <textarea rows={3} autoFocus placeholder="https://…" value={url}
            onChange={(e) => setUrl(e.target.value)} />
        </div>
        {name ? (
          <div className="file-preview">
            <IcoFile size={15} />
            <span style={{ flex: 1, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{name}</span>
            <span style={{ color: "var(--text-3)", fontSize: 11 }}>大小待探测</span>
          </div>
        ) : null}
        <div className="field-row">
          <div className="field">
            <label>保存到</label>
            <select defaultValue="~/Downloads">
              <option>~/Downloads</option>
              <option>~/Movies</option>
              <option>~/Documents</option>
              <option>选择目录…</option>
            </select>
          </div>
          <div className="field">
            <label>连接数</label>
            <div className="stepper">
              <button onClick={() => setConn(Math.max(1, conn - 1))}>−</button>
              <b>{conn}</b>
              <button onClick={() => setConn(Math.min(16, conn + 1))}>+</button>
            </div>
          </div>
        </div>
        <div>
          <button className="btn" style={{ border: "none", padding: 0, height: "auto", color: "var(--text-2)", fontSize: 12 }}
            onClick={() => setShowAdv(!showAdv)}>
            {showAdv ? "隐藏高级选项" : "高级选项…"}
          </button>
          {showAdv && (
            <div className="field" style={{ marginTop: 10 }}>
              <label>自定义 Header (每行一条, 如 Cookie: xxx)</label>
              <textarea rows={3} value={headers} onChange={(e) => setHeaders(e.target.value)}
                placeholder={"Referer: https://example.com\nCookie: session=…"} />
            </div>
          )}
        </div>
        <div className="modal-foot">
          <button className="btn" onClick={onClose}>取消</button>
          <button className="btn" disabled={!name} onClick={() => onCreate(url, name, conn, true)}>加入队列</button>
          <button className="btn primary" disabled={!name} onClick={() => onCreate(url, name, conn, false)}>
            <IcoDown size={13} /> 开始下载
          </button>
        </div>
      </div>
    </div>
  );
}

// ── 设置 ──
function SettingsModal({ onClose }) {
  const [notify, setNotify] = useState(true);
  const [takeover, setTakeover] = useState(true);
  const Row = ({ label, hint, children }) => (
    <div className="settings-row">
      <div>
        <div className="settings-label">{label}</div>
        {hint ? <div className="settings-hint">{hint}</div> : null}
      </div>
      {children}
    </div>
  );
  return (
    <div className="overlay" onClick={onClose}>
      <div className="modal" data-screen-label="设置" onClick={(e) => e.stopPropagation()} style={{ width: 520 }}>
        <h2>设置</h2>
        <div>
          <div className="side-title" style={{ padding: "0 0 2px" }}>通用</div>
          <Row label="默认下载目录"><span className="mono-chip">~/Downloads <IcoFolder size={12} /></span></Row>
          <Row label="同时下载任务数" hint="超出的任务进入队列等待">
            <div className="stepper"><button>−</button><b>3</b><button>+</button></div>
          </Row>
          <Row label="每任务最大连接数" hint="服务器支持 Range 时的分段上限">
            <div className="stepper"><button>−</button><b>8</b><button>+</button></div>
          </Row>
          <Row label="下载完成后通知">
            <button className={"toggle" + (notify ? " on" : "")} onClick={() => setNotify(!notify)}></button>
          </Row>
        </div>
        <div>
          <div className="side-title" style={{ padding: "6px 0 2px" }}>浏览器扩展</div>
          <Row label="接管浏览器下载" hint="小于 1MB 或 text/html 的请求不接管">
            <button className={"toggle" + (takeover ? " on" : "")} onClick={() => setTakeover(!takeover)}></button>
          </Row>
          <Row label="API 端口"><span className="mono-chip">127.0.0.1:41320</span></Row>
          <Row label="访问令牌" hint="扩展与 app 配对使用">
            <span className="mono-chip">dd_tok_9f3e···c21a <IcoCopy size={12} /></span>
          </Row>
          <Row label="扩展状态">
            <span className="ext-pill"><span className="ext-dot"></span>已连接 · Chrome 139</span>
          </Row>
        </div>
        <div className="modal-foot">
          <button className="btn primary" onClick={onClose}>完成</button>
        </div>
      </div>
    </div>
  );
}

// ── App ──
function App() {
  const [theme, setTheme] = useState("light");
  const [tasks, setTasks] = useState(INITIAL_TASKS);
  const [filter, setFilter] = useState("all");
  const [query, setQuery] = useState("");
  const [selectedId, setSelectedId] = useState("t1");
  const [showNew, setShowNew] = useState(false);
  const [showSettings, setShowSettings] = useState(false);

  // 模拟下载进度: 活动任务按速度推进, 各 Segment 均摊
  useEffect(() => {
    const timer = setInterval(() => {
      setTasks((prev) => prev.map((t) => {
        if (t.state !== "active") return t;
        const gain = t.speed * 0.6 * (0.85 + Math.random() * 0.3);
        let rest = gain;
        const segs = t.segments.map((s) => {
          const room = s.end - s.start - s.done;
          const add = Math.min(room, Math.ceil(rest / 2) + 1);
          rest -= add;
          return { ...s, done: s.done + Math.max(0, add) };
        });
        const done = segDone(segs);
        if (done >= t.size) {
          return { ...t, segments: segs, done: t.size, state: "completed", speed: 0, completedAt: "刚刚" };
        }
        return { ...t, segments: segs, done, speed: t.speed * (0.92 + Math.random() * 0.16) };
      }));
    }, 600);
    return () => clearInterval(timer);
  }, []);

  const visible = useMemo(() => {
    let list = tasks;
    if (filter === "active") list = list.filter((t) => t.state === "active" || t.state === "paused");
    else if (filter.startsWith("type:")) list = list.filter((t) => t.type === filter.slice(5));
    else if (filter !== "all") list = list.filter((t) => t.state === filter);
    if (query) list = list.filter((t) => t.name.toLowerCase().includes(query.toLowerCase()));
    return list;
  }, [tasks, filter, query]);

  const globalSpeed = tasks.reduce((a, t) => a + (t.state === "active" ? t.speed : 0), 0);
  const selected = tasks.find((t) => t.id === selectedId);
  const hasActive = tasks.some((t) => t.state === "active");

  const toggleTask = (id) => setTasks((prev) => prev.map((t) => {
    if (t.id !== id) return t;
    if (t.state === "active") return { ...t, state: "paused", speed: 0 };
    if (t.state === "paused" || t.state === "queued" || t.state === "failed")
      return { ...t, state: "active", speed: (4 + Math.random() * 8) * MB, error: "" };
    return t;
  }));
  const removeTask = (id) => {
    setTasks((prev) => prev.filter((t) => t.id !== id));
    if (selectedId === id) setSelectedId(null);
  };
  const toggleAll = () => setTasks((prev) => prev.map((t) => {
    if (hasActive) return t.state === "active" ? { ...t, state: "paused", speed: 0 } : t;
    return t.state === "paused" || t.state === "queued" ? { ...t, state: "active", speed: (4 + Math.random() * 8) * MB } : t;
  }));
  const createTask = (url, name, conn, queued) => {
    const size = (80 + Math.random() * 800) * MB;
    const id = "t" + Date.now();
    setTasks((prev) => [task({
      id, name, url: url.trim().split("\n")[0], size,
      type: /\.(mp4|mkv|mov)/i.test(name) ? "video" : /\.(zip|7z|rar|ipsw|tar)/i.test(name) ? "archive"
        : /\.(pdf|docx?|epub)/i.test(name) ? "doc" : /\.(mp3|flac|aac)/i.test(name) ? "audio"
        : /\.(exe|dmg|apk|pkg)/i.test(name) ? "app" : "other",
      state: queued ? "queued" : "active",
      segments: makeSegments(size, conn, 0),
      speed: queued ? 0 : (4 + Math.random() * 8) * MB,
    }), ...prev]);
    setShowNew(false);
    setSelectedId(id);
    if (!queued) setFilter("all");
  };

  const filterTitle = filter.startsWith("type:")
    ? TYPE_META[filter.slice(5)].label
    : SIDE_STATES.find((s) => s.key === filter)?.label || "全部";

  return (
    <div className="desktop" data-theme={theme}>
      <div className="app-window" data-screen-label="主窗口">
        <Sidebar tasks={tasks} filter={filter} setFilter={setFilter}
          globalSpeed={globalSpeed} onSettings={() => setShowSettings(true)} />
        <div className="main">
          <div className="toolbar">
            <span className="tb-title">{filterTitle}</span>
            <span className="tb-sub">{visible.length} 项</span>
            <div className="spacer"></div>
            <button className="btn" onClick={toggleAll}>
              {hasActive ? <IcoPause size={13} /> : <IcoPlay size={13} />}
              {hasActive ? "全部暂停" : "全部开始"}
            </button>
            <button className="btn primary" onClick={() => setShowNew(true)}>
              <IcoPlus size={13} /> 新建下载
            </button>
            <div className="search">
              <IcoSearch size={13} />
              <input placeholder="搜索" value={query} onChange={(e) => setQuery(e.target.value)} />
            </div>
            <button className="icon-btn" title="切换主题"
              onClick={() => setTheme(theme === "light" ? "dark" : "light")}>
              {theme === "light" ? <IcoMoon size={15} /> : <IcoSun size={15} />}
            </button>
          </div>
          <div className="task-scroll">
            {visible.length === 0 ? (
              <div className="empty">
                <IcoDown size={28} />
                <span>没有{filterTitle === "全部" ? "" : filterTitle}任务</span>
              </div>
            ) : (
              visible.map((t) => (
                <TaskRow key={t.id} t={t} selected={t.id === selectedId}
                  onSelect={setSelectedId} onToggle={toggleTask} onRemove={removeTask} />
              ))
            )}
          </div>
        </div>
        {selected ? (
          <DetailPanel t={selected} onToggle={toggleTask} onRemove={removeTask}
            onClose={() => setSelectedId(null)} />
        ) : null}
        {showNew && <NewTaskModal onClose={() => setShowNew(false)} onCreate={createTask} />}
        {showSettings && <SettingsModal onClose={() => setShowSettings(false)} />}
      </div>
    </div>
  );
}

ReactDOM.createRoot(document.getElementById("root")).render(<App />);
