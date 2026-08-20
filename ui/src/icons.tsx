import type { ComponentChildren, JSX } from "preact";
import type { FileType } from "./util";

interface IcoProps {
  d?: string;
  size?: number;
  children?: ComponentChildren;
}

function Ico({ d, size = 16, children }: IcoProps): JSX.Element {
  return (
    <svg width={size} height={size} viewBox="0 0 16 16" fill="none"
      stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">
      {d ? <path d={d} /> : null}
      {children}
    </svg>
  );
}

type P = { size?: number };

export const IcoPlus = (p: P) => <Ico {...p} d="M8 3.5v9M3.5 8h9" />;
export const IcoPlay = (p: P) => <Ico {...p} d="M5.2 3.6l7 4.4-7 4.4z" />;
export const IcoPause = (p: P) => <Ico {...p} d="M5.5 3.5v9M10.5 3.5v9" />;
export const IcoTrash = (p: P) => <Ico {...p} d="M3 4.5h10M6.5 4.5V3h3v1.5M4.5 4.5l.7 8h5.6l.7-8" />;
export const IcoGear = (p: P) => (
  <Ico {...p}>
    <circle cx="8" cy="8" r="2.2" />
    <path d="M8 1.8v1.7M8 12.5v1.7M1.8 8h1.7M12.5 8h1.7M3.6 3.6l1.2 1.2M11.2 11.2l1.2 1.2M12.4 3.6l-1.2 1.2M4.8 11.2l-1.2 1.2" />
  </Ico>
);
export const IcoSearch = (p: P) => (
  <Ico {...p}>
    <circle cx="7" cy="7" r="4" />
    <path d="M10.2 10.2l3 3" />
  </Ico>
);
export const IcoFolder = (p: P) => <Ico {...p} d="M2 4.5c0-.6.4-1 1-1h3l1.5 1.8H13c.6 0 1 .4 1 1v6c0 .6-.4 1-1 1H3c-.6 0-1-.4-1-1z" />;
export const IcoDown = (p: P) => <Ico {...p} d="M8 2.5v8M4.5 7l3.5 3.5L11.5 7M3 13.5h10" />;
export const IcoSun = (p: P) => (
  <Ico {...p}>
    <circle cx="8" cy="8" r="3" />
    <path d="M8 1.5v1.4M8 13.1v1.4M1.5 8h1.4M13.1 8h1.4M3.4 3.4l1 1M11.6 11.6l1 1M12.6 3.4l-1 1M4.4 11.6l-1 1" />
  </Ico>
);
export const IcoMoon = (p: P) => <Ico {...p} d="M13 9.5A5.5 5.5 0 016.5 3a5.5 5.5 0 106.5 6.5z" />;
export const IcoCheck = (p: P) => <Ico {...p} d="M3 8.5l3.2 3.2L13 5" />;
export const IcoAlert = (p: P) => (
  <Ico {...p}>
    <path d="M8 2l6.5 11.5h-13z" />
    <path d="M8 6.5v3M8 11.8v.2" />
  </Ico>
);
export const IcoCopy = (p: P) => (
  <Ico {...p}>
    <rect x="5.5" y="5.5" width="8" height="8" rx="1.2" />
    <path d="M10.5 5.5v-2c0-.6-.4-1-1-1h-6c-.6 0-1 .4-1 1v6c0 .6.4 1 1 1h2" />
  </Ico>
);
export const IcoQueue = (p: P) => <Ico {...p} d="M3 4.5h10M3 8h10M3 11.5h6" />;
export const IcoX = (p: P) => <Ico {...p} d="M4 4l8 8M12 4l-8 8" />;
export const IcoFilm = (p: P) => (
  <Ico {...p}>
    <rect x="2" y="3.5" width="12" height="9" rx="1.2" />
    <path d="M4.8 3.5v9M11.2 3.5v9M2 6.5h2.8M2 9.5h2.8M11.2 6.5H14M11.2 9.5H14" />
  </Ico>
);
export const IcoDoc = (p: P) => (
  <Ico {...p}>
    <path d="M4 2.5h5.5L12.5 5v8.5a1 1 0 01-1 1h-7.5a1 1 0 01-1-1v-10a1 1 0 011-1z" />
    <path d="M9.5 2.5V5h3M6 8.5h4M6 11h4" />
  </Ico>
);
export const IcoBox = (p: P) => (
  <Ico {...p}>
    <path d="M2.5 5L8 2.5 13.5 5v6L8 13.5 2.5 11z" />
    <path d="M2.5 5L8 7.5 13.5 5M8 7.5v6" />
  </Ico>
);
export const IcoApp = (p: P) => (
  <Ico {...p}>
    <rect x="2.5" y="2.5" width="11" height="11" rx="2.5" />
    <path d="M5.5 8l1.8 1.8 3.2-3.6" />
  </Ico>
);
export const IcoMusic = (p: P) => (
  <Ico {...p}>
    <path d="M6 12.5V4l7-1.5V11" />
    <circle cx="4.3" cy="12.5" r="1.7" />
    <circle cx="11.3" cy="11" r="1.7" />
  </Ico>
);
export const IcoFile = (p: P) => <Ico {...p} d="M4 2.5h5.5L12.5 5v8.5a1 1 0 01-1 1h-7.5a1 1 0 01-1-1v-10a1 1 0 011-1zM9.5 2.5V5h3" />;

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
