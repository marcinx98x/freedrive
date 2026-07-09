import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type {
  ActivityItem,
  AuthState,
  ExplorerIntegrationStatus,
  LoginResult,
  SelectedFolder,
  SharedItem,
  StorageInfo,
  SyncFolder,
  SyncProgress,
  SyncStatus,
  SystemFolder,
  User,
} from "../types";

export const api = {
  getAuthState: () => invoke<AuthState>("get_auth_state"),
  login: (server_url: string, email: string, password: string) =>
    invoke<LoginResult>("login", {
      req: { server_url, email, password },
    }),
  verify2FA: (server_url: string, challenge_id: string, code: string) =>
    invoke<User>("verify_2fa", {
      req: { server_url, challenge_id, code },
    }),
  logout: () => invoke<void>("logout"),
  getSystemFolders: () => invoke<SystemFolder[]>("get_system_folders"),
  pickFolder: () => invoke<string | null>("pick_folder"),
  saveSyncConfig: (folders: SelectedFolder[]) =>
    invoke<void>("save_sync_config", { req: { folders } }),
  completeOnboarding: () => invoke<void>("complete_onboarding"),
  getSyncStatus: () => invoke<SyncStatus>("get_sync_status"),
  getSyncActivity: () => invoke<ActivityItem[]>("get_sync_activity"),
  getSyncFolders: () => invoke<SyncFolder[]>("get_sync_folders"),
  addSyncFolder: (path: string) => invoke<string>("add_sync_folder", { path }),
  pauseSync: () => invoke<void>("pause_sync"),
  resumeSync: () => invoke<void>("resume_sync"),
  openDriveFolder: () => invoke<void>("open_drive_folder"),
  getExplorerIntegrationStatus: () =>
    invoke<ExplorerIntegrationStatus>("get_explorer_integration_status"),
  getProfile: () => invoke<User>("get_profile"),
  getStorageInfo: () => invoke<StorageInfo>("get_storage_info"),
  getSharedWithMe: () => invoke<SharedItem[]>("get_shared_with_me"),
  openServerUrl: (path?: string) => invoke<void>("open_server_url", { path }),
  openPathInExplorer: (path: string) =>
    invoke<void>("open_path_in_explorer", { path }),
};

export function onSyncStatusChanged(cb: (status: SyncStatus) => void) {
  return listen<SyncStatus>("sync-status-changed", (e) => cb(e.payload));
}

export function onSyncActivity(cb: (item: Partial<ActivityItem>) => void) {
  return listen<Partial<ActivityItem>>("sync-activity", (e) => cb(e.payload));
}

export function onSyncProgress(cb: (progress: SyncProgress) => void) {
  return listen<SyncProgress>("sync-progress", (e) => cb(e.payload));
}

export function formatBytes(bytes: number): string {
  if (bytes <= 0) return "—";
  const units = ["B", "KB", "MB", "GB"];
  let i = 0;
  let n = bytes;
  while (n >= 1024 && i < units.length - 1) {
    n /= 1024;
    i++;
  }
  return `${n.toFixed(i > 0 ? 1 : 0)} ${units[i]}`;
}

export function formatRelativeTime(iso: string | null): string {
  if (!iso) return "Never";
  const diff = Date.now() - new Date(iso).getTime();
  const mins = Math.floor(diff / 60000);
  if (mins < 1) return "less than a minute ago";
  if (mins < 60) return `${mins} minute${mins > 1 ? "s" : ""} ago`;
  const hours = Math.floor(mins / 60);
  if (hours < 24) return `${hours} hour${hours > 1 ? "s" : ""} ago`;
  return new Date(iso).toLocaleDateString();
}
