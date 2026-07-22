import * as Clipboard from "expo-clipboard";
import * as FileSystem from "expo-file-system/legacy";
import * as Sharing from "expo-sharing";
import { Alert, NativeModules, PermissionsAndroid, Platform } from "react-native";
import { api } from "../api/client";
import type { FileItem } from "../api/types";
import { decryptDownloadedFile } from "../crypto";
import type { RootStackParamList } from "../navigation/types";

type DownloadsNativeModule = {
  beginDownload(fileName: string): Promise<number>;
  completeDownload(
    notificationId: number,
    fileName: string,
    mimeType: string,
    contentUri: string,
  ): Promise<void>;
  failDownload(notificationId: number, message: string): Promise<void>;
  saveBase64(fileName: string, mimeType: string, base64: string): Promise<string>;
};

const downloadsModule = NativeModules.FreeDriveDownloads as DownloadsNativeModule | undefined;

type PreviewNav = {
  navigate: (
    screen: "FilePreview",
    params: RootStackParamList["FilePreview"],
  ) => void;
};

function bytesToBase64(bytes: Uint8Array): string {
  let binary = "";
  const chunk = 0x8000;
  for (let i = 0; i < bytes.length; i += chunk) {
    binary += String.fromCharCode(...bytes.subarray(i, i + chunk));
  }
  return globalThis.btoa(binary);
}

function safeFileName(name: string): string {
  return name.replace(/[\\/:*?"<>|]/g, "_") || "file";
}

export async function downloadAndDecrypt(file: FileItem): Promise<{
  uri: string;
  mime: string;
  bytes: Uint8Array;
}> {
  const downloaded = await api.downloadEncrypted(file.id);
  const plain = await decryptDownloadedFile(file.id, downloaded.iv || file.iv, downloaded.data);
  const mime = downloaded.mime || file.mime_type || "application/octet-stream";
  const dir = FileSystem.cacheDirectory;
  if (!dir) throw new Error("Cache directory unavailable");
  const path = `${dir}fd_${file.id}_${safeFileName(file.name)}`;
  await FileSystem.writeAsStringAsync(path, bytesToBase64(plain), {
    encoding: FileSystem.EncodingType.Base64,
  });
  return { uri: path, mime, bytes: plain };
}

function isImage(mime: string): boolean {
  return mime.toLowerCase().startsWith("image/");
}

function isText(mime: string, name: string): boolean {
  const m = mime.toLowerCase();
  const n = name.toLowerCase();
  return (
    m.startsWith("text/") ||
    m.includes("json") ||
    n.endsWith(".md") ||
    n.endsWith(".txt") ||
    n.endsWith(".csv") ||
    n.endsWith(".json")
  );
}

function isPdf(mime: string, name: string): boolean {
  return mime.toLowerCase().includes("pdf") || name.toLowerCase().endsWith(".pdf");
}

export async function openFile(file: FileItem, navigation?: PreviewNav): Promise<void> {
  try {
    const { uri, mime, bytes } = await downloadAndDecrypt(file);

    if (navigation && isImage(mime)) {
      navigation.navigate("FilePreview", {
        title: file.name,
        uri,
        mime,
        mode: "image",
      });
      return;
    }

    if (navigation && isText(mime, file.name)) {
      const text = new TextDecoder().decode(bytes);
      navigation.navigate("FilePreview", {
        title: file.name,
        uri,
        mime,
        mode: "text",
        text,
      });
      return;
    }

    if (navigation && isPdf(mime, file.name)) {
      navigation.navigate("FilePreview", {
        title: file.name,
        uri,
        mime,
        mode: "pdf",
      });
      return;
    }

    if (await Sharing.isAvailableAsync()) {
      await Sharing.shareAsync(uri, { mimeType: mime, dialogTitle: file.name });
    } else {
      Alert.alert("Downloaded", `Saved to cache as ${file.name}`);
    }
  } catch (err) {
    Alert.alert("Cannot open file", err instanceof Error ? err.message : String(err));
  }
}

/** Decrypt and open the system share sheet (send a copy). */
export async function downloadFileToShare(file: FileItem): Promise<void> {
  try {
    const { uri, mime } = await downloadAndDecrypt(file);
    if (await Sharing.isAvailableAsync()) {
      await Sharing.shareAsync(uri, { mimeType: mime, dialogTitle: file.name });
    } else {
      Alert.alert("Ready", file.name);
    }
  } catch (err) {
    Alert.alert("Share failed", err instanceof Error ? err.message : String(err));
  }
}

async function requestNotificationPermissionIfNeeded(): Promise<void> {
  if (Platform.OS !== "android" || Platform.Version < 33) return;
  try {
    await PermissionsAndroid.request(
      PermissionsAndroid.PERMISSIONS.POST_NOTIFICATIONS,
    );
  } catch {
    // Denial must not block saving the file.
  }
}

/** Decrypt and save into Android's shared Downloads collection. */
export async function downloadFileToDevice(file: FileItem): Promise<void> {
  let notificationId: number | undefined;
  try {
    if (Platform.OS !== "android") {
      const { uri, mime } = await downloadAndDecrypt(file);
      if (await Sharing.isAvailableAsync()) {
        await Sharing.shareAsync(uri, { mimeType: mime, dialogTitle: `Save ${file.name}` });
      } else {
        Alert.alert("Downloaded", file.name);
      }
      return;
    }

    if (!downloadsModule) {
      const { uri, mime } = await downloadAndDecrypt(file);
      if (await Sharing.isAvailableAsync()) {
        await Sharing.shareAsync(uri, {
          mimeType: mime,
          dialogTitle: `Save ${file.name}`,
        });
        return;
      }
      throw new Error("Android download service is unavailable");
    }

    await requestNotificationPermissionIfNeeded();
    notificationId = await downloadsModule.beginDownload(file.name);

    const { mime, bytes } = await downloadAndDecrypt(file);
    const savedUri = await downloadsModule.saveBase64(
      file.name,
      mime,
      bytesToBase64(bytes),
    );
    await downloadsModule.completeDownload(notificationId, file.name, mime, savedUri);
    notificationId = undefined;
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    if (notificationId != null && downloadsModule) {
      try {
        await downloadsModule.failDownload(notificationId, message);
      } catch {
        // Notification failure must not hide the download error.
      }
    }
    Alert.alert("Download failed", message);
  }
}

export async function copyText(text: string): Promise<void> {
  await Clipboard.setStringAsync(text);
}
