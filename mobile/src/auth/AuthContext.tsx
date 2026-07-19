import React, { createContext, useCallback, useContext, useEffect, useMemo, useState } from "react";
import { api, setUnauthorizedHandler } from "../api/client";
import type { LoginResult, User } from "../api/types";
import { is2FAChallenge } from "../api/types";
import {
  clearSession,
  getServerUrl,
  getUser,
  hasSession,
  setServerUrl,
  setTokens,
  setUser,
} from "../auth/storage";

interface AuthContextValue {
  booting: boolean;
  user: User | null;
  serverUrl: string | null;
  signedIn: boolean;
  login: (serverUrl: string, email: string, password: string) => Promise<LoginResult>;
  verify2FA: (challengeId: string, code: string) => Promise<void>;
  logout: () => Promise<void>;
  refreshProfile: () => Promise<void>;
}

const AuthContext = createContext<AuthContextValue | null>(null);

async function cacheUser(user: User): Promise<void> {
  try {
    await setUser(user);
  } catch {
    // Cache write must never log the user out (avatar data-URLs can be large).
  }
}

export function AuthProvider({ children }: { children: React.ReactNode }) {
  const [booting, setBooting] = useState(true);
  const [user, setUserState] = useState<User | null>(null);
  const [serverUrl, setServerUrlState] = useState<string | null>(null);

  const logout = useCallback(async () => {
    try {
      await api.logout();
    } catch {
      /* ignore */
    }
    await clearSession();
    setUserState(null);
  }, []);

  useEffect(() => {
    setUnauthorizedHandler(() => {
      setUserState(null);
    });
    return () => setUnauthorizedHandler(null);
  }, []);

  useEffect(() => {
    (async () => {
      try {
        const url = await getServerUrl();
        setServerUrlState(url);
        if (await hasSession()) {
          const cached = await getUser();
          if (cached) setUserState(cached);
        }
      } finally {
        // Unblock UI immediately from local storage; refresh profile in background.
        setBooting(false);
      }

      if (await hasSession()) {
        try {
          const me = await api.me();
          setUserState(me);
          await cacheUser(me);
        } catch {
          // Network/timeout: keep cached session. 401 is handled by request()
          // (clearSession + onUnauthorized).
        }
      }
    })();
  }, []);

  const login = useCallback(async (url: string, email: string, password: string) => {
    await setServerUrl(url);
    setServerUrlState(url.replace(/\/$/, ""));
    const result = await api.login(email.trim().toLowerCase(), password);
    if (!is2FAChallenge(result)) {
      await setTokens(result.tokens);
      setUserState(result.user);
      await cacheUser(result.user);
    }
    return result;
  }, []);

  const verify2FA = useCallback(async (challengeId: string, code: string) => {
    const result = await api.verify2FA(challengeId, code);
    await setTokens(result.tokens);
    setUserState(result.user);
    await cacheUser(result.user);
  }, []);

  const refreshProfile = useCallback(async () => {
    const me = await api.me();
    setUserState(me);
    await cacheUser(me);
  }, []);

  const value = useMemo(
    () => ({
      booting,
      user,
      serverUrl,
      signedIn: Boolean(user),
      login,
      verify2FA,
      logout,
      refreshProfile,
    }),
    [booting, user, serverUrl, login, verify2FA, logout, refreshProfile],
  );

  return <AuthContext.Provider value={value}>{children}</AuthContext.Provider>;
}

export function useAuth() {
  const ctx = useContext(AuthContext);
  if (!ctx) throw new Error("useAuth must be used within AuthProvider");
  return ctx;
}
