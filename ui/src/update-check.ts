import type { UpdateStatus } from "./api";

/** 手动检查后的瞬时提示, 不能写进长期 hint, 否则没法验「停留后消失」. */
export const LATEST_MSG = "已经是最新版了";
export const FLASH_MS = 3000;

export type CheckSnap = Pick<UpdateStatus, "phase" | "latest">;

export type CheckPlan =
  | { kind: "latest"; flash: string }
  | { kind: "ask"; prompt: string }
  | { kind: "none" };

/** 设置页只根据探测结果决定闪一下还是先问, 安装必须另走 checkNow. */
export function planCheck(st: CheckSnap): CheckPlan {
  if (st.phase === "up_to_date") return { kind: "latest", flash: LATEST_MSG };
  if (st.phase === "available") {
    const ver = st.latest ? ` v${st.latest}` : "";
    return { kind: "ask", prompt: `发现新版本${ver}, 是否升级?` };
  }
  return { kind: "none" };
}

export interface CheckDeps<T extends CheckSnap> {
  check: () => Promise<T>;
  install: () => Promise<T>;
  ask: (prompt: string) => boolean;
}

/** check → (最新闪提示 | 确认后 install), 顺序是这条功能的验收点. */
export async function applyCheck<T extends CheckSnap>(
  deps: CheckDeps<T>,
): Promise<{ status: T; flash: string }> {
  const st = await deps.check();
  const plan = planCheck(st);
  if (plan.kind === "latest") return { status: st, flash: plan.flash };
  if (plan.kind === "ask" && deps.ask(plan.prompt)) {
    return { status: await deps.install(), flash: "" };
  }
  return { status: st, flash: "" };
}

/** 提示必须可被假时钟推进, 才能断言停留后消失而不是一直挂着. */
export function armFlash<T>(
  setText: (s: string) => void,
  clock: {
    setTimeout: (fn: () => void, ms: number) => T
    clearTimeout: (id: T) => void
  },
  slot: { current: T | null },
  msg: string,
  ms = FLASH_MS,
) {
  setText(msg);
  if (slot.current != null) clock.clearTimeout(slot.current);
  slot.current = clock.setTimeout(() => setText(""), ms);
}
