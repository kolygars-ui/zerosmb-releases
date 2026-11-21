export const DEFAULT_THEME_COLOR = "#080c18";

export function normalizeHex(value: string): string {
  const trimmed = value.trim();
  if (/^#[0-9a-fA-F]{6}$/.test(trimmed)) return trimmed.toLowerCase();
  return DEFAULT_THEME_COLOR;
}

export function hexToRgb(value: string): { r: number; g: number; b: number } {
  const hex = normalizeHex(value).slice(1);
  return {
    r: Number.parseInt(hex.slice(0, 2), 16),
    g: Number.parseInt(hex.slice(2, 4), 16),
    b: Number.parseInt(hex.slice(4, 6), 16),
  };
}

export function rgbToHex(r: number, g: number, b: number): string {
  const clamp = (value: number) =>
    Math.max(0, Math.min(255, Math.round(value)));
  return `#${[r, g, b]
    .map((value) => clamp(value).toString(16).padStart(2, "0"))
    .join("")}`;
}

export function isLightColor(value: string): boolean {
  const { r, g, b } = hexToRgb(value);
  const luminance = (0.2126 * r + 0.7152 * g + 0.0722 * b) / 255;
  return luminance > 0.62;
}

export function applyTheme(value: string): void {
  const color = normalizeHex(value);
  const light = isLightColor(color);
  const root = document.documentElement;
  root.style.setProperty("--theme-bg", color);
  root.style.colorScheme = light ? "light" : "dark";
  document.body.classList.toggle("theme-light", light);
}
