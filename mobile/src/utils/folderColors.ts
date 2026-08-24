/** Same 24-swatch palette + default as web `filemanager.js`. */
export const DEFAULT_FOLDER_COLOR = "#5f6368";

export const FOLDER_COLORS = [
  "#ac725e",
  "#d06b64",
  "#f83a22",
  "#fa573c",
  "#ff7537",
  "#ffad46",
  "#fad165",
  "#fbe983",
  "#4986e7",
  "#9fc6e7",
  "#9fe1e7",
  "#92e1c0",
  "#7bd148",
  "#16a765",
  "#b3dc6c",
  "#42d692",
  "#5f6368",
  "#c2c2c2",
  "#cabdbf",
  "#cca6ac",
  "#f691b2",
  "#cd74e6",
  "#a47ae2",
  "#b99aff",
] as const;

export function resolveFolderColor(color?: string | null): string {
  let value = String(color || "").trim();
  if (/^[0-9a-fA-F]{3,8}$/.test(value)) value = `#${value}`;
  if (/^#[0-9a-fA-F]{3,8}$/.test(value)) return value.toLowerCase();
  return DEFAULT_FOLDER_COLOR;
}

/** Persist default as empty string (matches web / server omitempty). */
export function storeFolderColor(hex: string): string {
  const resolved = resolveFolderColor(hex);
  return resolved === DEFAULT_FOLDER_COLOR ? "" : resolved;
}
