import AsyncStorage from "@react-native-async-storage/async-storage";
import React, { useCallback, useEffect, useState } from "react";
import {
  ActivityIndicator,
  FlatList,
  Pressable,
  RefreshControl,
  StyleSheet,
  Text,
  View,
} from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";
import type { CompositeScreenProps } from "@react-navigation/native";
import type { BottomTabScreenProps } from "@react-navigation/bottom-tabs";
import type { NativeStackScreenProps } from "@react-navigation/native-stack";
import { api, ApiError } from "../api/client";
import type { Computer, FileItem, FolderItem, SortDir, SortKey, ViewMode } from "../api/types";
import { AppDrawer } from "../components/AppDrawer";
import { EmptyState } from "../components/EmptyState";
import { FileGridTile, FileRow } from "../components/FileRow";
import { ComputerRow, FolderGridTile, FolderRow } from "../components/FolderRow";
import { ItemActionsSheet, type ItemTarget } from "../components/ItemActionsSheet";
import { ProfileMenu } from "../components/ProfileMenu";
import { SearchBar } from "../components/SearchBar";
import { SortHeader } from "../components/SortHeader";
import type { MainTabParamList, RootStackParamList } from "../navigation/types";
import { colors, spacing } from "../theme";
import { openFile } from "../utils/openFile";

type Props = CompositeScreenProps<
  BottomTabScreenProps<MainTabParamList, "Files">,
  NativeStackScreenProps<RootStackParamList>
>;

type FilesTab = "my-drive" | "computers";

type ListEntry =
  | { kind: "folder"; item: FolderItem }
  | { kind: "file"; item: FileItem }
  | { kind: "computer"; item: Computer };

const VIEW_KEY = "fd_view_mode";

