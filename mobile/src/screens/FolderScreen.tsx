import AsyncStorage from "@react-native-async-storage/async-storage";
import React, { useCallback, useEffect, useState } from "react";
import {
  ActivityIndicator,
  FlatList,
  RefreshControl,
  StyleSheet,
  Text,
} from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";
import type { NativeStackScreenProps } from "@react-navigation/native-stack";
import { api, ApiError } from "../api/client";
import type { FileItem, FolderItem, SortDir, SortKey, ViewMode } from "../api/types";
import { EmptyState } from "../components/EmptyState";
import { FileGridTile, FileRow } from "../components/FileRow";
import { FolderGridTile, FolderRow } from "../components/FolderRow";
import { SortHeader } from "../components/SortHeader";
import type { RootStackParamList } from "../navigation/types";
import { colors, spacing } from "../theme";

type Props = NativeStackScreenProps<RootStackParamList, "Folder">;

type ListEntry =
  | { kind: "folder"; item: FolderItem }
  | { kind: "file"; item: FileItem };

const VIEW_KEY = "fd_view_mode";

export function FolderScreen({ route, navigation }: Props) {
  const { folderId, title } = route.params;
  const [loading, setLoading] = useState(true);
  const [refreshing, setRefreshing] = useState(false);
  const [error, setError] = useState("");
  const [folders, setFolders] = useState<FolderItem[]>([]);
  const [files, setFiles] = useState<FileItem[]>([]);
  const [viewMode, setViewMode] = useState<ViewMode>("list");
  const [sort, setSort] = useState<SortKey>("name");
  const [dir, setDir] = useState<SortDir>("asc");

  useEffect(() => {
    navigation.setOptions({
      title: title || "Folder",
      headerStyle: { backgroundColor: colors.bg },
      headerTintColor: colors.text,
      headerTitleStyle: { fontWeight: "600" },
      headerShadowVisible: false,
    });
  }, [navigation, title]);

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
      const contents = await api.folder(folderId);
      setFolders(contents.folders);
      setFiles(contents.files);
      if (contents.folder?.name) {
        navigation.setOptions({ title: contents.folder.name });
      }
    } catch (err) {
      setError(err instanceof ApiError ? err.message : String(err));
    } finally {
      setLoading(false);
      setRefreshing(false);
    }
  }, [folderId, navigation]);

  useEffect(() => {
    setLoading(true);
    load();
  }, [load]);

  const entries: ListEntry[] = (() => {
    const compare = (a: string, b: string) =>
      dir === "asc" ? a.localeCompare(b) : b.localeCompare(a);
    const byDate = (a?: string, b?: string) => {
      const av = a ? new Date(a).getTime() : 0;
      const bv = b ? new Date(b).getTime() : 0;
      return dir === "asc" ? av - bv : bv - av;
    };
    const folderEntries = [...folders]
      .sort((a, b) =>
        sort === "name" ? compare(a.name, b.name) : byDate(a.updated_at, b.updated_at),
      )
      .map((item) => ({ kind: "folder" as const, item }));
    const fileEntries = [...files]
      .sort((a, b) =>
        sort === "name" ? compare(a.name, b.name) : byDate(a.updated_at, b.updated_at),
      )
      .map((item) => ({ kind: "file" as const, item }));
    return [...folderEntries, ...fileEntries];
  })();

  const renderItem = ({ item }: { item: ListEntry }) => {
    if (item.kind === "folder") {
      if (viewMode === "grid") {
        return (
          <FolderGridTile
            folder={item.item}
            onPress={() =>
              navigation.push("Folder", { folderId: item.item.id, title: item.item.name })
            }
          />
        );
      }
      return (
        <FolderRow
          folder={item.item}
          onPress={() =>
            navigation.push("Folder", { folderId: item.item.id, title: item.item.name })
          }
        />
      );
    }
    return viewMode === "grid" ? <FileGridTile file={item.item} /> : <FileRow file={item.item} />;
  };

  return (
    <SafeAreaView style={styles.safe} edges={["bottom"]}>
      <SortHeader
        sort={sort}
        dir={dir}
        viewMode={viewMode}
        onToggleSort={() => {
          if (sort === "name") setDir((d) => (d === "asc" ? "desc" : "asc"));
          else {
            setSort("name");
            setDir("asc");
          }
        }}
        onChangeViewMode={changeViewMode}
      />
      {error ? <Text style={styles.error}>{error}</Text> : null}
      {loading ? (
        <ActivityIndicator style={{ marginTop: 40 }} color={colors.accent} />
      ) : (
        <FlatList
          key={viewMode}
          data={entries}
          keyExtractor={(item) => `${item.kind}-${item.item.id}`}
          renderItem={renderItem}
          numColumns={viewMode === "grid" ? 2 : 1}
          columnWrapperStyle={viewMode === "grid" ? styles.gridRow : undefined}
          contentContainerStyle={entries.length === 0 ? styles.emptyContainer : undefined}
          refreshControl={
            <RefreshControl
              refreshing={refreshing}
              onRefresh={() => {
                setRefreshing(true);
                load();
              }}
              tintColor={colors.accent}
            />
          }
          ListEmptyComponent={
            <EmptyState title="This folder is empty" subtitle="No files or folders here yet" />
          }
        />
      )}
    </SafeAreaView>
  );
}

const styles = StyleSheet.create({
  safe: { flex: 1, backgroundColor: colors.bg },
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
