import { useEffect, useState } from "react";
import { api } from "../api/tauri";
import type { SyncMode } from "../types";

export function FreeDriveTab() {
  const [mode, setMode] = useState<SyncMode>("mirror");
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState("");

  useEffect(() => {
    api
      .getSyncMode()
      .then((value) => setMode(value === "stream" ? "stream" : "mirror"))
      .catch((err) => setError(String(err)))
      .finally(() => setLoading(false));
  }, []);

  const handleChange = async (next: SyncMode) => {
    if (next === mode || saving) return;
    setError("");
    setSaving(true);
    try {
      await api.setSyncMode(next);
      setMode(next);
    } catch (err) {
      setError(String(err));
    } finally {
      setSaving(false);
    }
  };

  if (loading) {
    return <div className="preferences-loading">Loading sync options…</div>;
  }

  return (
    <div className="freedrive-tab">
      <div className="preferences-section-header">
        <h2>FreeDrive</h2>
        <p>Browse and open FreeDrive from your computer.</p>
        <button
          type="button"
          className="btn-secondary preferences-open-explorer"
          onClick={() => api.openDriveFolder().catch((err) => setError(String(err)))}
        >
          Open in File Explorer
        </button>
      </div>

      {error && <div className="error-banner">{error}</div>}

      <div className="sync-mode-options">
        <label className={`sync-mode-card${mode === "stream" ? " selected" : ""}`}>
          <input
            type="radio"
            name="sync-mode"
            value="stream"
            checked={mode === "stream"}
            disabled={saving}
            onChange={() => handleChange("stream")}
          />
          <div className="sync-mode-card-body">
            <span className="sync-mode-title">Stream files</span>
            <ul className="sync-mode-features">
              <li>Store all My Drive files in the cloud only.</li>
              <li>Access files from a virtual drive or folder on your computer.</li>
              <li>Choose specific files and folders to make available offline.</li>
            </ul>
          </div>
        </label>

        <label className={`sync-mode-card${mode === "mirror" ? " selected" : ""}`}>
          <input
            type="radio"
            name="sync-mode"
            value="mirror"
            checked={mode === "mirror"}
            disabled={saving}
            onChange={() => handleChange("mirror")}
          />
          <div className="sync-mode-card-body">
            <span className="sync-mode-title">Mirror files</span>
            <ul className="sync-mode-features">
              <li>Store synced files in the cloud and on your computer.</li>
              <li>Access files from a folder on your computer.</li>
              <li>Synced folders are automatically available offline in ~/FreeDrive.</li>
            </ul>
          </div>
        </label>
      </div>

      <div className="preferences-info-box">
        <span className="preferences-info-icon" aria-hidden>
          i
        </span>
        <p>
          Streaming uses less hard drive space. Mirror mode downloads synced files to your local
          FreeDrive folder for offline access.
        </p>
      </div>
    </div>
  );
}
