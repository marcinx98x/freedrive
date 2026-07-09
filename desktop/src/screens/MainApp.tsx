import { useCallback, useEffect, useState } from "react";
import { api, onCryptoRecoverySetup, onMyDriveHydrateFailed, onSyncActivity, onSyncProgress, onSyncStatusChanged } from "../api/tauri";
import { ProfileMenu } from "../components/ProfileMenu";
import { Sidebar } from "../components/Sidebar";
import { TopBar } from "../components/TopBar";
import { useNotifications } from "../hooks/useNotifications";
import { Home } from "./Home";
import { Notifications } from "./Notifications";
import { SyncActivity } from "./SyncActivity";
import type { ActivityItem, MainView, StorageInfo, SyncStatus, User } from "../types";

interface MainAppProps {
  user: User | null;
  serverUrl: string | null;
  onLogout: () => void;
  onUserUpdate: (user: User | null) => void;
}

const defaultStatus: SyncStatus = {
  status: "up_to_date",
  message: "Up to date",
  last_synced_at: null,
  paused: false,
};

export function MainApp({ user, serverUrl, onLogout, onUserUpdate }: MainAppProps) {
  const [view, setView] = useState<MainView>("home");
  const [syncStatus, setSyncStatus] = useState<SyncStatus>(defaultStatus);
  const [activity, setActivity] = useState<ActivityItem[]>([]);
  const [storageInfo, setStorageInfo] = useState<StorageInfo | null>(null);
  const [search, setSearch] = useState("");
  const [showSettings, setShowSettings] = useState(false);
  const [profileOpen, setProfileOpen] = useState(false);
  const [profileAnchor, setProfileAnchor] = useState<DOMRect | null>(null);
  const [settingsError, setSettingsError] = useState("");
  const [keysImportMessage, setKeysImportMessage] = useState("");
  const [keysImporting, setKeysImporting] = useState(false);
  const [keysExporting, setKeysExporting] = useState(false);
  const [hydrateWarning, setHydrateWarning] = useState("");
  const [folderError, setFolderError] = useState("");
  const [explorerWarning, setExplorerWarning] = useState("");
  const [signingOut, setSigningOut] = useState(false);

  const refreshProfile = useCallback(async () => {
    try {
      const profile = await api.getProfile();
      onUserUpdate(profile);
    } catch {
      /* profile fetch optional */
    }
  }, [onUserUpdate]);

  const refreshStorage = useCallback(async () => {
    try {
      const storage = await api.getStorageInfo();
      setStorageInfo(storage);
    } catch {
      /* storage optional */
    }
  }, []);

  const refresh = useCallback(async () => {
    try {
      const status = await api.getSyncStatus();
      setSyncStatus(status);
    } catch {
      /* sync engine may not be ready */
    }
    try {
      const items = await api.getSyncActivity();
      setActivity(items);
    } catch {
      /* activity db may not be ready */
    }
    refreshStorage();
  }, [refreshStorage]);

  const {
    notifications,
    badgeCount,
    dismiss,
    markAllSeen,
    setDontShowFor,
    dontShowChecked,
  } = useNotifications(syncStatus, activity, storageInfo);

  useEffect(() => {
    refreshProfile();
    refresh();
    api
      .getExplorerIntegrationStatus()
      .then((status) => {
        if (status.registered && !status.connected) {
          setExplorerWarning(
            "File Explorer integration is disconnected. Use Open Drive folder to reconnect.",
          );
        }
      })
      .catch(() => {
        /* optional status */
      });
    const unsubs: (() => void)[] = [];
    onSyncStatusChanged(setSyncStatus).then((u) => unsubs.push(u));
    onSyncProgress((progress) => {
      if (progress.phase === "scanning") return;
      if (progress.show_in_ui === false) return;
      if (!progress.message || progress.message.startsWith("Scanning complete")) return;
      if (progress.phase === "syncing") {
        setSyncStatus((prev) => ({
          ...prev,
          status: "syncing",
          message: progress.message,
        }));
      }
    }).then((u) => unsubs.push(u));
    onSyncActivity((item) => {
      setActivity((prev) => {
        const name = item.name || "File";
        const row: ActivityItem = {
          id: typeof item.id === "number" ? item.id : Date.now(),
          name,
          detail: item.detail || "",
          file_size: item.file_size || 0,
          status: item.status || "uploading",
          created_at: new Date().toISOString(),
        };
        const idx = prev.findIndex((a) => a.name === name);
        if (idx >= 0) {
          const next = [...prev];
          next[idx] = { ...row, id: prev[idx].id };
          return next;
        }
        return [row, ...prev].slice(0, 50);
      });
    }).then((u) => unsubs.push(u));
    onMyDriveHydrateFailed((event) => {
      const isWebKey =
        event.message.includes("uploaded via web") ||
        event.message.includes("encryption key not available");
      setHydrateWarning(
        isWebKey
          ? "Could not decrypt this file yet. Sign out and sign in again to sync encryption keys, or use Settings export/import as backup."
          : event.message,
      );
    }).then((u) => unsubs.push(u));
    onCryptoRecoverySetup((code) => {
      setKeysImportMessage(
        `Save this recovery code in a safe place: ${code}`,
      );
      setShowSettings(true);
    }).then((u) => unsubs.push(u));
    const interval = setInterval(refresh, 10000);
    return () => {
      unsubs.forEach((u) => u());
      clearInterval(interval);
    };
  }, [refresh, refreshProfile]);

  useEffect(() => {
    if (view === "notifications") {
      markAllSeen();
    }
  }, [view, markAllSeen]);

  const handlePauseResume = async () => {
    try {
      if (syncStatus.paused) {
        await api.resumeSync();
      } else {
        await api.pauseSync();
      }
      refresh();
    } catch (err) {
      console.error("pause/resume failed:", err);
    }
  };

  const handleExportEncryptionKeys = async () => {
    setSettingsError("");
    setKeysImportMessage("");
    setKeysExporting(true);
    try {
      const result = await api.exportEncryptionKeys();
      setKeysImportMessage(
        `Exported ${result.exported} encryption key${result.exported === 1 ? "" : "s"} to ${result.path}.`,
      );
    } catch (err) {
      const message = String(err);
      if (!message.includes("Export cancelled")) {
        setSettingsError(message);
      }
    } finally {
      setKeysExporting(false);
    }
  };

  const handleImportEncryptionKeys = async () => {
    setSettingsError("");
    setKeysImportMessage("");
    setKeysImporting(true);
    try {
      const result = await api.importEncryptionKeys();
      setKeysImportMessage(`Imported ${result.imported} encryption key${result.imported === 1 ? "" : "s"}.`);
      setHydrateWarning("");
    } catch (err) {
      const message = String(err);
      if (!message.includes("No file selected")) {
        setSettingsError(message);
      }
    } finally {
      setKeysImporting(false);
    }
  };

  const handleSignOut = async () => {
    setSettingsError("");
    setSigningOut(true);
    try {
      await api.logout();
      setShowSettings(false);
      setProfileOpen(false);
      onLogout();
    } catch (err) {
      setSettingsError(String(err));
    } finally {
      setSigningOut(false);
    }
  };

  const handleSignInAnother = async () => {
    setProfileOpen(false);
    await handleSignOut();
  };

  const handleOpenDriveFolder = async () => {
    setFolderError("");
    setExplorerWarning("");
    try {
      await api.openDriveFolder();
    } catch (err) {
      const message = String(err);
      setFolderError(message);
      setExplorerWarning(message);
    }
  };

  return (
    <div className="main-layout">
      <Sidebar
        view={view}
        notificationCount={badgeCount}
        onNavigate={setView}
        onOpenFolder={handleOpenDriveFolder}
      />
      <div className="main-content">
        {(folderError || explorerWarning || hydrateWarning) && (
          <div className="error-banner" style={{ margin: "8px 16px 0" }}>
            {hydrateWarning || folderError || explorerWarning}
          </div>
        )}
        <TopBar
          user={user}
          syncStatus={syncStatus}
          search={search}
          onSearchChange={setSearch}
          onPauseResume={handlePauseResume}
          onOpenSettings={() => {
            setSettingsError("");
            setKeysImportMessage("");
            setShowSettings(true);
          }}
          onProfileClick={(rect) => {
            setProfileAnchor(rect);
            setProfileOpen((open) => !open);
          }}
        />
        <div className="content-area">
          {view === "home" && (
            <Home
              syncStatus={syncStatus}
              activity={activity}
              notifications={notifications}
              onDismiss={dismiss}
              onGoToNotifications={() => setView("notifications")}
              onResumeSync={handlePauseResume}
              onFoldersChanged={refresh}
            />
          )}
          {view === "sync" && (
            <SyncActivity
              syncStatus={syncStatus}
              activity={activity}
              search={search}
            />
          )}
          {view === "notifications" && (
            <Notifications
              notifications={notifications}
              dontShowChecked={dontShowChecked}
              onDismiss={dismiss}
              onDontShowChange={setDontShowFor}
              onResumeSync={handlePauseResume}
            />
          )}
        </div>
      </div>

      {profileOpen && (
        <ProfileMenu
          user={user}
          serverUrl={serverUrl}
          anchorRect={profileAnchor}
          onClose={() => setProfileOpen(false)}
          onSignOut={handleSignOut}
          onSignInAnother={handleSignInAnother}
        />
      )}

      {showSettings && (
        <div className="settings-overlay" onClick={() => setShowSettings(false)}>
          <div className="settings-modal" onClick={(e) => e.stopPropagation()}>
            <h2>Settings</h2>
            {settingsError && <div className="error-banner">{settingsError}</div>}
            <div className="form-group">
              <label>Server URL</label>
              <input type="text" value={serverUrl || ""} readOnly />
            </div>
            <div className="form-group">
              <label>Account</label>
              <input type="text" value={user?.email || ""} readOnly />
            </div>
            <div className="form-group">
              <label>Encryption keys (E2E)</label>
              <p className="settings-hint">
                Encryption keys sync automatically when you sign in. Use export/import below only
                as a backup if you need to move keys manually.
              </p>
              {keysImportMessage && (
                <div className="success-banner">{keysImportMessage}</div>
              )}
              <div className="settings-actions" style={{ marginBottom: 0, justifyContent: "flex-start", gap: 8 }}>
                <button
                  type="button"
                  className="btn-secondary"
                  onClick={handleExportEncryptionKeys}
                  disabled={keysExporting || keysImporting}
                >
                  {keysExporting ? "Exporting…" : "Export encryption keys…"}
                </button>
                <button
                  type="button"
                  className="btn-secondary"
                  onClick={handleImportEncryptionKeys}
                  disabled={keysImporting || keysExporting}
                >
                  {keysImporting ? "Importing…" : "Import encryption keys…"}
                </button>
              </div>
            </div>
            <div className="settings-actions">
              <button type="button" className="btn-secondary" onClick={() => setShowSettings(false)}>
                Close
              </button>
              <button type="button" className="btn-primary" onClick={handleSignOut} disabled={signingOut}>
                {signingOut ? "Signing out…" : "Sign out"}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

