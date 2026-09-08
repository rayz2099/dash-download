export type Locale = "zh" | "en";

/** 只支持中英文；非中文系统统一使用英文，避免意外回退到中文。 */
export function resolveLocale(langs: readonly string[]): Locale {
  const lang = langs[0]?.toLowerCase() || "";
  return lang.startsWith("zh") ? "zh" : "en";
}

function systemLangs(): readonly string[] {
  if (typeof navigator === "undefined") return [];
  if (navigator.languages?.length) return navigator.languages;
  return navigator.language ? [navigator.language] : [];
}

export const LOCALE = resolveLocale(systemLangs());
export const LANG = LOCALE === "zh" ? "zh-CN" : "en-US";

export function pick(locale: Locale, zh: string, en: string): string {
  return locale === "zh" ? zh : en;
}

export function t(zh: string, en: string): string {
  return pick(LOCALE, zh, en);
}
