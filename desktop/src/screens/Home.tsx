import { useState } from "react";
import { api, formatRelativeTime } from "../api/tauri";
import type { ActivityItem, SyncStatus } from "../types";

interface HomeProps {
  syncStatus: SyncStatus;
  activity: ActivityItem[];
  onViewAll: () => void;
  onFoldersChanged: () => void;
}

export function Home({ syncStatus, activity, onViewAll, onFoldersChanged }: HomeProps) {
  const [addingFolder, setAddingFolder] = useState(false);
  const recent = activity.slice(0, 3);
  const isError = syncStatus.status === "error";
  const isSyncing = syncStatus.status === "syncing";
  const isUpToDate =
    syncStatus.status === "up_to_date" || syncStatus.status === "paused";

  const statusTitle = syncStatus.paused
    ? "Sync paused"
    : isError
      ? syncStatus.message
      : isSyncing
        ? syncStatus.message
        : isUpToDate
          ? "Up to date"
          : syncStatus.message;

  const handleAddFolder = async () => {
    if (addingFolder) return;
    setAddingFolder(true);
    try {
      const path = await Promise.race([
        api.pickFolder(),
        new Promise<null>((resolve) => setTimeout(() => resolve(null), 120_000)),
      ]);
      if (!path) return;
      await api.addSyncFolder(path);
      onFoldersChanged();
    } catch (err) {
      console.error("add folder failed:", err);
    } finally {
      setAddingFolder(false);
    }
  };

  return (
    <div className="home-grid">
      <div className="card">
        <div className="status-card-header">
          <div className="status-icon">
            {isError ? "!" : isSyncing ? "↻" : isUpToDate ? "☁" : "↻"}
          </div>
          <div style={{ flex: 1 }}>
            <div className={`status-title${isError ? " status-error" : ""}`}>
              {statusTitle}
            </div>
            <div className="status-subtitle">
              {isSyncing
                ? syncStatus.message || "Sync in progress…"
                : `Synced ${formatRelativeTime(syncStatus.last_synced_at)}`}
            </div>
          </div>
        </div>

        {isError && (
          <button
            type="button"
            className="btn-secondary"
            style={{ marginBottom: 12 }}
            onClick={() => api.resumeSync()}
          >
            Retry sync
          </button>
        )}

        <div className="activity-mini-list">
          {recent.length === 0 ? (
            <div className="empty-state" style={{ padding: 16 }}>
              No recent activity yet
            </div>
          ) : (
            recent.map((item) => (
              <div key={item.id} className="activity-mini-item">
                <span>📄</span>
                <div style={{ flex: 1 }}>
                  <div className="name">{item.name}</div>
                  <div
                    className={`detail${item.status === "error" ? " detail-error" : ""}`}
                  >
                    {item.detail}
                  </div>
                </div>
                {item.status === "synced" && <span className="status-check">✓</span>}
                {item.status === "error" && <span className="status-error-mark">✕</span>}
              </div>
            ))
          )}
        </div>
        <button type="button" className="btn-primary" onClick={onViewAll}>
          View all
        </button>
      </div>

      <div className="card caught-up-card">
        <h3 className="status-title" style={{ fontSize: 18, marginBottom: 8 }}>
          {isError ? "Sync needs attention" : isSyncing ? "Sync in progress" : "You're all caught up!"}
        </h3>
        <p className="caught-up-text">
          {isError
            ? "Some files could not be synced. Use Retry sync or check Sync activity for details."
            : isSyncing
              ? "Files are being uploaded. Check Sync activity for details."
              : "Things that need your attention will show here."}
        </p>
      </div>

      <div className="card" style={{ gridColumn: "2" }}>
        <h3 style={{ fontSize: 16, marginBottom: 12 }}>Quick links</h3>
        <div className="quick-links">
          <button
            type="button"
            className="quick-link"
            onClick={handleAddFolder}
            disabled={addingFolder}
          >
            + Add more folders to sync
          </button>
          <button type="button" className="quick-link" onClick={() => api.openServerUrl().catch(console.error)}>
            Open Drive web ↗
          </button>
          <button type="button" className="quick-link" onClick={() => api.openServerUrl().catch(console.error)}>
            Learn more about offline files ↗
          </button>
          <button type="button" className="quick-link" onClick={() => api.openServerUrl().catch(console.error)}>
            Frequently asked questions ↗
          </button>
        </div>
      </div>
    </div>
  );
}
