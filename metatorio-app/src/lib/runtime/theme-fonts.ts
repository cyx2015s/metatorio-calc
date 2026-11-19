export type ThemeMode = "system" | "light" | "dark";

export const THEME_KEY = "metatorio-theme";
export const UI_FONT_KEY = "metatorio-ui-font";
export const MONO_FONT_KEY = "metatorio-mono-font";

export const UI_FONT_FALLBACK = `"Inter", "IBM Plex Sans", "Segoe UI", system-ui, -apple-system, sans-serif`;
export const MONO_FONT_FALLBACK = `"JetBrains Mono", "SFMono-Regular", Consolas, "Cascadia Mono", monospace`;
export const GENERIC_FONT_FAMILIES = new Set([
  "serif",
  "sans-serif",
  "monospace",
  "cursive",
  "fantasy",
  "system-ui",
  "ui-serif",
  "ui-sans-serif",
  "ui-monospace",
  "ui-rounded",
  "math",
  "emoji",
  "fangsong",
]);

/** 清理用户输入的字体名称，剔除可能破坏 CSS 变量声明的字符。 */
export function sanitizeFontName(name: string): string {
  return name.replace(/[;{}]/g, "").trim();
}

/** 把用户输入的字体族转成安全的 CSS 字体栈。允许直接粘贴完整 stack（含逗号）。 */
export function fontStack(name: string, fallback: string): string {
  const raw = sanitizeFontName(name);
  if (!raw) return "";
  const family = raw.includes(",")
    ? raw
    : GENERIC_FONT_FAMILIES.has(raw.toLowerCase())
      ? raw
      : `"${raw.replace(/"/g, "")}"`;
  return `${family}, ${fallback}`;
}

/** 将主题模式解析为具体的亮/暗主题。 */
export function resolveTheme(theme: ThemeMode): "light" | "dark" {
  if (theme !== "system") return theme;
  return typeof window !== "undefined" &&
    window.matchMedia("(prefers-color-scheme: light)").matches
    ? "light"
    : "dark";
}

/** 依主题模式解析实际亮/暗，并写入 <html data-theme>。 */
export function applyThemeToDocument(theme: ThemeMode): void {
  if (typeof document === "undefined") return;
  document.documentElement.dataset.theme = resolveTheme(theme);
}

/** 将当前 UI 与等宽字体应用到文档根节点的 CSS 自定义属性。 */
export function applyFontsToDocument(uiFont: string, monoFont: string): void {
  if (typeof document === "undefined") return;
  const root = document.documentElement.style;
  const ui = fontStack(uiFont, UI_FONT_FALLBACK);
  const mono = fontStack(monoFont, MONO_FONT_FALLBACK);
  if (ui) root.setProperty("--sans", ui);
  else root.removeProperty("--sans");
  if (mono) root.setProperty("--mono", mono);
  else root.removeProperty("--mono");
}

/** 清理并持久化单个字体偏好到 localStorage。 */
export function persistFontPreference(key: string, name: string): string {
  const sanitized = sanitizeFontName(name);
  if (typeof localStorage !== "undefined") {
    if (sanitized) localStorage.setItem(key, sanitized);
    else localStorage.removeItem(key);
  }
  return sanitized;
}
