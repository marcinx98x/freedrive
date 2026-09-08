import AsyncStorage from "@react-native-async-storage/async-storage";

export type ThemePreference = "system" | "light" | "dark";

const THEME_KEY = "fd_theme";
const WIFI_ONLY_KEY = "fd_wifi_only";

export async function getThemePreference(): Promise<ThemePreference> {
  try {
    const v = await AsyncStorage.getItem(THEME_KEY);
    if (v === "light" || v === "dark" || v === "system") return v;
  } catch {
    /* ignore */
  }
  return "system";
}

export async function setThemePreference(value: ThemePreference): Promise<void> {
  await AsyncStorage.setItem(THEME_KEY, value);
}

export async function getWifiOnly(): Promise<boolean> {
  try {
    const v = await AsyncStorage.getItem(WIFI_ONLY_KEY);
    return v === "true";
  } catch {
    return false;
  }
}

export async function setWifiOnly(value: boolean): Promise<void> {
  await AsyncStorage.setItem(WIFI_ONLY_KEY, value ? "true" : "false");
}

export function themePreferenceLabel(pref: ThemePreference): string {
  switch (pref) {
    case "light":
      return "Light";
    case "dark":
      return "Dark";
    default:
      return "System default";
  }
}
