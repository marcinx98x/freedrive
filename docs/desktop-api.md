# Desktop client API contract

The FreeDrive desktop app (`desktop/`) uses the server REST API under `/api/v1`. It does not embed server code; breaking changes to these endpoints require a desktop client update.

Base URL: configured at sign-in (e.g. `http://localhost:8080`).

## Authentication

| Method | Path | Purpose |
|--------|------|---------|
| `POST` | `/api/v1/auth/login` | Email/password login |
| `POST` | `/api/v1/auth/verify-2fa` | Complete 2FA challenge |
| `POST` | `/api/v1/auth/refresh` | Refresh access token |
| `POST` | `/api/v1/auth/logout` | End session |

## Computers (device registration)

| Method | Path | Purpose |
|--------|------|---------|
| `POST` | `/api/v1/computers/register` | Register this PC, get root folder |
| `POST` | `/api/v1/computers/{id}/heartbeat` | Keep-alive |

## Folders

| Method | Path | Purpose |
|--------|------|---------|
| `POST` | `/api/v1/folders` | Create under parent; idempotent — reuses a live same-name folder or restores a trashed one (UNIQUE-safe for desktop nested sync) |
| `GET` | `/api/v1/folders/root` | My Drive root contents (Windows Explorer CfAPI) |
| `GET` | `/api/v1/folders/{id}` | List folder contents (poll / mirror) |

Query params for `GET /folders/root` and `GET /folders/{id}`:

- `page_size` — files per page (default **100**, max **500**)
- `page_token` — opaque offset from previous `next_page_token`

Response includes `folders` (full child-folder list on the first page only), `files` (one page), `next_page_token`, and `total_files`. The desktop client always walks every page so sync/orphan reconcile sees the complete folder.

## Files

| Method | Path | Purpose |
|--------|------|---------|
| `POST` | `/api/v1/files/upload` | Upload new encrypted file (multipart; used when ciphertext ≤ 32 MiB) |
| `POST` | `/api/v1/files/{id}/content` | Update file content (multipart; small payloads) |
| `GET` | `/api/v1/files/{id}/download` | Download encrypted blob |

## Resumable uploads

Used by desktop (and web/mobile) when encrypted size **> 32 MiB** so uploads work behind Cloudflare (~100 MB request limit). Chunk hint: **8 MiB**.

| Method | Path | Purpose |
|--------|------|---------|
| `POST` | `/api/v1/uploads/sessions` | Start session (JSON metadata: name, mime_type, iv, sizes, optional folder_id / file_id) |
| `PUT` | `/api/v1/uploads/sessions/{id}` | Upload chunk (`Content-Range: bytes start-end/total`); final chunk returns `File` |
| `GET` | `/api/v1/uploads/sessions/{id}` | Session status / resume offset |
| `DELETE` | `/api/v1/uploads/sessions/{id}` | Abort session |

## Shares

| Method | Path | Purpose |
|--------|------|---------|
| `GET` | `/api/v1/shares/with-me` | List files and folders shared with the current user (Home panel) |

## Implementation reference

Rust client: [`desktop/src-tauri/src/api/client.rs`](../desktop/src-tauri/src/api/client.rs)

## Windows Explorer (CfAPI)

On Windows 10 1809+ the desktop app registers a **Cloud Files** sync root at `%USERPROFILE%\FreeDrive\` after sign-in. Explorer shows **FreeDrive** in the navigation pane and under **This PC**. The `My Drive\` subfolder lists server content as cloud placeholders; files hydrate on open via `GET /api/v1/files/{id}/download`.

- Registration: `desktop/src-tauri/src/cfapi/`
- Placeholder cache: SQLite table `my_drive_placeholders` in `sync.db`
- Requires the desktop app to be running while browsing placeholders

## Versioning

- Server releases: git tags `v*`
- Desktop releases: git tags `desktop-v*`
- API compatibility is maintained manually; document breaking changes in release notes for both components.
