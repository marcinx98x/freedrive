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
| `POST` | `/api/v1/folders` | Create or resolve folder under computer root |
| `GET` | `/api/v1/folders/{id}` | List folder contents (poll / mirror) |

## Files

| Method | Path | Purpose |
|--------|------|---------|
| `POST` | `/api/v1/files/upload` | Upload new encrypted file (multipart) |
| `PUT` | `/api/v1/files/{id}/content` | Update file content |
| `GET` | `/api/v1/files/{id}/download` | Download encrypted blob |

## Implementation reference

Rust client: [`desktop/src-tauri/src/api/client.rs`](../desktop/src-tauri/src/api/client.rs)

## Versioning

- Server releases: git tags `v*`
- Desktop releases: git tags `desktop-v*`
- API compatibility is maintained manually; document breaking changes in release notes for both components.
