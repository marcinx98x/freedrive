import React, { useCallback, useEffect, useMemo, useState } from "react";
import {
  ActivityIndicator,
  Alert,
  Linking,
  Pressable,
  ScrollView,
  StyleSheet,
  Switch,
  Text,
  View,
} from "react-native";
import type { NativeStackScreenProps } from "@react-navigation/native-stack";
import { useSafeAreaInsets } from "react-native-safe-area-context";
import { api } from "../api/client";
import type { StorageInfo } from "../api/types";
import { useAuth } from "../auth/AuthContext";
import { Icon } from "../components/Icon";
import { clearAppCache, getCacheUsageBytes } from "../settings/cacheStats";
import {
  getWifiOnly,
  setWifiOnly,
  themePreferenceLabel,
} from "../settings/prefs";
import { useTheme } from "../theme/ThemeContext";
import { radii, spacing, type ThemeColors } from "../theme";
import { formatBytes } from "../utils/format";
import type { RootStackParamList } from "../navigation/types";

type Props = NativeStackScreenProps<RootStackParamList, "Settings">;

export function SettingsScreen({ navigation }: Props) {
  const { colors, preference, setPreference } = useTheme();
  const { serverUrl } = useAuth();
  const insets = useSafeAreaInsets();
  const styles = useMemo(() => makeStyles(colors), [colors]);

  const [storage, setStorage] = useState<StorageInfo | null>(null);
  const [cacheBytes, setCacheBytes] = useState<number | null>(null);
  const [clearing, setClearing] = useState(false);
  const [wifiOnly, setWifiOnlyState] = useState(false);
  const [wifiLoading, setWifiLoading] = useState(true);

  const refreshCache = useCallback(async () => {
    try {
      setCacheBytes(await getCacheUsageBytes());
    } catch {
      setCacheBytes(0);
    }
  }, []);

  useEffect(() => {
    navigation.setOptions({
      headerShown: true,
      title: "Settings",
      headerStyle: { backgroundColor: colors.bg },
      headerTintColor: colors.text,
      headerShadowVisible: false,
      contentStyle: { backgroundColor: colors.bg },
    });
  }, [navigation, colors]);

  useEffect(() => {
    let cancelled = false;
    api
      .myStorage()
      .then((s) => {
        if (!cancelled) setStorage(s);
      })
      .catch(() => {
        if (!cancelled) setStorage(null);
      });
    void refreshCache();
    getWifiOnly()
      .then((v) => {
        if (!cancelled) setWifiOnlyState(v);
      })
      .finally(() => {
        if (!cancelled) setWifiLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [refreshCache]);

  const openManageStorage = () => {
    const base = serverUrl?.replace(/\/$/, "") || "";
    if (!base) {
      Alert.alert("Storage", "Server URL is not available.");
      return;
    }
    Linking.openURL(`${base}/#/storage`).catch(() => {
      Alert.alert("Storage", "Could not open storage page.");
    });
  };

  const openNotificationSettings = () => {
    Linking.openSettings().catch(() => {
      Alert.alert("Notifications", "Could not open system settings.");
    });
  };

  const pickTheme = () => {
    Alert.alert("Choose theme", undefined, [
      {
        text: "System default",
        onPress: () => void setPreference("system"),
      },
      {
        text: "Light",
        onPress: () => void setPreference("light"),
      },
      {
        text: "Dark",
        onPress: () => void setPreference("dark"),
      },
      { text: "Cancel", style: "cancel" },
    ]);
  };

  const onClearCache = () => {
    Alert.alert(
      "Clear cache",
      "Remove all cached documents? This does not delete files on FreeDrive.",
      [
        { text: "Cancel", style: "cancel" },
        {
          text: "Clear",
          style: "destructive",
          onPress: () => {
            void (async () => {
              setClearing(true);
              try {
                await clearAppCache();
                await refreshCache();
              } catch (err) {
                Alert.alert(
                  "Clear cache",
                  err instanceof Error ? err.message : String(err),
                );
              } finally {
                setClearing(false);
              }
            })();
          },
        },
      ],
    );
  };

  const onToggleWifi = (value: boolean) => {
    setWifiOnlyState(value);
    void setWifiOnly(value);
  };

  const used = storage?.used_bytes ?? 0;
  const total = storage?.total_bytes ?? 0;
  const storageHint =
    total > 0 ? `${formatBytes(used)} of ${formatBytes(total)} used` : null;

  return (
    <ScrollView
      style={styles.scroll}
      contentContainerStyle={[
        styles.content,
        { paddingBottom: spacing.xxl + insets.bottom },
      ]}
    >
      <Pressable
        onPress={openManageStorage}
        style={({ pressed }) => [styles.pill, pressed && styles.pillPressed]}
      >
        <View style={styles.pillIcon}>
          <Icon name="cloud" size={20} color={colors.accent} />
        </View>
        <View style={{ flex: 1 }}>
          <Text style={styles.pillTitle}>Manage storage</Text>
          {storageHint ? <Text style={styles.pillSub}>{storageHint}</Text> : null}
        </View>
      </Pressable>

      <Text style={styles.section}>Notifications</Text>
      <Pressable
        onPress={openNotificationSettings}
        style={({ pressed }) => [styles.row, pressed && styles.rowPressed]}
      >
        <Text style={styles.rowTitle}>Notification settings</Text>
      </Pressable>

      <Text style={styles.section}>Theme</Text>
      <Pressable
        onPress={pickTheme}
        style={({ pressed }) => [styles.row, pressed && styles.rowPressed]}
      >
        <Text style={styles.rowTitle}>Choose theme</Text>
        <Text style={styles.rowSub}>{themePreferenceLabel(preference)}</Text>
      </Pressable>

      <Text style={styles.section}>Documents cache</Text>
      <Pressable
        onPress={onClearCache}
        disabled={clearing}
        style={({ pressed }) => [styles.row, pressed && styles.rowPressed]}
      >
        <Text style={styles.rowTitle}>Clear cache</Text>
        <Text style={styles.rowSub}>Remove all cached documents</Text>
      </Pressable>
      <View style={styles.row}>
        <Text style={styles.rowTitle}>Cache size</Text>
        {clearing ? (
          <ActivityIndicator color={colors.accent} style={{ marginTop: 8 }} />
        ) : (
          <Text style={styles.rowSub}>
            {cacheBytes === null ? "…" : `Using ${formatBytes(cacheBytes)}`}
          </Text>
        )}
      </View>

      <Text style={styles.section}>Data usage</Text>
      <View style={styles.rowWifi}>
        <View style={{ flex: 1, paddingRight: spacing.md }}>
          <Text style={styles.rowTitle}>Transfer files only over Wi-Fi</Text>
          <Text style={styles.rowSub}>
            Uploading and updating of files will pause when Wi-Fi connection
            isn't available.
          </Text>
        </View>
        {wifiLoading ? (
          <ActivityIndicator color={colors.accent} />
        ) : (
          <Switch
            value={wifiOnly}
            onValueChange={onToggleWifi}
            trackColor={{ false: colors.border, true: colors.accent }}
            thumbColor="#FFFFFF"
          />
        )}
      </View>
    </ScrollView>
  );
}

function makeStyles(colors: ThemeColors) {
  return StyleSheet.create({
    scroll: {
      flex: 1,
      backgroundColor: colors.bg,
    },
    content: {
      paddingHorizontal: spacing.lg,
      paddingTop: spacing.md,
    },
    pill: {
      flexDirection: "row",
      alignItems: "center",
      gap: spacing.md,
      alignSelf: "flex-start",
      paddingVertical: spacing.sm,
      paddingHorizontal: spacing.lg,
      borderRadius: radii.pill,
      borderWidth: StyleSheet.hairlineWidth,
      borderColor: colors.pillBorder,
      marginBottom: spacing.xl,
    },
    pillPressed: {
      opacity: 0.75,
    },
    pillIcon: {
      width: 28,
      height: 28,
      borderRadius: 14,
      alignItems: "center",
      justifyContent: "center",
      backgroundColor: colors.surfaceElevated,
    },
    pillTitle: {
      color: colors.accent,
      fontSize: 15,
      fontWeight: "600",
    },
    pillSub: {
      color: colors.textSecondary,
      fontSize: 12,
      marginTop: 2,
    },
    section: {
      color: colors.sectionHeader,
      fontSize: 13,
      fontWeight: "600",
      marginTop: spacing.lg,
      marginBottom: spacing.sm,
    },
    row: {
      paddingVertical: spacing.md,
    },
    rowPressed: {
      opacity: 0.7,
    },
    rowTitle: {
      color: colors.text,
      fontSize: 16,
    },
    rowSub: {
      color: colors.textSecondary,
      fontSize: 13,
      marginTop: 4,
      lineHeight: 18,
    },
    rowWifi: {
      flexDirection: "row",
      alignItems: "center",
      paddingVertical: spacing.md,
    },
  });
}
