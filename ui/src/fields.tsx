import { useEffect, useState } from "preact/hooks";
import * as api from "./api";
import { IcoFolder } from "./icons";

function clampInt(raw: string, min: number, max: number): number | null {
  const n = Number.parseInt(raw, 10);
  if (!Number.isFinite(n)) return null;
  return Math.min(max, Math.max(min, n));
}

/// +/− 步进, 中间可手填. 失焦/回车才提交, 非法值回退, 超出 min/max 钳住.
export function NumStep(props: {
  value: number;
  min: number;
  max: number;
  onChange: (n: number) => void;
}) {
  const { value, min, max, onChange } = props;
  const [text, setText] = useState(String(value));
  useEffect(() => setText(String(value)), [value]);

  const commit = () => {
    const n = clampInt(text, min, max);
    if (n == null) {
      setText(String(value));
      return;
    }
    setText(String(n));
    if (n !== value) onChange(n);
  };

  return (
    <div class="stepper">
      <button type="button" disabled={value <= min} onClick={() => onChange(value - 1)}>−</button>
      <input
        class="stepper-in"
        inputMode="numeric"
        value={text}
        onInput={(e) => {
          const v = (e.target as HTMLInputElement).value;
          if (v === "" || /^-?\d*$/.test(v)) setText(v);
        }}
        onBlur={commit}
        onKeyDown={(e) => {
          if (e.key === "Enter") (e.target as HTMLInputElement).blur();
        }}
      />
      <button type="button" disabled={value >= max} onClick={() => onChange(value + 1)}>+</button>
    </div>
  );
}

/// 路径可手填, 右侧浏览走系统目录面板. 空路径不提交.
export function DirPick(props: { value: string; onChange: (dir: string) => void }) {
  const [text, setText] = useState(props.value);
  useEffect(() => setText(props.value), [props.value]);

  const commit = () => {
    const v = text.trim();
    if (!v) {
      setText(props.value);
      return;
    }
    if (v !== props.value) props.onChange(v);
  };

  return (
    <div class="dir-pick">
      <input
        class="dir-pick-path"
        type="text"
        value={text}
        spellcheck={false}
        autoCapitalize="none"
        autoCorrect="off"
        autoComplete="off"
        onInput={(e) => setText((e.target as HTMLInputElement).value)}
        onBlur={commit}
        onKeyDown={(e) => {
          if (e.key === "Enter") (e.target as HTMLInputElement).blur();
        }}
      />
      <button type="button" class="btn" onClick={() => {
        api.pickDir(text || props.value).then((dir) => {
          if (!dir) return;
          setText(dir);
          if (dir !== props.value) props.onChange(dir);
        });
      }}>
        <IcoFolder size={15} /> 浏览
      </button>
    </div>
  );
}
