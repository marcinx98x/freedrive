import * as Device from "expo-device";
import {
  clearSession,
  getServerUrl,
  getTokens,
  setTokens,
} from "../auth/storage";
import type {
  ActivityLog,
  Computer,
  FileItem,
  FilesListResponse,
  FolderContents,
  FolderItem,
  LoginResult,
  LoginSuccess,
  SharedItem,
  SortDir,
  SortKey,
  TokenPair,
  User,
} from "./types";

export class ApiError extends Error {
  status: number;
  constructor(message: string, status: number) {
    super(message);
    this.status = status;
  }
}

type AuthListener = () => void;
let onUnauthorized: AuthListener | null = null;

export function setUnauthorizedHandler(handler: AuthListener | null) {
  onUnauthorized = handler;
}

function deviceName(): string {
  const model = Device.modelName || Device.deviceName || "Phone";
  return `Mobile (${model})`;
}

function deviceHeaders(): Record<string, string> {
  return {
    "X-Device-Type": "web",
    "X-Device-Name": deviceName(),
  };
}

async function baseUrl(): Promise<string> {
  const url = await getServerUrl();
  if (!url) throw new ApiError("Server URL is not configured", 0);
  return url.replace(/\/$/, "");
}

async function parseError(res: Response): Promise<string> {
  try {
    const data = await res.json();
    if (data?.error) return String(data.error);
  } catch {
    /* ignore */
  }
  return `Request failed (${res.status})`;
}

const REQUEST_TIMEOUT_MS = 15_000;

async function fetchWithTimeout(input: string, init?: RequestInit): Promise<Response> {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), REQUEST_TIMEOUT_MS);
  try {
    return await fetch(input, { ...init, signal: controller.signal });
  } catch (err) {
    if (err instanceof Error && err.name === "AbortError") {
      throw new ApiError("Request timed out", 0);
    }
    throw err;
  } finally {
    clearTimeout(timer);
  }
}

let refreshInFlight: Promise<boolean> | null = null;

async function tryRefresh(): Promise<boolean> {
  if (refreshInFlight) return refreshInFlight;
  refreshInFlight = (async () => {
    try {
      const tokens = await getTokens();
      if (!tokens?.refresh_token) return false;
      const url = await baseUrl();
      const res = await fetchWithTimeout(`${url}/api/v1/auth/refresh`, {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
          ...deviceHeaders(),
        },
        body: JSON.stringify({ refresh_token: tokens.refresh_token }),
      });
      if (!res.ok) return false;
      const data = (await res.json()) as { tokens: TokenPair };
      if (!data?.tokens?.access_token) return false;
      await setTokens(data.tokens);
      return true;
    } catch {
      return false;
    } finally {
      refreshInFlight = null;
    }
  })();
  return refreshInFlight;
}

async function request<T>(
  method: string,
  path: string,
  body?: unknown,
  opts?: { auth?: boolean; retry?: boolean },
): Promise<T> {
  const auth = opts?.auth !== false;
  const retry = opts?.retry !== false;
  const url = await baseUrl();
  const headers: Record<string, string> = {
    "Content-Type": "application/json",
    ...deviceHeaders(),
  };
  if (auth) {
    const tokens = await getTokens();
    if (tokens?.access_token) {
      headers.Authorization = `Bearer ${tokens.access_token}`;
    }
  }

  const res = await fetchWithTimeout(`${url}/api/v1${path}`, {
    method,
    headers,
    body: body === undefined ? undefined : JSON.stringify(body),
  });

  if (res.status === 401 && auth && retry) {
    const refreshed = await tryRefresh();
    if (refreshed) {
      return request<T>(method, path, body, { auth, retry: false });
    }
    await clearSession();
    onUnauthorized?.();
    throw new ApiError("Session expired", 401);
  }

  if (!res.ok) {
    throw new ApiError(await parseError(res), res.status);
  }

  if (res.status === 204) return undefined as T;
  return (await res.json()) as T;
}

export const api = {
  login: async (email: string, password: string) => {
    return request<LoginResult>("POST", "/auth/login", { email, password }, { auth: false });
  },

  verify2FA: async (challenge_id: string, code: string) => {
    return request<LoginSuccess>(
      "POST",
      "/auth/verify-2fa",
      { challenge_id, code },
      { auth: false },
    );
  },

  logout: async () => {
    const tokens = await getTokens();
    if (!tokens?.refresh_token) return;
    try {
      await request("POST", "/auth/logout", { refresh_token: tokens.refresh_token }, { retry: false });
    } catch {
      /* ignore logout errors */
    }
  },

  me: () => request<User>("GET", "/me"),

  folderRoot: async () => {
    const data = await request<FolderContents>("GET", "/folders/root");
    return {
      folder: data.folder ?? null,
      folders: data.folders ?? [],
      files: data.files ?? [],
    } satisfies FolderContents;
  },

  folder: async (id: string) => {
    const data = await request<FolderContents>("GET", `/folders/${id}`);
    return {
      folder: data.folder ?? null,
      folders: data.folders ?? [],
      files: data.files ?? [],
    } satisfies FolderContents;
  },

  breadcrumb: async (id: string) => {
    const data = await request<{ breadcrumb: { id: string; name: string }[] }>(
      "GET",
      `/folders/${id}/breadcrumb`,
    );
    return data.breadcrumb ?? [];
  },

  computers: async () => {
    const data = await request<{ computers: Computer[] }>("GET", "/computers");
    return data.computers ?? [];
  },

  listFiles: async (params: {
    folder_id?: string;
    search?: string;
    starred?: boolean;
    sort?: SortKey;
    dir?: SortDir;
    page_size?: number;
  }) => {
    const q = new URLSearchParams();
    if (params.folder_id) q.set("folder_id", params.folder_id);
    if (params.search) q.set("search", params.search);
    if (params.starred) q.set("starred", "true");
    if (params.sort) q.set("sort", params.sort);
    if (params.dir) q.set("dir", params.dir);
    q.set("page_size", String(params.page_size ?? 100));
    const data = await request<FilesListResponse>("GET", `/files?${q.toString()}`);
    return {
      files: data.files ?? [],
      total: data.total ?? 0,
      page: data.page ?? 1,
    };
  },

  myActivity: async (pageSize = 50) => {
    const data = await request<{ activities: ActivityLog[] | null; total: number }>(
      "GET",
      `/activity?page_size=${pageSize}`,
    );
    return data.activities ?? [];
  },

  trashedFiles: async () => {
    const data = await request<{ files: FileItem[] | null }>("GET", "/files/trash");
    return data.files ?? [];
  },

  trashedFolders: async () => {
    const data = await request<{ folders: FolderItem[] | null }>("GET", "/folders/trash");
    return data.folders ?? [];
  },

  sharedWithMe: async () => {
    const data = await request<SharedItem[] | { shares?: SharedItem[]; items?: SharedItem[] }>(
      "GET",
      "/shares/with-me",
    );
    if (Array.isArray(data)) return data;
    return data.shares ?? data.items ?? [];
  },
};
