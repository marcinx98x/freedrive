export interface TokenPair {
  access_token: string;
  refresh_token: string;
  expires_in: number;
}

export interface User {
  id: string;
  email: string;
  username: string;
  role: string;
  quota_bytes: number;
  used_bytes: number;
  avatar_url?: string;
  suspended: boolean;
  email_2fa_enabled: boolean;
  two_factor_required?: boolean;
  created_at: string;
  updated_at: string;
  last_login_at?: string;
}

export interface LoginSuccess {
  tokens: TokenPair;
  user: User;
}

export interface Login2FAChallenge {
  requires_2fa: true;
  challenge_id: string;
  email_masked: string;
}

export type LoginResult = LoginSuccess | Login2FAChallenge;

export function is2FAChallenge(result: LoginResult): result is Login2FAChallenge {
  return "requires_2fa" in result && result.requires_2fa === true;
}

export interface FolderItem {
  id: string;
  name: string;
  parent_id?: string | null;
  owner_id: string;
  color?: string;
  is_starred: boolean;
  is_trashed: boolean;
  trashed_at?: string | null;
  created_at: string;
  updated_at: string;
}

export interface FileItem {
  id: string;
  name: string;
  mime_type: string;
  size: number;
  encrypted_size: number;
  folder_id?: string | null;
  owner_id: string;
  iv: string;
  version: number;
  is_starred: boolean;
  is_trashed: boolean;
  trashed_at?: string | null;
  created_at: string;
  updated_at: string;
  accessed_at?: string;
}

export interface FolderContents {
  folder: FolderItem | null;
  folders: FolderItem[];
  files: FileItem[];
}

export interface Computer {
  id: string;
  owner_id: string;
  name: string;
  hostname?: string;
  root_folder_id: string;
  last_seen_at?: string | null;
  created_at: string;
  updated_at: string;
}

export interface BreadcrumbItem {
  id: string;
  name: string;
}

export interface FilesListResponse {
  files: FileItem[] | null;
  total: number;
  page: number;
}

export interface SharedItem {
  share: {
    id: string;
    file_id?: string | null;
    folder_id?: string | null;
    shared_by: string;
    shared_with: string;
    permission: string;
    created_at: string;
  };
  item_type: "file" | "folder" | string;
  item_id: string;
  item_name: string;
  owner_id?: string;
  owner_name?: string;
  owner_email?: string;
}

export type ViewMode = "list" | "grid";
export type SortKey = "name" | "updated_at";
export type SortDir = "asc" | "desc";
