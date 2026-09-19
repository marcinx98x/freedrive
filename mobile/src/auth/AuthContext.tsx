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
import {
  isUnlocked,
  lockAndClearDevice,
  tryRestoreUnlock,
  unlockWithPassword,
} from "../crypto";
import { ensureUnlockedOrPrompt } from "../crypto/ensureUnlocked";

interface AuthContextValue {
  booting: boolean;
  user: User | null;
  serverUrl: string | null;
  signedIn: boolean;
  cryptoUnlocked: boolean;
  cryptoUnlockHint: string | null;
  login: (serverUrl: string, email: string, password: string) => Promise<LoginResult>;
  verify2FA: (challengeId: string, code: string) => Promise<void>;
  logout: () => Promise<void>;
  refreshProfile: () => Promise<void>;
  markCryptoUnlocked: () => void;
  clearCryptoUnlockHint: () => void;
  ensureCryptoUnlocked: (reason?: "upload" | "open" | "generic") => Promise<void>;
}

const AuthContext = createContext<AuthContextValue | null>(null);

/** Held only until 2FA completes so we can unlock crypto with the same password. */
let pendingLoginPassword: string | null = null;

const UNLOCK_HINT =
  "Encryption is locked. Enter your password to upload or open encrypted files.";

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
  const [cryptoUnlocked, setCryptoUnlocked] = useState(false);
  const [cryptoUnlockHint, setCryptoUnlockHint] = useState<string | null>(null);

  const syncCryptoState = useCallback(() => {
    setCryptoUnlocked(isUnlocked());
  }, []);

  const markCryptoUnlocked = useCallback(() => {
    setCryptoUnlocked(true);
    setCryptoUnlockHint(null);
  }, []);

  const clearCryptoUnlockHint = useCallback(() => {
    setCryptoUnlockHint(null);
  }, []);

  const unlockCryptoSafe = useCallback(async (password: string, userId: string): Promise<boolean> => {
    try {
      await unlockWithPassword(password, userId);
      const ok = isUnlocked();
      setCryptoUnlocked(ok);
      if (ok) {
        setCryptoUnlockHint(null);
      } else {
        setCryptoUnlockHint(UNLOCK_HINT);
      }
      return ok;
    } catch (err) {
      console.warn("Crypto unlock failed:", err);
      setCryptoUnlocked(false);
      setCryptoUnlockHint(UNLOCK_HINT);
      return false;
    }
  }, []);

  const logout = useCallback(async () => {
    const uid = user?.id ?? null;
    try {
      const { unregisterPushNotifications } = await import("../notifications/push");
      await unregisterPushNotifications();
    } catch {
      /* ignore */
    }
    try {
      await api.logout();
    } catch {
      /* ignore */
    }
    await lockAndClearDevice(uid);
    pendingLoginPassword = null;
    await clearSession();
    setUserState(null);
    setCryptoUnlocked(false);
    setCryptoUnlockHint(null);
  }, [user?.id]);

  useEffect(() => {
    setUnauthorizedHandler(() => {
      setUserState(null);
      setCryptoUnlocked(false);
      setCryptoUnlockHint(null);
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
          if (cached) {
            setUserState(cached);
            await tryRestoreUnlock(cached.id);
            syncCryptoState();
          }
        }
      } finally {
        setBooting(false);
      }

      if (await hasSession()) {
        try {
          const me = await api.me();
          setUserState(me);
          await cacheUser(me);
          if (!pendingLoginPassword && !isUnlocked()) {
            await tryRestoreUnlock(me.id);
          }
          syncCryptoState();
          if (!isUnlocked()) {
            setCryptoUnlockHint(UNLOCK_HINT);
          }
        } catch {
          // Network/timeout: keep cached session. 401 is handled by request()
        }
      }
    })();
  }, [syncCryptoState]);

  const login = useCallback(
    async (url: string, email: string, password: string) => {
      await setServerUrl(url);
      setServerUrlState(url.replace(/\/$/, ""));
      const result = await api.login(email.trim().toLowerCase(), password);
      if (is2FAChallenge(result)) {
        pendingLoginPassword = password;
        return result;
      }
      pendingLoginPassword = null;
      await setTokens(result.tokens);
      setUserState(result.user);
      await cacheUser(result.user);
      await unlockCryptoSafe(password, result.user.id);
      return result;
    },
    [unlockCryptoSafe],
  );

  const verify2FA = useCallback(
    async (challengeId: string, code: string) => {
      const result = await api.verify2FA(challengeId, code);
      await setTokens(result.tokens);
      setUserState(result.user);
      await cacheUser(result.user);
      const password = pendingLoginPassword;
      pendingLoginPassword = null;
      if (password) {
        await unlockCryptoSafe(password, result.user.id);
      } else {
        await tryRestoreUnlock(result.user.id);
        syncCryptoState();
        if (!isUnlocked()) {
          setCryptoUnlockHint(UNLOCK_HINT);
        }
      }
    },
    [unlockCryptoSafe, syncCryptoState],
  );

  const refreshProfile = useCallback(async () => {
    const me = await api.me();
    setUserState(me);
    await cacheUser(me);
  }, []);

  const ensureCryptoUnlocked = useCallback(
    async (reason: "upload" | "open" | "generic" = "generic") => {
      if (!user?.id) {
        throw new Error("Sign out and sign in again with your password to unlock encryption.");
      }
      await ensureUnlockedOrPrompt(user.id, reason);
      markCryptoUnlocked();
    },
    [user?.id, markCryptoUnlocked],
  );

  const value = useMemo(
    () => ({
      booting,
      user,
      serverUrl,
      signedIn: Boolean(user),
      cryptoUnlocked,
      cryptoUnlockHint,
      login,
      verify2FA,
      logout,
      refreshProfile,
      markCryptoUnlocked,
      clearCryptoUnlockHint,
      ensureCryptoUnlocked,
    }),
    [
      booting,
      user,
      serverUrl,
      cryptoUnlocked,
      cryptoUnlockHint,
      login,
      verify2FA,
      logout,
      refreshProfile,
      markCryptoUnlocked,
      clearCryptoUnlockHint,
      ensureCryptoUnlocked,
    ],
  );

  return <AuthContext.Provider value={value}>{children}</AuthContext.Provider>;
}

export function useAuth() {
  const ctx = useContext(AuthContext);
  if (!ctx) throw new Error("useAuth must be used within AuthProvider");
  return ctx;
}
