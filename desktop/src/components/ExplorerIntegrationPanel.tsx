import { useEffect, useState } from "react";
import { api } from "../api/tauri";
import type { ExplorerIntegrationStatus } from "../types";

interface ExplorerIntegrationPanelProps {
  onBack?: () => void;
  embedded?: boolean;
}

export function ExplorerIntegrationPanel({
  onBack,
  embedded = false,
}: ExplorerIntegrationPanelProps) {
  const [status, setStatus] = useState<ExplorerIntegrationStatus | null>(null);
  const [error, setError] = useState("");

  useEffect(() => {
    api
      .getExplorerIntegrationStatus()
      .then(setStatus)
      .catch((err) => setError(String(err)));
  }, []);

  return (
    <div className={`explorer-integration-panel${embedded ? " settings-panel-embedded" : ""}`}>
      {!embedded && onBack && (
        <button type="button" className="preferences-back-btn" onClick={onBack}>
          ← Settings
        </button>
      )}
      {!embedded && <h2>Explorer integration</h2>}
      {error && <div className="error-banner">{error}</div>}
      {status && (
        <>
          <ul className="explorer-status-list">
            <li>
              <span>Connected</span>
              <span>{status.connected ? "Yes" : "No"}</span>
            </li>
            <li>
              <span>Registered</span>
              <span>{status.registered ? "Yes" : "No"}</span>
            </li>
            <li>
              <span>Sync root</span>
              <span className="settings-info-value">{status.sync_root_path}</span>
            </li>
            <li>
              <span>My Drive path</span>
              <span className="settings-info-value">{status.my_drive_path}</span>
            </li>
          </ul>
          <p className="settings-hint">
            In File Explorer open My Drive in Details view and enable the{" "}
            <strong>Status</strong> column (More… → Status / StorageProviderUIStatus).
            Icons are Windows cloud / check / sync glyphs — FreeDrive does not add custom Status icons.
          </p>
          <button
            type="button"
            className="btn-secondary"
            onClick={() => api.openDriveFolder().catch((err) => setError(String(err)))}
          >
            Open My Drive folder
          </button>
        </>
      )}
    </div>
  );
}
