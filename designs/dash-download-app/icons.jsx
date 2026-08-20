// 16px 线性图标集, stroke 继承 currentColor
function Ico({ d, size = 16, sw = 1.6, children }) {
  return (
    <svg width={size} height={size} viewBox="0 0 16 16" fill="none"
      stroke="currentColor" strokeWidth={sw} strokeLinecap="round" strokeLinejoin="round">
      {d ? <path d={d} /> : null}
      {children}
    </svg>
  );
}

const IcoPlus = (p) => <Ico {...p} d="M8 3.5v9M3.5 8h9" />;
const IcoPlay = (p) => <Ico {...p} d="M5.2 3.6l7 4.4-7 4.4z" />;
const IcoPause = (p) => <Ico {...p} d="M5.5 3.5v9M10.5 3.5v9" />;
const IcoStop = (p) => <Ico {...p} d="M4.5 4.5h7v7h-7z" />;
const IcoTrash = (p) => <Ico {...p} d="M3 4.5h10M6.5 4.5V3h3v1.5M4.5 4.5l.7 8h5.6l.7-8" />;
const IcoGear = (p) => (
  <Ico {...p}>
    <circle cx="8" cy="8" r="2.2" />
    <path d="M8 1.8v1.7M8 12.5v1.7M1.8 8h1.7M12.5 8h1.7M3.6 3.6l1.2 1.2M11.2 11.2l1.2 1.2M12.4 3.6l-1.2 1.2M4.8 11.2l-1.2 1.2" />
  </Ico>
);
const IcoSearch = (p) => (
  <Ico {...p}>
    <circle cx="7" cy="7" r="4" />
    <path d="M10.2 10.2l3 3" />
  </Ico>
);
const IcoFolder = (p) => <Ico {...p} d="M2 4.5c0-.6.4-1 1-1h3l1.5 1.8H13c.6 0 1 .4 1 1v6c0 .6-.4 1-1 1H3c-.6 0-1-.4-1-1z" />;
const IcoDown = (p) => <Ico {...p} d="M8 2.5v8M4.5 7l3.5 3.5L11.5 7M3 13.5h10" />;
const IcoLink = (p) => <Ico {...p} d="M6.5 9.5l3-3M5 7L3.4 8.6a2.5 2.5 0 003.5 3.5L8.5 10.5M11 9l1.6-1.6a2.5 2.5 0 00-3.5-3.5L7.5 5.5" />;
const IcoSun = (p) => (
  <Ico {...p}>
    <circle cx="8" cy="8" r="3" />
    <path d="M8 1.5v1.4M8 13.1v1.4M1.5 8h1.4M13.1 8h1.4M3.4 3.4l1 1M11.6 11.6l1 1M12.6 3.4l-1 1M4.4 11.6l-1 1" />
  </Ico>
);
const IcoMoon = (p) => <Ico {...p} d="M13 9.5A5.5 5.5 0 016.5 3a5.5 5.5 0 106.5 6.5z" />;
const IcoCheck = (p) => <Ico {...p} d="M3 8.5l3.2 3.2L13 5" />;
const IcoAlert = (p) => (
  <Ico {...p}>
    <path d="M8 2l6.5 11.5h-13z" />
    <path d="M8 6.5v3M8 11.8v.2" />
  </Ico>
);
const IcoCopy = (p) => (
  <Ico {...p}>
    <rect x="5.5" y="5.5" width="8" height="8" rx="1.2" />
    <path d="M10.5 5.5v-2c0-.6-.4-1-1-1h-6c-.6 0-1 .4-1 1v6c0 .6.4 1 1 1h2" />
  </Ico>
);
const IcoQueue = (p) => <Ico {...p} d="M3 4.5h10M3 8h10M3 11.5h6" />;
const IcoX = (p) => <Ico {...p} d="M4 4l8 8M12 4l-8 8" />;
const IcoGlobe = (p) => (
  <Ico {...p}>
    <circle cx="8" cy="8" r="6" />
    <path d="M2 8h12M8 2c-3.5 3.7-3.5 8.3 0 12M8 2c3.5 3.7 3.5 8.3 0 12" />
  </Ico>
);

// 文件类型图标: 简洁的类型徽标
const IcoFilm = (p) => (
  <Ico {...p}>
    <rect x="2" y="3.5" width="12" height="9" rx="1.2" />
    <path d="M4.8 3.5v9M11.2 3.5v9M2 6.5h2.8M2 9.5h2.8M11.2 6.5H14M11.2 9.5H14" />
  </Ico>
);
const IcoDoc = (p) => (
  <Ico {...p}>
    <path d="M4 2.5h5.5L12.5 5v8.5a1 1 0 01-1 1h-7.5a1 1 0 01-1-1v-10a1 1 0 011-1z" />
    <path d="M9.5 2.5V5h3M6 8.5h4M6 11h4" />
  </Ico>
);
const IcoBox = (p) => (
  <Ico {...p}>
    <path d="M2.5 5L8 2.5 13.5 5v6L8 13.5 2.5 11z" />
    <path d="M2.5 5L8 7.5 13.5 5M8 7.5v6" />
  </Ico>
);
const IcoApp = (p) => (
  <Ico {...p}>
    <rect x="2.5" y="2.5" width="11" height="11" rx="2.5" />
    <path d="M5.5 8l1.8 1.8 3.2-3.6" />
  </Ico>
);
const IcoMusic = (p) => (
  <Ico {...p}>
    <path d="M6 12.5V4l7-1.5V11" />
    <circle cx="4.3" cy="12.5" r="1.7" />
    <circle cx="11.3" cy="11" r="1.7" />
  </Ico>
);
const IcoFile = (p) => <Ico {...p} d="M4 2.5h5.5L12.5 5v8.5a1 1 0 01-1 1h-7.5a1 1 0 01-1-1v-10a1 1 0 011-1zM9.5 2.5V5h3" />;

Object.assign(window, {
  Ico, IcoPlus, IcoPlay, IcoPause, IcoStop, IcoTrash, IcoGear, IcoSearch,
  IcoFolder, IcoDown, IcoLink, IcoSun, IcoMoon, IcoCheck, IcoAlert, IcoCopy,
  IcoQueue, IcoX, IcoGlobe, IcoFilm, IcoDoc, IcoBox, IcoApp, IcoMusic, IcoFile,
});
