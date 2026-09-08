export type ThemeColors = {
  bg: string;
  surface: string;
  surfaceElevated: string;
  border: string;
  text: string;
  textSecondary: string;
  accent: string;
  accentSoft: string;
  accentMuted: string;
  fab: string;
  danger: string;
  success: string;
  folder: string;
  doc: string;
  sheet: string;
  image: string;
  video: string;
  inputBg: string;
  overlay: string;
  sectionHeader: string;
  pillBorder: string;
};

export const darkColors: ThemeColors = {
  bg: "#121212",
  surface: "#1E1E1E",
  surfaceElevated: "#2A2A2A",
  border: "#2C2C2C",
  text: "#FFFFFF",
  textSecondary: "#9E9E9E",
  accent: "#8AB4F8",
  accentSoft: "#DDE1EE",
  accentMuted: "#B4C5FF",
  fab: "#3C4454",
  danger: "#F28B82",
  success: "#81C995",
  folder: "#5f6368",
  doc: "#4285F4",
  sheet: "#0F9D58",
  image: "#E8710A",
  video: "#A142F4",
  inputBg: "#2A2A2A",
  overlay: "rgba(0,0,0,0.55)",
  sectionHeader: "#A8C7FA",
  pillBorder: "#E3E3E3",
};

export const lightColors: ThemeColors = {
  bg: "#FFFFFF",
  surface: "#F8F9FA",
  surfaceElevated: "#FFFFFF",
  border: "#E0E0E0",
  text: "#202124",
  textSecondary: "#5F6368",
  accent: "#1A73E8",
  accentSoft: "#E8F0FE",
  accentMuted: "#1967D2",
  fab: "#E8F0FE",
  danger: "#D93025",
  success: "#188038",
  folder: "#5f6368",
  doc: "#4285F4",
  sheet: "#0F9D58",
  image: "#E8710A",
  video: "#A142F4",
  inputBg: "#F1F3F4",
  overlay: "rgba(0,0,0,0.4)",
  sectionHeader: "#1967D2",
  pillBorder: "#DADCE0",
};

/** @deprecated Prefer useTheme().colors — kept as dark default for non-React modules. */
export const colors = darkColors;

export const spacing = {
  xs: 4,
  sm: 8,
  md: 12,
  lg: 16,
  xl: 24,
  xxl: 32,
};

export const radii = {
  sm: 8,
  md: 12,
  lg: 16,
  pill: 999,
};
