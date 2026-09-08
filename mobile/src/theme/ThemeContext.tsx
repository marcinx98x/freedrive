import React, {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
} from "react";
import { Appearance, useColorScheme } from "react-native";
import {
  getThemePreference,
  setThemePreference,
  type ThemePreference,
} from "../settings/prefs";
import { darkColors, lightColors, type ThemeColors } from "../theme";

type ThemeContextValue = {
  colors: ThemeColors;
  preference: ThemePreference;
  resolved: "light" | "dark";
  setPreference: (pref: ThemePreference) => Promise<void>;
};

const ThemeContext = createContext<ThemeContextValue | null>(null);

function resolveScheme(
  pref: ThemePreference,
  system: string | null | undefined,
): "light" | "dark" {
  if (pref === "light" || pref === "dark") return pref;
  return system === "light" ? "light" : "dark";
}

export function ThemeProvider({ children }: { children: React.ReactNode }) {
  const system = useColorScheme();
  const [preference, setPreferenceState] = useState<ThemePreference>("system");
  const [ready, setReady] = useState(false);

  useEffect(() => {
    let cancelled = false;
    getThemePreference().then((pref) => {
      if (!cancelled) {
        setPreferenceState(pref);
        setReady(true);
      }
    });
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    const sub = Appearance.addChangeListener(() => {
      /* useColorScheme updates; force re-render via preference dependency */
      setPreferenceState((p) => p);
    });
    return () => sub.remove();
  }, []);

  const setPreference = useCallback(async (pref: ThemePreference) => {
    setPreferenceState(pref);
    await setThemePreference(pref);
  }, []);

  const resolved = resolveScheme(preference, system);
  const colors = resolved === "light" ? lightColors : darkColors;

  const value = useMemo(
    () => ({ colors, preference, resolved, setPreference }),
    [colors, preference, resolved, setPreference],
  );

  if (!ready) {
    return (
      <ThemeContext.Provider
        value={{
          colors: darkColors,
          preference: "system",
          resolved: "dark",
          setPreference,
        }}
      >
        {children}
      </ThemeContext.Provider>
    );
  }

  return <ThemeContext.Provider value={value}>{children}</ThemeContext.Provider>;
}

export function useTheme(): ThemeContextValue {
  const ctx = useContext(ThemeContext);
  if (!ctx) {
    return {
      colors: darkColors,
      preference: "system",
      resolved: "dark",
      setPreference: async () => {},
    };
  }
  return ctx;
}
