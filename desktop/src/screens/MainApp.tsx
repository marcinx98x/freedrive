import { useCallback, useEffect, useState } from "react";
import { api, onSyncActivity, onSyncProgress, onSyncStatusChanged } from "../api/tauri";
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
        {(folderError || explorerWarning) && (
          <div className="error-banner" style={{ margin: "8px 16px 0" }}>
            {folderError || explorerWarning}
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

