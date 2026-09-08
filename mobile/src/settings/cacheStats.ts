import AsyncStorage from "@react-native-async-storage/async-storage";
import * as FileSystem from "expo-file-system/legacy";
import { clearListCaches } from "../cache/listCache";

const CACHE_PREFIXES = ["fd_", "fd_enc_"];

function isAppCacheName(name: string): boolean {
  return CACHE_PREFIXES.some((p) => name.startsWith(p));
}

async function listCacheFiles(): Promise<Array<{ uri: string; size: number }>> {
  const dir = FileSystem.cacheDirectory;
  if (!dir) return [];
  try {
    const names = await FileSystem.readDirectoryAsync(dir);
    const out: Array<{ uri: string; size: number }> = [];
    for (const name of names) {
      if (!isAppCacheName(name)) continue;
      const uri = `${dir}${name}`;
      try {
        const info = await FileSystem.getInfoAsync(uri);
        if (info.exists && !("isDirectory" in info && info.isDirectory)) {
          const size = "size" in info && typeof info.size === "number" ? info.size : 0;
          out.push({ uri, size });
        }
      } catch {
        /* skip */
      }
    }
    return out;
  } catch {
    return [];
  }
}

/** Bytes used by FreeDrive document/media cache files (+ AsyncStorage list caches are tiny). */
export async function getCacheUsageBytes(): Promise<number> {
  const files = await listCacheFiles();
  return files.reduce((sum, f) => sum + f.size, 0);
}

/**
 * Clears decrypted/encrypted file cache and list caches.
 * Does not touch auth tokens, crypto keys, or settings prefs.
 */
export async function clearAppCache(): Promise<void> {
  const files = await listCacheFiles();
  await Promise.all(
    files.map((f) => FileSystem.deleteAsync(f.uri, { idempotent: true }).catch(() => {})),
  );
  await clearListCaches();

  // Also drop ephemeral activity / view-mode caches that are safe to rebuild.
  try {
    const keys = await AsyncStorage.getAllKeys();
    const drop = keys.filter(
      (k) =>
        k.startsWith("fd_cache_") ||
        k === "fd_home_activity_cache" ||
        k.startsWith("fd_home_view_mode") ||
        k.startsWith("fd_files_view_mode"),
    );
    if (drop.length) await AsyncStorage.multiRemove(drop);
  } catch {
    /* ignore */
  }
}
