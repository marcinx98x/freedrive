import AsyncStorage from "@react-native-async-storage/async-storage";
import * as SecureStore from "expo-secure-store";
import type { TokenPair, User } from "../api/types";

const KEYS = {
  serverUrl: "fd_server_url",
  accessToken: "fd_access_token",
  refreshToken: "fd_refresh_token",
  // Marker string for APK bundle freshness checks.
  userCache: "fd_user_cache",
  // Legacy SecureStore key — cleaned up on clearSession.
  userLegacy: "fd_user",
} as const;

export async function getServerUrl(): Promise<string | null> {
  return SecureStore.getItemAsync(KEYS.serverUrl);
}

export async function setServerUrl(url: string): Promise<void> {
  await SecureStore.setItemAsync(KEYS.serverUrl, url.replace(/\/$/, ""));
}

export async function getTokens(): Promise<TokenPair | null> {
  const access = await SecureStore.getItemAsync(KEYS.accessToken);
  const refresh = await SecureStore.getItemAsync(KEYS.refreshToken);
  if (!access || !refresh) return null;
  return { access_token: access, refresh_token: refresh, expires_in: 0 };
}

export async function setTokens(tokens: TokenPair): Promise<void> {
  await SecureStore.setItemAsync(KEYS.accessToken, tokens.access_token);
  await SecureStore.setItemAsync(KEYS.refreshToken, tokens.refresh_token);
}

export async function getUser(): Promise<User | null> {
  const raw = await AsyncStorage.getItem(KEYS.userCache);
  if (!raw) return null;
  try {
    return JSON.parse(raw) as User;
  } catch {
    return null;
  }
}

export async function setUser(user: User): Promise<void> {
  // Profile (with optional large avatar data-URL) lives in AsyncStorage —
  // SecureStore has a 2048-byte value limit that would reject it.
  await AsyncStorage.setItem(KEYS.userCache, JSON.stringify(user));
}

export async function clearSession(): Promise<void> {
  await SecureStore.deleteItemAsync(KEYS.accessToken);
  await SecureStore.deleteItemAsync(KEYS.refreshToken);
  try {
    await SecureStore.deleteItemAsync(KEYS.userLegacy);
  } catch {
    /* ignore legacy cleanup errors */
  }
  await AsyncStorage.removeItem(KEYS.userCache);
}

export async function hasSession(): Promise<boolean> {
  const tokens = await getTokens();
  return Boolean(tokens?.access_token && tokens?.refresh_token);
}
