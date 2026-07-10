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
        <p>
          Browse My Drive from File Explorer. Stream and mirror settings below apply only to{" "}
          <strong>My Drive</strong> in your FreeDrive folder.
        </p>
        <p className="preferences-section-note">
          Folders you add for sync (Documents, Downloads, etc.) upload copies to the cloud only.
          They are not shown inside FreeDrive.
        </p>
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
              <li>Access files from My Drive in File Explorer as cloud placeholders.</li>
              <li>Files download when you open them.</li>
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
              <li>Keep a full local copy of My Drive under FreeDrive.</li>
              <li>Access files from the My Drive folder on your computer.</li>
              <li>New and changed files on the server are downloaded automatically.</li>
            </ul>
          </div>
        </label>
      </div>

      <div className="preferences-info-box">
        <span className="preferences-info-icon" aria-hidden>
          i
        </span>
        <p>
          Streaming uses less disk space. Mirror mode keeps My Drive files offline in{" "}
          ~/FreeDrive/My Drive.
        </p>
      </div>
    </div>
  );
}
