import { Alert } from "react-native";
import * as DocumentPicker from "expo-document-picker";
import * as FileSystem from "expo-file-system/legacy";
import { api } from "../api/client";
import type { FileItem } from "../api/types";
import { cacheFileKey, prepareNewEncryptedFile } from "../crypto";

function bytesToBase64(bytes: Uint8Array): string {
  let binary = "";
  const chunk = 0x8000;
  for (let i = 0; i < bytes.length; i += chunk) {
    binary += String.fromCharCode(...bytes.subarray(i, i + chunk));
  }
  return globalThis.btoa(binary);
}

function base64ToBytes(base64: string): Uint8Array {
  const binary = globalThis.atob(base64);
  const out = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i += 1) out[i] = binary.charCodeAt(i);
  return out;
}

function safeName(name: string): string {
  return name.replace(/[\\/:*?"<>|]/g, "_") || "file";
}

async function uploadOne(
  asset: DocumentPicker.DocumentPickerAsset,
  folderId: string | null,
): Promise<FileItem> {
  const name = asset.name || "file";
  const mimeType = asset.mimeType || "application/octet-stream";
  const b64 = await FileSystem.readAsStringAsync(asset.uri, {
    encoding: FileSystem.EncodingType.Base64,
  });
  const plaintext = base64ToBytes(b64);
  const prepared = await prepareNewEncryptedFile(plaintext);
  const dir = FileSystem.cacheDirectory;
  if (!dir) throw new Error("Cache directory unavailable");
  const encPath = `${dir}fd_new_${Date.now()}_${safeName(name)}.bin`;
  await FileSystem.writeAsStringAsync(encPath, bytesToBase64(prepared.ciphertext), {
    encoding: FileSystem.EncodingType.Base64,
  });
  try {
    const created = await api.uploadFile({
      name,
      mimeType,
      iv: prepared.ivB64,
      originalSize: plaintext.length,
      encryptedUri: encPath,
      folderId,
    });
    await api.putFileEncryptionKey(created.id, prepared.wrappedFileKey);
    await cacheFileKey(created.id, prepared.rawKey);
    return created;
  } finally {
    await FileSystem.deleteAsync(encPath, { idempotent: true }).catch(() => {});
  }
}

export type UploadProgress = {
  current: number;
  total: number;
  name: string;
};

/**
 * Pick multiple files and upload them encrypted into folderId (null = My Drive root).
 * Returns uploaded FileItems. Shows an alert summarizing failures.
 */
export async function pickAndUploadFiles(
  folderId: string | null,
  onProgress?: (p: UploadProgress) => void,
): Promise<FileItem[]> {
  const result = await DocumentPicker.getDocumentAsync({
    multiple: true,
    copyToCacheDirectory: true,
  });
  if (result.canceled || !result.assets?.length) return [];

  const uploaded: FileItem[] = [];
  const failures: string[] = [];
  const total = result.assets.length;

  for (let i = 0; i < result.assets.length; i += 1) {
    const asset = result.assets[i]!;
    const name = asset.name || "file";
    onProgress?.({ current: i + 1, total, name });
    try {
      uploaded.push(await uploadOne(asset, folderId));
    } catch (err) {
      failures.push(`${name}: ${err instanceof Error ? err.message : String(err)}`);
    }
  }

  if (failures.length > 0) {
    Alert.alert(
      uploaded.length ? "Partial upload" : "Upload failed",
      `${uploaded.length} of ${total} uploaded.\n\n${failures.slice(0, 5).join("\n")}${
        failures.length > 5 ? `\n…and ${failures.length - 5} more` : ""
      }`,
    );
  }

  return uploaded;
}

/** Create a new encrypted text file in folderId (null = My Drive root). */
export async function createEncryptedTextFile(opts: {
  name: string;
  mimeType: string;
  text: string;
  folderId: string | null;
}): Promise<FileItem> {
  return createEncryptedBinaryFile({
    name: opts.name,
    mimeType: opts.mimeType,
    bytes: new TextEncoder().encode(opts.text),
    folderId: opts.folderId,
  });
}

/** Create a new encrypted binary file (e.g. empty xlsx) in folderId. */
export async function createEncryptedBinaryFile(opts: {
  name: string;
  mimeType: string;
  bytes: Uint8Array;
  folderId: string | null;
}): Promise<FileItem> {
  const plaintext = opts.bytes;
  const prepared = await prepareNewEncryptedFile(plaintext);
  const dir = FileSystem.cacheDirectory;
  if (!dir) throw new Error("Cache directory unavailable");
  const encPath = `${dir}fd_new_${Date.now()}_${safeName(opts.name)}.bin`;
  await FileSystem.writeAsStringAsync(encPath, bytesToBase64(prepared.ciphertext), {
    encoding: FileSystem.EncodingType.Base64,
  });
  try {
    const created = await api.uploadFile({
      name: opts.name,
      mimeType: opts.mimeType,
      iv: prepared.ivB64,
      originalSize: plaintext.length,
      encryptedUri: encPath,
      folderId: opts.folderId,
    });
    await api.putFileEncryptionKey(created.id, prepared.wrappedFileKey);
    await cacheFileKey(created.id, prepared.rawKey);
    return created;
  } finally {
    await FileSystem.deleteAsync(encPath, { idempotent: true }).catch(() => {});
  }
}
