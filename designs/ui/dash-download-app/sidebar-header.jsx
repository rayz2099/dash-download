const { createRoot } = ReactDOM;

const Ico = ({ d, size = 14 }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="none"
    stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
    <path d={d} />
  </svg>
);

function Nav({ extraTop = 0 }) {
  const items = [
    ["M4 6h16M4 12h10M4 18h14", "全部", "3", true],
    ["M12 5v14M5 12h14", "下载中", "1", false],
    ["M4 6h16v4H4zM4 14h10", "等待中", "", false],
  ];
  return (
    <div className="side-group" style={{ paddingTop: extraTop }}>
      <div className="side-title">下载</div>
      {items.map(([d, label, n, on]) => (
        <div key={label} className={"side-item" + (on ? " on" : "")}>
          <span className="side-ico"><Ico d={d} /></span>
          {label}
          <span className="side-count">{n}</span>
        </div>
      ))}
    </div>
  );
}

function Win({ head, extraTop }) {
  return (
    <div className="win">
      <div className="titlebar">
        <div className="lights">
          <i style={{ background: "#ff5f57" }} />
          <i style={{ background: "#febc2e" }} />
          <i style={{ background: "#28c840" }} />
        </div>
        Dash Download
      </div>
      <div className="body">
        <div className="sidebar">
          {head}
          <Nav extraTop={extraTop} />
        </div>
        <div className="main">
          <div className="page-title">全部</div>
          <div className="page-sub">3 个任务</div>
        </div>
      </div>
    </div>
  );
}

function App() {
  return (
    <DesignCanvas>
      <DCPostIt top={24} left={28} width={280} rotate={-1.2}>
        窗口 titlebar 已经是 Dash Download. 侧栏再放蓝标+品名, 是第二份身份, 和主栏大标题「全部」抢层级.
      </DCPostIt>
      <DCSection id="header" title="Sidebar header"
        subtitle="生产截图是 A. B 是推荐: 标和字都可以没有, 导航直接开始.">
        <DCArtboard id="now" label="A · 现在" width={420} height={340}>
          <Win head={
            <div className="brand">
              <span className="brand-mark">
                <Ico d="M12 5v10M8 11l4 4 4-4M5 19h14" size={14} />
              </span>
              <span className="brand-name">Dash Download</span>
            </div>
          } />
        </DCArtboard>
        <DCArtboard id="none" label="B · 去品牌 (推荐)" width={420} height={340}>
          <Win head={null} />
        </DCArtboard>
        <DCArtboard id="mark" label="C · 只留兔子标" width={420} height={340}>
          <Win head={
            <div className="brand">
              <img className="brand-bunny" src="mark.png" alt="" />
            </div>
          } />
        </DCArtboard>
        <DCArtboard id="air" label="D · 更空" width={420} height={340}>
          <Win head={null} extraTop={10} />
        </DCArtboard>
      </DCSection>
    </DesignCanvas>
  );
}

createRoot(document.getElementById("root")).render(<App />);
