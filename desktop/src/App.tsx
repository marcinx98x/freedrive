import { useCallback, useEffect, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { api } from "./api/tauri";
import { ForceChangePassword } from "./screens/ForceChangePassword";
import { MainApp } from "./screens/MainApp";
import { OnboardingWizard } from "./screens/OnboardingWizard";
import { PreferencesApp } from "./screens/PreferencesApp";
import { SignIn } from "./screens/SignIn";
import { Welcome } from "./screens/Welcome";
import type { AppScreen, User } from "./types";
import "./styles/drive-theme.css";

function MainShell() {
  const [screen, setScreen] = useState<AppScreen>("loading");
  const [user, setUser] = useState<User | null>(null);
  const [serverUrl, setServerUrl] = useState<string | null>(null);

  const goAfterAuth = (profile: User | null, onboardingComplete: boolean) => {
    if (profile?.must_change_password) {
      setScreen("force_password");
      return;
    }
    if (!onboardingComplete) {
      setScreen("welcome");
    } else {
      setScreen("main");
    }
  };

  const bootstrap = useCallback(async () => {
    try {
      const auth = await Promise.race([
        api.getAuthState(),
        new Promise<never>((_, reject) =>
          setTimeout(() => reject(new Error("bootstrap timeout")), 8000),
        ),
      ]);
      setServerUrl(auth.server_url);
      if (!auth.logged_in) {
        setScreen("signin");
        return;
      }
      setUser(auth.user);
      if (auth.user?.must_change_password) {
        setScreen("force_password");
        return;
      }
      if (!auth.onboarding_complete) {
        setScreen("welcome");
      } else {
        setScreen("main");
        if (auth.logged_in) {
          api.getProfile().then((profile) => {
            setUser(profile);
            if (profile.must_change_password) setScreen("force_password");
          }).catch(() => {});
        }
      }
    } catch {
      setScreen("signin");
    }
  }, []);

  useEffect(() => {
    bootstrap();
  }, [bootstrap]);

  const handleLoginSuccess = async () => {
    const auth = await api.getAuthState();
    setUser(auth.user);
    setServerUrl(auth.server_url);
    let profile = auth.user;
    if (auth.logged_in) {
      try {
        profile = await api.getProfile();
        setUser(profile);
      } catch {
        /* use auth user */
      }
    }
    goAfterAuth(profile, auth.onboarding_complete);
  };

  const handleForcePasswordSuccess = async () => {
    try {
      const profile = await api.getProfile();
      setUser(profile);
      const auth = await api.getAuthState();
      goAfterAuth(profile, auth.onboarding_complete);
    } catch {
      setScreen("main");
    }
  };

  const handleLogout = async () => {
    setScreen("loading");
    try {
      await api.logout();
    } catch {
      /* ignore */
    }
    await bootstrap();
  };

  if (screen === "loading") {
    return <div className="loading-screen">Loading FreeDrive…</div>;
  }

  return (
    <div className="app-shell">
      {screen === "signin" && (
        <SignIn
          defaultServerUrl={serverUrl || undefined}
          onSuccess={handleLoginSuccess}
        />
      )}
      {screen === "force_password" && (
        <ForceChangePassword
          onSuccess={() => void handleForcePasswordSuccess()}
          onLogout={() => void handleLogout()}
        />
      )}
      {screen === "welcome" && (
        <Welcome onGetStarted={() => setScreen("wizard")} />
      )}
      {screen === "wizard" && (
        <OnboardingWizard onComplete={() => setScreen("main")} />
      )}
      {screen === "main" && (
        <MainApp
          user={user}
          serverUrl={serverUrl}
          onLogout={handleLogout}
          onUserUpdate={setUser}
        />
      )}
    </div>
  );
}

function App() {
  const [windowLabel, setWindowLabel] = useState<string | null>(null);

  useEffect(() => {
    setWindowLabel(getCurrentWindow().label);
  }, []);

  if (!windowLabel) {
    return <div className="loading-screen">Loading…</div>;
  }

  if (windowLabel === "preferences") {
    return <PreferencesApp />;
  }

  return <MainShell />;
}

export default App;
