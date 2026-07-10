import { api } from "../api/tauri";

export type SettingsSubPage = "encryption" | "explorer";

interface PreferencesSettingsPageProps {
  serverUrl: string | null;
  launchOnLogin: boolean;
  onBackToSync: () => void;
  onLaunchOnLoginChange: (enabled: boolean) => void;
  onOpenSubPage: (page: SettingsSubPage) => void;
}

export function PreferencesSettingsPage({
  serverUrl,
  launchOnLogin,
  onBackToSync,
  onLaunchOnLoginChange,
  onOpenSubPage,
}: PreferencesSettingsPageProps) {
  return (
    <div className="preferences-settings-page">
      <button type="button" className="preferences-settings-back" onClick={onBackToSync}>
        ← FreeDrive
      </button>

      <section className="preferences-settings-section">
        <h3>Launch on login</h3>
        <label className="preferences-settings-checkbox-row">
          <input
            type="checkbox"
            checked={launchOnLogin}
            onChange={(e) => onLaunchOnLoginChange(e.target.checked)}
          />
          <span>Launch FreeDrive when you log in to your computer</span>
        </label>
      </section>

      <section className="preferences-settings-section">
        <h3>Diagnostics</h3>
        <button
          type="button"
          className="preferences-settings-row"
          onClick={() => api.openSyncLogFolder().catch(console.error)}
        >
          <span>Open sync log folder</span>
          <span className="preferences-settings-chevron" aria-hidden>
            ›
          </span>
        </button>
      </section>

      <section className="preferences-settings-section">
        <h3>Security</h3>
        <button
          type="button"
          className="preferences-settings-row"
          onClick={() => onOpenSubPage("encryption")}
        >
          <span>Encryption &amp; keys</span>
          <span className="preferences-settings-chevron" aria-hidden>
            ›
          </span>
        </button>
      </section>

      <section className="preferences-settings-section">
        <h3>File Explorer</h3>
        <button
          type="button"
          className="preferences-settings-row"
          onClick={() => onOpenSubPage("explorer")}
        >
          <span>Explorer integration</span>
          <span className="preferences-settings-chevron" aria-hidden>
            ›
          </span>
        </button>
      </section>

      <section className="preferences-settings-section">
        <h3>Server</h3>
        <p className="settings-info-value preferences-settings-server-url">
          {serverUrl || "—"}
        </p>
      </section>
    </div>
  );
}