export function FilesScreen({ navigation }: Props) {
  const [tab, setTab] = useState<FilesTab>("my-drive");
  const [search, setSearch] = useState("");
  const [loading, setLoading] = useState(true);
  const [refreshing, setRefreshing] = useState(false);
  const [error, setError] = useState("");
  const [folders, setFolders] = useState<FolderItem[]>([]);
  const [files, setFiles] = useState<FileItem[]>([]);
  const [computers, setComputers] = useState<Computer[]>([]);
  const [viewMode, setViewMode] = useState<ViewMode>("list");
  const [sort, setSort] = useState<SortKey>("name");
  const [dir, setDir] = useState<SortDir>("asc");
  const [drawerOpen, setDrawerOpen] = useState(false);
  const [profileOpen, setProfileOpen] = useState(false);
  const [menuTarget, setMenuTarget] = useState<ItemTarget | null>(null);

  useEffect(() => {
    AsyncStorage.getItem(VIEW_KEY).then((v) => {
      if (v === "list" || v === "grid") setViewMode(v);
    });
  }, []);

  const changeViewMode = async (mode: ViewMode) => {
    setViewMode(mode);
    await AsyncStorage.setItem(VIEW_KEY, mode);
  };

  const load = useCallback(async () => {
    setError("");
    try {
      if (tab === "computers") {
        const list = await api.computers();
        setComputers(list);
        setFolders([]);
        setFiles([]);
      } else {
        const contents = await api.folderRoot();
        setFolders(contents.folders);
        setFiles(contents.files);
        setComputers([]);
      }
    } catch (err) {
      setError(err instanceof ApiError ? err.message : String(err));
    } finally {
      setLoading(false);
      setRefreshing(false);
    }
  }, [tab]);

  useEffect(() => {
    setLoading(true);
    load();
  }, [load]);

  const onRefresh = () => {
    setRefreshing(true);
    load();
  };

  const sortedEntries = (): ListEntry[] => {
    if (tab === "computers") {
      return computers.map((item) => ({ kind: "computer" as const, item }));
    }
    const folderEntries = folders.map((item) => ({ kind: "folder" as const, item }));
    const fileEntries = files.map((item) => ({ kind: "file" as const, item }));
    const compare = (a: string, b: string) =>
      dir === "asc" ? a.localeCompare(b) : b.localeCompare(a);
    const byDate = (a?: string, b?: string) => {
      const av = a ? new Date(a).getTime() : 0;
      const bv = b ? new Date(b).getTime() : 0;
      return dir === "asc" ? av - bv : bv - av;
    };
    const sortFolders = [...folderEntries].sort((a, b) =>
      sort === "name"
        ? compare(a.item.name, b.item.name)
        : byDate(a.item.updated_at, b.item.updated_at),
    );
    const sortFiles = [...fileEntries].sort((a, b) =>
      sort === "name"
        ? compare(a.item.name, b.item.name)
        : byDate(a.item.updated_at, b.item.updated_at),
    );
    return [...sortFolders, ...sortFiles];
  };

  const entries = sortedEntries();

  const openFolder = (id: string, title: string) => {
    navigation.navigate("Folder", { folderId: id, title });
  };

  const renderItem = ({ item }: { item: ListEntry }) => {
    if (item.kind === "computer") {
      return (
        <ComputerRow
          computer={item.item}
          onPress={() =>
            openFolder(item.item.root_folder_id, item.item.name || item.item.hostname || "Computer")
          }
        />
      );
    }
    if (item.kind === "folder") {
      if (viewMode === "grid") {
        return (
          <FolderGridTile
            folder={item.item}
            onPress={() => openFolder(item.item.id, item.item.name)}
            onMenuPress={() => setMenuTarget({ kind: "folder", item: item.item })}
          />
        );
      }
      return (
        <FolderRow
          folder={item.item}
          onPress={() => openFolder(item.item.id, item.item.name)}
          onMenuPress={() => setMenuTarget({ kind: "folder", item: item.item })}
        />
      );
    }
    if (viewMode === "grid") {
      return (
        <FileGridTile
          file={item.item}
          onPress={() => openFile(item.item, navigation)}
          onMenuPress={() => setMenuTarget({ kind: "file", item: item.item })}
        />
      );
    }
    return (
      <FileRow
        file={item.item}
        onPress={() => openFile(item.item, navigation)}
        onMenuPress={() => setMenuTarget({ kind: "file", item: item.item })}
      />
    );
  };

  return (
    <SafeAreaView style={styles.safe} edges={["top"]}>
      <AppDrawer
        visible={drawerOpen}
        onClose={() => setDrawerOpen(false)}
        onNavigate={(route) => navigation.navigate(route)}
        onSettings={() => setProfileOpen(true)}
      />
      <ProfileMenu visible={profileOpen} onClose={() => setProfileOpen(false)} />
      <ItemActionsSheet
        target={menuTarget}
        onClose={() => setMenuTarget(null)}
        onChanged={load}
      />
      <SearchBar
        value={search}
        onChangeText={setSearch}
        onSubmit={() => {
          if (search.trim()) navigation.navigate("Search", { query: search.trim() });
        }}
        onAvatarPress={() => setProfileOpen(true)}
        onMenuPress={() => setDrawerOpen(true)}
      />

      <View style={styles.tabs}>
        <Pressable
          style={[styles.tab, tab === "my-drive" && styles.tabActive]}
          onPress={() => setTab("my-drive")}
        >
          <Text style={[styles.tabText, tab === "my-drive" && styles.tabTextActive]}>My Drive</Text>
        </Pressable>
        <Pressable
          style={[styles.tab, tab === "computers" && styles.tabActive]}
          onPress={() => setTab("computers")}
        >
          <Text style={[styles.tabText, tab === "computers" && styles.tabTextActive]}>
            Computers
          </Text>
        </Pressable>
      </View>

      {tab === "computers" ? (
        <View style={styles.banner}>
          <Text style={styles.bannerText}>
            Changes to your files will automatically sync between FreeDrive and your computer
          </Text>
        </View>
      ) : null}

      {tab === "my-drive" ? (
        <SortHeader
          sort={sort}
          dir={dir}
          viewMode={viewMode}
          onToggleSort={() => {
            if (sort === "name") {
              setDir((d) => (d === "asc" ? "desc" : "asc"));
            } else {
              setSort("name");
              setDir("asc");
            }
          }}
          onChangeViewMode={changeViewMode}
        />
      ) : (
        <SortHeader
          sort={sort}
          dir={dir}
          viewMode="list"
          onToggleSort={() => setDir((d) => (d === "asc" ? "desc" : "asc"))}
          onChangeViewMode={() => undefined}
        />
      )}

      {error ? <Text style={styles.error}>{error}</Text> : null}

      {loading ? (
        <ActivityIndicator style={{ marginTop: 40 }} color={colors.accent} />
      ) : (
        <FlatList
          key={viewMode === "grid" && tab === "my-drive" ? "grid" : "list"}
          data={entries}
          keyExtractor={(item) => `${item.kind}-${item.item.id}`}
          renderItem={renderItem}
          numColumns={viewMode === "grid" && tab === "my-drive" ? 2 : 1}
          columnWrapperStyle={
            viewMode === "grid" && tab === "my-drive" ? styles.gridRow : undefined
          }
          contentContainerStyle={entries.length === 0 ? styles.emptyContainer : undefined}
          refreshControl={
            <RefreshControl refreshing={refreshing} onRefresh={onRefresh} tintColor={colors.accent} />
          }
          ListEmptyComponent={
            <EmptyState
              title={tab === "computers" ? "No computers" : "No files yet"}
              subtitle={
                tab === "computers"
                  ? "Register a computer with the FreeDrive desktop app"
                  : "Folders and files from My Drive will appear here"
              }
            />
          }
        />
      )}
    </SafeAreaView>
  );
}

const styles = StyleSheet.create({
  safe: { flex: 1, backgroundColor: colors.bg },
  tabs: {
    flexDirection: "row",
    paddingHorizontal: spacing.lg,
    gap: spacing.xl,
    borderBottomWidth: StyleSheet.hairlineWidth,
    borderBottomColor: colors.border,
  },
  tab: {
    paddingVertical: spacing.md,
    borderBottomWidth: 2,
    borderBottomColor: "transparent",
  },
  tabActive: { borderBottomColor: colors.accent },
  tabText: { color: colors.textSecondary, fontSize: 15, fontWeight: "500" },
  tabTextActive: { color: colors.text },
  banner: {
    marginHorizontal: spacing.lg,
    marginTop: spacing.md,
    backgroundColor: colors.surface,
    borderRadius: 12,
    padding: spacing.md,
  },
  bannerText: { color: colors.textSecondary, fontSize: 13, lineHeight: 18 },
  error: {
    color: colors.danger,
    paddingHorizontal: spacing.lg,
    marginBottom: spacing.sm,
  },
  gridRow: {
    paddingHorizontal: spacing.lg,
    justifyContent: "space-between",
  },
  emptyContainer: { flexGrow: 1 },
});
