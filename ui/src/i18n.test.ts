import { describe, expect, it } from "vitest";
import { pick, resolveLocale } from "./i18n";

describe("i18n", () => {
  it("只把中文系统语言映射为中文", () => {
    expect(resolveLocale(["zh-CN"])).toBe("zh");
    expect(resolveLocale(["zh-Hant", "en-US"])).toBe("zh");
  });

  it("英文和其它系统语言统一映射为英文", () => {
    expect(resolveLocale(["en-US"])).toBe("en");
    expect(resolveLocale(["ja-JP"])).toBe("en");
    expect(resolveLocale([])).toBe("en");
  });

  it("按语言返回界面文案", () => {
    expect(pick("zh", "下载中", "Downloading")).toBe("下载中");
    expect(pick("en", "下载中", "Downloading")).toBe("Downloading");
  });
});
