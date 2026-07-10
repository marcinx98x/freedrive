import { useCallback, useEffect, useRef, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  api,
  onCryptoKeysSynced,
  onCryptoRecoverySetup,
  onCryptoUnlocked,
} from "../api/tauri";
import { EncryptionSettingsPanel } from "../components/EncryptionSettingsPanel";
import { ExplorerIntegrationPanel } from "../components/ExplorerIntegrationPanel";
import { FreeDriveTab } from "../components/FreeDriveTab";
import { MyComputerTab } from "../components/MyComputerTab";
import {
  PreferencesHeader,
  type PreferencesView,
} from "../components/PreferencesHeader";
import {
  PreferencesSettingsPage,
  type SettingsSubPage,
} from "../components/PreferencesSettingsPage";
import { PreferencesSidebar } from "../components/PreferencesSidebar";
import { ProfileMenu } from "../components/ProfileMenu";
import { useEncryptionSettings } from "../hooks/useEncryptionSettings";
import type { PreferencesTab, User } from "../types";

export function PreferencesApp() {
  const [user, setUser] = useState<User | null>(null);
  const [serverUrl, setServerUrl] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [preferencesView, setPreferencesView] = useState<PreferencesView>("sync");
  const [activeTab, setActiveTab] = useState<PreferencesTab>("my-computer");
  const [settingsSubPage, setSettingsSubPage] = useState<SettingsSubPage | null>(null);
  const [profileOpen, setProfileOpen] = useState(false);
  const [profileAnchor, setProfileAnchor] = useState<DOMRect | null>(null);
  const [launchOnLogin, setLaunchOnLogin] = useState(false);
  const [bootstrapError, setBootstrapError] = useState("");

  const avatarButtonRef = useRef<HTMLButtonElement>(null);

  const encryption = useEncryptionSettings();
  const {
    refreshCryptoStatus,
    setKeysImportMessage,
    setCryptoUnlocked,
    setCryptoUnlockError,
    setSettingsError,
  } = encryption;

  const bootstrap = useCallback(async () => {
    setBootstrapError("");
    try {
      const auth = await api.getAuthState();
      if (!auth.logged_in) {
        setBootstrapError("Sign in from the main FreeDrive window to use preferences.");
        return;
      }
      setUser(auth.user);
      setServerUrl(auth.server_url);
      api.getProfile().then(setUser).catch(() => {});
      const launch = await api.getLaunchOnLogin().catch(() => false);
      setLaunchOnLogin(launch);
      await refreshCryptoStatus();
    } catch (err) {
      setBootstrapError(String(err));
    } finally {
      setLoading(false);
    }
  }, [refreshCryptoStatus]);

  useEffect(() => {
    bootstrap();
  }, [bootstrap]);

  useEffect(() => {
    const unsubs: (() => void)[] = [];
    onCryptoRecoverySetup((code) => {
      setKeysImportMessage(`Save this recovery code in a safe place: ${code}`);
      setCryptoUnlocked(true);
      setPreferencesView("settings");
      setSettingsSubPage("encryption");
    }).then((u) => unsubs.push(u));
    onCryptoKeysSynced((stats) => {
      const total = stats.pulled + stats.pushed + stats.pending_flushed;
      if (total > 0) {
        setKeysImportMessage(
          `Synced ${total} encryption key${total === 1 ? "" : "s"}.`,
        );
        setCryptoUnlocked(true);
        setCryptoUnlockError("");
      }
    }).then((u) => unsubs.push(u));
    onCryptoUnlocked(() => {
      setCryptoUnlocked(true);
      setCryptoUnlockError("");
      refreshCryptoStatus();
    }).then((u) => unsubs.push(u));
    return () => unsubs.forEach((u) => u());
  }, [
    refreshCryptoStatus,
    setKeysImportMessage,
    setCryptoUnlocked,
    setCryptoUnlockError,
  ]);

  const handleDone = async () => {
    await getCurrentWindow().hide();
  };

  const handleSignOut = async () => {
    try {
      await api.logout();
      setProfileOpen(false);
      await getCurrentWindow().hide();
    } catch (err) {
      setSettingsError(String(err));
    }
  };

  const handleLaunchOnLoginChange = async (enabled: boolean) => {
    try {
      await api.setLaunchOnLogin(enabled);
      setLaunchOnLogin(enabled);
    } catch (err) {
      setSettingsError(String(err));
    }
  };

  const handleOpenSettings = () => {
    setSettingsSubPage(null);
    setPreferencesView("settings");
  };

  const handleBackToSync = () => {
    setSettingsSubPage(null);
    setPreferencesView("sync");
  };

  if (loading) {
    return <div className="loading-screen">Loading preferences…</div>;
  }

  if (bootstrapError && !user) {
    return (
      <div className="preferences-window">
        <div className="preferences-content" style={{ padding: 32 }}>
          <div className="error-banner">{bootstrapError}</div>
          <button type="button" className="btn-primary" style={{ marginTop: 16 }} onClick={handleDone}>
            Close
          </button>
        </div>
      </div>
    );
  }

  return (
    <div className="preferences-window">
      <PreferencesHeader
        user={user}
        preferencesView={preferencesView}
        avatarButtonRef={avatarButtonRef}
        onHelp={() =>
          api.openServerUrl().catch(() => {
            /* optional */
          })
        }
        onOpenSettings={handleOpenSettings}
        onProfileClick={(rect) => {
          setProfileAnchor(rect);
          setProfileOpen((open) => !open);
        }}
      />

      <div className="preferences-body">
        {preferencesView === "sync" ? (
          <>
            <PreferencesSidebar activeTab={activeTab} onTabChange={setActiveTab} />
            <main className="preferences-content">
              {bootstrapError && <div className="error-banner">{bootstrapError}</div>}
              {encryption.settingsError && (
                <div className="error-banner">{encryption.settingsError}</div>
              )}
              {activeTab === "my-computer" ? <MyComputerTab /> : <FreeDriveTab />}
            </main>
          </>
        ) : (
          <main className="preferences-content preferences-content-full">
            {encryption.settingsError && (
              <div className="error-banner">{encryption.settingsError}</div>
            )}
            {settingsSubPage === null ? (
              <PreferencesSettingsPage
                serverUrl={serverUrl}
                launchOnLogin={launchOnLogin}
                onBackToSync={handleBackToSync}
                onLaunchOnLoginChange={handleLaunchOnLoginChange}
                onOpenSubPage={setSettingsSubPage}
              />
            ) : settingsSubPage === "encryption" ? (
              <EncryptionSettingsPanel
                serverUrl={serverUrl}
                settingsError={encryption.settingsError}
                keysImportMessage={encryption.keysImportMessage}
                keysImporting={encryption.keysImporting}
                keysExporting={encryption.keysExporting}
                cryptoUnlocked={encryption.cryptoUnlocked}
                serverHasCrypto={encryption.serverHasCrypto}
                cryptoUnlockError={encryption.cryptoUnlockError}
                needsCryptoRecovery={encryption.needsCryptoRecovery}
                recoveryCode={encryption.recoveryCode}
                recoveryUnlocking={encryption.recoveryUnlocking}
                rotatePassword={encryption.rotatePassword}
                rotatingKey={encryption.rotatingKey}
                onRecoveryCodeChange={encryption.setRecoveryCode}
                onRotatePasswordChange={encryption.setRotatePassword}
                onUnlockRecovery={encryption.handleUnlockRecovery}
                onRotateCryptoKey={encryption.handleRotateCryptoKey}
                onExportKeys={encryption.handleExportEncryptionKeys}
                onImportKeys={encryption.handleImportEncryptionKeys}
                onBack={() => setSettingsSubPage(null)}
              />
            ) : (
              <ExplorerIntegrationPanel onBack={() => setSettingsSubPage(null)} />
            )}
          </main>
        )}
      </div>

      <footer className="preferences-footer">
        <button type="button" className="btn-primary" onClick={handleDone}>
          Done
        </button>
      </footer>

      {profileOpen && (
        <ProfileMenu
          user={user}
          serverUrl={serverUrl}
          anchorRect={profileAnchor}
          onClose={() => setProfileOpen(false)}
          onSignOut={handleSignOut}
        />
      )}
    </div>
  );
}
