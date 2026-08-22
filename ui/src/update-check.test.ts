import { afterEach, describe, expect, it, vi } from "vitest";
import {
  FLASH_MS,
  LATEST_MSG,
  applyCheck,
  armFlash,
  planCheck,
} from "./update-check";
import type { CheckSnap } from "./update-check";

function snap(phase: CheckSnap["phase"], latest: string | null = null): CheckSnap {
  return { phase, latest };
}

describe("planCheck", () => {
  it("已是最新则只闪提示, 不问升级", () => {
    expect(planCheck(snap("up_to_date"))).toEqual({
      kind: "latest",
      flash: LATEST_MSG,
    });
    expect(LATEST_MSG).toBe("已经是最新版了");
  });

  it("有新版本先问, 文案带版本号", () => {
    expect(planCheck(snap("available", "1.2.3"))).toEqual({
      kind: "ask",
      prompt: "发现新版本 v1.2.3, 是否升级?",
    });
  });

  it("latest 缺失时仍要能问", () => {
    expect(planCheck(snap("available", null)).kind).toBe("ask");
    expect(planCheck(snap("available", null))).toEqual({
      kind: "ask",
      prompt: "发现新版本, 是否升级?",
    });
  });

  it.each([
    "idle",
    "checking",
    "downloading",
    "waiting",
    "installing",
    "error",
  ] as const)("phase=%s 时既不闪也不问", (phase) => {
    expect(planCheck(snap(phase))).toEqual({ kind: "none" });
  });
});

describe("applyCheck", () => {
  it("已是最新不走安装", async () => {
    const checked = snap("up_to_date");
    const install = vi.fn();
    const ask = vi.fn();
    const r = await applyCheck({
      check: async () => checked,
      install,
      ask,
    });
    expect(r).toEqual({ status: checked, flash: LATEST_MSG });
    expect(install).not.toHaveBeenCalled();
    expect(ask).not.toHaveBeenCalled();
  });

  it("确认后走通用安装路径", async () => {
    const found = snap("available", "2.0.0");
    const installed = snap("downloading", "2.0.0");
    const install = vi.fn(async () => installed);
    const ask = vi.fn(() => true);
    const r = await applyCheck({
      check: async () => found,
      install,
      ask,
    });
    expect(ask).toHaveBeenCalledWith("发现新版本 v2.0.0, 是否升级?");
    expect(install).toHaveBeenCalledTimes(1);
    expect(r).toEqual({ status: installed, flash: "" });
  });

  it("取消则保持 Available, 不安装", async () => {
    const found = snap("available", "2.0.0");
    const install = vi.fn();
    const r = await applyCheck({
      check: async () => found,
      install,
      ask: () => false,
    });
    expect(install).not.toHaveBeenCalled();
    expect(r).toEqual({ status: found, flash: "" });
  });

  it("探测失败不安装也不闪最新提示", async () => {
    const failed = snap("error");
    const install = vi.fn();
    const ask = vi.fn();
    const r = await applyCheck({
      check: async () => failed,
      install,
      ask,
    });
    expect(r).toEqual({ status: failed, flash: "" });
    expect(install).not.toHaveBeenCalled();
    expect(ask).not.toHaveBeenCalled();
  });
});

describe("armFlash", () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  it("提示停留 FLASH_MS 后消失", () => {
    vi.useFakeTimers();
    const texts: string[] = [];
    const slot: { current: ReturnType<typeof setTimeout> | null } = { current: null };
    const clock = {
      setTimeout: (fn: () => void, ms: number) => setTimeout(fn, ms),
      clearTimeout: (id: ReturnType<typeof setTimeout>) => clearTimeout(id),
    };
    armFlash((s) => texts.push(s), clock, slot, LATEST_MSG);
    expect(texts).toEqual([LATEST_MSG]);
    vi.advanceTimersByTime(FLASH_MS - 1);
    expect(texts).toEqual([LATEST_MSG]);
    vi.advanceTimersByTime(1);
    expect(texts).toEqual([LATEST_MSG, ""]);
    expect(FLASH_MS).toBe(3000);
  });

  it("再次闪提示会取消上一轮定时器", () => {
    vi.useFakeTimers();
    const texts: string[] = [];
    const slot: { current: ReturnType<typeof setTimeout> | null } = { current: null };
    const clock = {
      setTimeout: (fn: () => void, ms: number) => setTimeout(fn, ms),
      clearTimeout: (id: ReturnType<typeof setTimeout>) => clearTimeout(id),
    };
    armFlash((s) => texts.push(s), clock, slot, "first");
    vi.advanceTimersByTime(1000);
    armFlash((s) => texts.push(s), clock, slot, "second");
    vi.advanceTimersByTime(FLASH_MS - 1);
    expect(texts).toEqual(["first", "second"]);
    vi.advanceTimersByTime(1);
    expect(texts).toEqual(["first", "second", ""]);
  });
});
