import type { ComponentChildren, JSX } from "preact";
import type { FileType } from "./util";

interface IcoProps {
  size?: number;
  children: ComponentChildren;
}

// Lucide 24 网格, stroke 1.75: 侧栏/工具栏共用一套线型, 避免 16px 自制图标发糊发挤
function Ico({ size = 18, children }: IcoProps): JSX.Element {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" fill="none"
      stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round">
      {children}
    </svg>
  );
}

type P = { size?: number };

export const IcoPlus = (p: P) => (
  <Ico {...p}>
    <path d="M5 12h14" />
    <path d="M12 5v14" />
  </Ico>
);
export const IcoPlay = (p: P) => (
  <Ico {...p}>
    <polygon points="6 3 20 12 6 21 6 3" />
  </Ico>
);
export const IcoPause = (p: P) => (
  <Ico {...p}>
    <rect x="14" y="4" width="4" height="16" rx="1" />
    <rect x="6" y="4" width="4" height="16" rx="1" />
  </Ico>
);
export const IcoTrash = (p: P) => (
  <Ico {...p}>
    <path d="M3 6h18" />
    <path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6" />
    <path d="M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2" />
    <line x1="10" x2="10" y1="11" y2="17" />
    <line x1="14" x2="14" y1="11" y2="17" />
  </Ico>
);
export const IcoGear = (p: P) => (
  <Ico {...p}>
    <path d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z" />
    <circle cx="12" cy="12" r="3" />
  </Ico>
);
export const IcoSearch = (p: P) => (
  <Ico {...p}>
    <circle cx="11" cy="11" r="8" />
    <path d="m21 21-4.3-4.3" />
  </Ico>
);
export const IcoFolder = (p: P) => (
  <Ico {...p}>
    <path d="M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z" />
  </Ico>
);
export const IcoDown = (p: P) => (
  <Ico {...p}>
    <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4" />
    <polyline points="7 10 12 15 17 10" />
    <line x1="12" x2="12" y1="15" y2="3" />
  </Ico>
);
export const IcoSun = (p: P) => (
  <Ico {...p}>
    <circle cx="12" cy="12" r="4" />
    <path d="M12 2v2" />
    <path d="M12 20v2" />
    <path d="m4.93 4.93 1.41 1.41" />
    <path d="m17.66 17.66 1.41 1.41" />
    <path d="M2 12h2" />
    <path d="M20 12h2" />
    <path d="m6.34 17.66-1.41 1.41" />
    <path d="m19.07 4.93-1.41 1.41" />
  </Ico>
);
export const IcoMoon = (p: P) => (
  <Ico {...p}>
    <path d="M12 3a6 6 0 0 0 9 9 9 9 0 1 1-9-9Z" />
  </Ico>
);
export const IcoCheck = (p: P) => (
  <Ico {...p}>
    <circle cx="12" cy="12" r="10" />
    <path d="m9 12 2 2 4-4" />
  </Ico>
);
export const IcoAlert = (p: P) => (
  <Ico {...p}>
    <circle cx="12" cy="12" r="10" />
    <line x1="12" x2="12" y1="8" y2="12" />
    <line x1="12" x2="12.01" y1="16" y2="16" />
  </Ico>
);
export const IcoCopy = (p: P) => (
  <Ico {...p}>
    <rect width="14" height="14" x="8" y="8" rx="2" ry="2" />
    <path d="M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2" />
  </Ico>
);
export const IcoQueue = (p: P) => (
  <Ico {...p}>
    <path d="M8 6h13" />
    <path d="M8 12h13" />
    <path d="M8 18h13" />
    <path d="M3 6h.01" />
    <path d="M3 12h.01" />
    <path d="M3 18h.01" />
  </Ico>
);
export const IcoClock = (p: P) => (
  <Ico {...p}>
    <circle cx="12" cy="12" r="10" />
    <polyline points="12 6 12 12 16 14" />
  </Ico>
);
export const IcoX = (p: P) => (
  <Ico {...p}>
    <path d="M18 6 6 18" />
    <path d="m6 6 12 12" />
  </Ico>
);
export const IcoFilm = (p: P) => (
  <Ico {...p}>
    <rect width="18" height="18" x="3" y="3" rx="2" />
    <path d="M7 3v18" />
    <path d="M3 7.5h4" />
    <path d="M3 12h18" />
    <path d="M3 16.5h4" />
    <path d="M17 3v18" />
    <path d="M17 7.5h4" />
    <path d="M17 16.5h4" />
  </Ico>
);
export const IcoDoc = (p: P) => (
  <Ico {...p}>
    <path d="M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7Z" />
    <path d="M14 2v4a2 2 0 0 0 2 2h4" />
    <path d="M10 9H8" />
    <path d="M16 13H8" />
    <path d="M16 17H8" />
  </Ico>
);
export const IcoBox = (p: P) => (
  <Ico {...p}>
    <rect width="20" height="5" x="2" y="3" rx="1" />
    <path d="M4 8v11a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8" />
    <path d="M10 12h4" />
  </Ico>
);
export const IcoApp = (p: P) => (
  <Ico {...p}>
    <rect width="20" height="16" x="2" y="4" rx="2" />
    <path d="M2 8h20" />
    <path d="M6 4v4" />
    <path d="M10 4v4" />
  </Ico>
);
export const IcoMusic = (p: P) => (
  <Ico {...p}>
    <path d="M9 18V5l12-2v13" />
    <circle cx="6" cy="18" r="3" />
    <circle cx="18" cy="16" r="3" />
  </Ico>
);
export const IcoFile = (p: P) => (
  <Ico {...p}>
    <path d="M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7Z" />
    <path d="M14 2v4a2 2 0 0 0 2 2h4" />
  </Ico>
);

export function TypeIcon({ type, size = 16 }: { type: FileType; size?: number }): JSX.Element {
  switch (type) {
    case "video": return <IcoFilm size={size} />;
    case "doc": return <IcoDoc size={size} />;
    case "archive": return <IcoBox size={size} />;
    case "app": return <IcoApp size={size} />;
    case "audio": return <IcoMusic size={size} />;
    default: return <IcoFile size={size} />;
  }
}
