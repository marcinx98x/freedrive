import { formatBytes, formatRelativeTime } from "../api/tauri";
import type { ActivityItem, SyncStatus } from "../types";

interface SyncActivityProps {
  syncStatus: SyncStatus;
  activity: ActivityItem[];
  search: string;
}

function statusLabel(status: string) {
  switch (status) {
    case "synced":
      return { text: "Synced", className: "synced" };
    case "uploading":
      return { text: "Uploading", className: "uploading" };
    case "error":
      return { text: "Error", className: "error" };
    case "skipped":
      return { text: "Skipped", className: "skipped" };
    default:
      return { text: status, className: "" };
  }
}

export function SyncActivity({ syncStatus, activity, search }: SyncActivityProps) {
  const filtered = search
    ? activity.filter((a) => a.name.toLowerCase().includes(search.toLowerCase()))
    : activity;

  const isError = syncStatus.status === "error";
  const isSyncing = syncStatus.status === "syncing";
  const isUpToDate = syncStatus.status === "up_to_date";

  const headerTitle = syncStatus.paused
    ? "Sync paused"
    : isError
      ? syncStatus.message
      : isSyncing
        ? syncStatus.message
        : isUpToDate
          ? "Up to date"
          : syncStatus.message;

  return (
    <div className="sync-panel">
      <div className="sync-panel-header">
        <div className="status-icon">
          {isError ? "!" : isSyncing ? "↻" : isUpToDate ? "☁" : "↻"}
        </div>
        <div style={{ flex: 1 }}>
          <div className={`status-title${isError ? " status-error" : ""}`}>
            {headerTitle}
          </div>
          <div className="status-subtitle">
            {isSyncing
              ? syncStatus.message || "Sync in progress…"
              : `Synced ${formatRelativeTime(syncStatus.last_synced_at)}`}
          </div>
        </div>
      </div>

      <table className="sync-table">
        <thead>
          <tr>
            <th>Name</th>
            <th>File size</th>
            <th>Status</th>
          </tr>
        </thead>
        <tbody>
          {filtered.length === 0 ? (
            <tr>
              <td colSpan={3}>
                <div className="empty-state">No sync activity yet</div>
              </td>
            </tr>
          ) : (
            filtered.map((item) => {
              const st = statusLabel(item.status);
              return (
                <tr key={item.id} className={item.status === "error" ? "row-error" : ""}>
                  <td>
                    <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
                      <span>📄</span>
                      <div>
                        <div>{item.name}</div>
                        <div
                          className={`activity-detail${
                            item.status === "error" ? " detail-error" : ""
                          }`}
                        >
                          {item.detail}
                        </div>
                      </div>
                    </div>
                  </td>
                  <td>{formatBytes(item.file_size)}</td>
                  <td>
                    <span className={`status-badge ${st.className}`}>
                      {st.text === "Synced" && "✓ "}
                      {st.text}
                    </span>
                  </td>
                </tr>
              );
            })
          )}
        </tbody>
      </table>
    </div>
  );
}
