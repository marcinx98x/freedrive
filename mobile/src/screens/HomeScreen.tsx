import React, { useCallback, useState } from "react";
import {
  Alert,
  Pressable,
  RefreshControl,
  ScrollView,
  StyleSheet,
  Text,
  View,
} from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";
import { useAuth } from "../auth/AuthContext";
import { AppDrawer } from "../components/AppDrawer";
import { Icon } from "../components/Icon";
import { SearchBar } from "../components/SearchBar";
import { colors, radii, spacing } from "../theme";
import { formatBytes } from "../utils/format";
import type { BottomTabScreenProps } from "@react-navigation/bottom-tabs";
import type { MainTabParamList } from "../navigation/types";

type Props = BottomTabScreenProps<MainTabParamList, "Home">;

export function HomeScreen({ navigation }: Props) {
  const { user, serverUrl, logout, refreshProfile } = useAuth();
  const [refreshing, setRefreshing] = useState(false);
  const [search, setSearch] = useState("");
  const [drawerOpen, setDrawerOpen] = useState(false);

  const onRefresh = useCallback(async () => {
    setRefreshing(true);
    try {
      await refreshProfile();
    } catch {
      /* ignore */
    } finally {
      setRefreshing(false);
    }
  }, [refreshProfile]);

  const onAvatar = () => {
    Alert.alert(user?.username || "Account", user?.email || serverUrl || "", [
      { text: "Cancel", style: "cancel" },
      {
        text: "Sign out",
        style: "destructive",
        onPress: () => {
          logout().catch(console.error);
        },
      },
    ]);
  };

  const used = user?.used_bytes ?? 0;
  const total = user?.quota_bytes ?? 0;
  const pct = total > 0 ? Math.min(100, Math.round((used / total) * 100)) : 0;

  return (
    <SafeAreaView style={styles.safe} edges={["top"]}>
      <AppDrawer
        visible={drawerOpen}
        onClose={() => setDrawerOpen(false)}
        onNavigate={(route) => navigation.getParent()?.navigate(route)}
        onSettings={onAvatar}
      />
      <SearchBar
        value={search}
        onChangeText={setSearch}
        onSubmit={() => {
          if (search.trim()) {
            navigation.getParent()?.navigate("Search", { query: search.trim() });
          }
        }}
        onAvatarPress={onAvatar}
        onMenuPress={() => setDrawerOpen(true)}
      />
      <ScrollView
        contentContainerStyle={styles.content}
        refreshControl={<RefreshControl refreshing={refreshing} onRefresh={onRefresh} tintColor={colors.accent} />}
      >
        <Text style={styles.greeting}>Hi, {user?.username || "there"}</Text>
        <Text style={styles.subtitle}>Your files are ready on this server.</Text>

        <View style={styles.card}>
          <Text style={styles.cardTitle}>Storage</Text>
          <View style={styles.barTrack}>
            <View style={[styles.barFill, { width: `${pct}%` }]} />
          </View>
          <Text style={styles.cardMeta}>
            {formatBytes(used)} of {formatBytes(total)} used ({pct}%)
          </Text>
        </View>

        <View style={styles.shortcuts}>
          <Pressable style={styles.shortcut} onPress={() => navigation.navigate("Files")}>
            <View style={styles.shortcutIcon}>
              <Icon name="folder" size={24} color={colors.accent} />
            </View>
            <Text style={styles.shortcutLabel}>Files</Text>
          </Pressable>
          <Pressable style={styles.shortcut} onPress={() => navigation.navigate("Starred")}>
            <View style={styles.shortcutIcon}>
              <Icon name="star" size={24} color={colors.accent} />
            </View>
            <Text style={styles.shortcutLabel}>Starred</Text>
          </Pressable>
          <Pressable style={styles.shortcut} onPress={() => navigation.navigate("Shared")}>
            <View style={styles.shortcutIcon}>
              <Icon name="people" size={24} color={colors.accent} />
            </View>
            <Text style={styles.shortcutLabel}>Shared</Text>
          </Pressable>
        </View>
      </ScrollView>
    </SafeAreaView>
  );
}

const styles = StyleSheet.create({
  safe: { flex: 1, backgroundColor: colors.bg },
  content: { padding: spacing.lg, paddingBottom: spacing.xxl },
  greeting: {
    color: colors.text,
    fontSize: 26,
    fontWeight: "700",
    marginTop: spacing.md,
  },
  subtitle: {
    color: colors.textSecondary,
    fontSize: 14,
    marginTop: spacing.xs,
    marginBottom: spacing.xl,
  },
  card: {
    backgroundColor: colors.surface,
    borderRadius: radii.lg,
    padding: spacing.lg,
    marginBottom: spacing.xl,
  },
  cardTitle: {
    color: colors.text,
    fontWeight: "600",
    marginBottom: spacing.md,
  },
  barTrack: {
    height: 8,
    borderRadius: 4,
    backgroundColor: colors.surfaceElevated,
    overflow: "hidden",
  },
  barFill: {
    height: "100%",
    backgroundColor: "#F9AB00",
  },
  cardMeta: {
    color: colors.textSecondary,
    fontSize: 13,
    marginTop: spacing.sm,
  },
  shortcuts: {
    flexDirection: "row",
    gap: spacing.md,
  },
  shortcut: {
    flex: 1,
    backgroundColor: colors.surface,
    borderRadius: radii.lg,
    paddingVertical: spacing.lg,
    alignItems: "center",
  },
  shortcutIcon: { marginBottom: spacing.sm },
  shortcutLabel: { color: colors.text, fontWeight: "600" },
});
