<p align="center">
  <img src="docs/favicon.svg" width="78" alt="FreeDrive Logo" />
</p>

<h1 align="center">FreeDrive</h1>

<p align="center">
  <strong>Self-hosted cloud storage with a familiar Drive-like UX.</strong><br/>
  Single Go binary, embedded SQLite, disk-backed storage, admin panel, and modern web UI.
</p>
<p align="center"><strong>Licensed under MIT</strong></p>

<p align="center">
  <a href="https://github.com/marcinx98x/freedrive"><img src="https://img.shields.io/badge/Website-freedrive-blue?style=flat-square" alt="Website"/></a>
  <a href="https://github.com/marcinx98x/freedrive/releases"><img src="https://img.shields.io/github/v/release/marcinx98x/freedrive?style=flat-square" alt="Release"/></a>
  <a href="https://github.com/marcinx98x/freedrive/stargazers"><img src="https://img.shields.io/github/stars/marcinx98x/freedrive?style=flat-square" alt="Stars"/></a>
  <a href="https://github.com/marcinx98x/freedrive/blob/master/LICENSE"><img src="https://img.shields.io/github/license/marcinx98x/freedrive?style=flat-square" alt="License"/></a>
  <a href="https://github.com/marcinx98x/freedrive"><img src="https://img.shields.io/github/go-mod/go-version/marcinx98x/freedrive?style=flat-square" alt="Go version"/></a>
  <a href="https://hub.docker.com/r/marcinx98x/freedrive"><img src="https://img.shields.io/docker/pulls/marcinx98x/freedrive?style=flat-square" alt="Docker pulls"/></a>
</p>

---

## Table of Contents

- [Overview](#overview)
- [Screenshots](#screenshots)
- [Core Features](#core-features)
- [Admin Capabilities](#admin-capabilities)
- [Architecture](#architecture)
- [Security Model](#security-model)
- [Quick Start](#quick-start)
- [Production Install (systemd)](#production-install-systemd)
- [Configuration](#configuration)
- [API Reference](#api-reference)
- [Project Structure](#project-structure)
- [Deployment Options](#deployment-options)
- [Operations](#operations)
- [Star History](#star-history)
- [Troubleshooting](#troubleshooting)
- [Contributing](#contributing)
- [License](#license)

---

## Overview

FreeDrive is an open-source, self-hosted storage platform designed to feel instantly familiar for users coming from mainstream cloud drives.

What makes it practical:

- Single binary backend (`Go`) with embedded web assets
- Embedded SQLite database (no external DB required)
- Local disk storage backend
- JWT access + refresh-token authentication
- Optional email-based two-factor authentication (2FA)
- User profile settings and secure email change flow
- User and admin workspaces in one application
- Simple deployment with direct binary run or `systemd`

FreeDrive is ideal for:

- Developers wanting full ownership of files and auth
- Small teams needing private internal storage
- Self-hosting enthusiasts who want low operational overhead

---

## Screenshots

### User Workspace

![User Workspace](docs/screenshots/user.png)

### Admin Workspace

![Admin Workspace](docs/screenshots/admin.png)

---

## Core Features

### 1. Drive-like File Management UX

- Folder-based navigation and root view
- Unified page headers across Home, My Drive, Recent, Starred, Shared with me, Trash, and other tabs
- Home Suggested files: list view with Name / Reason suggested / Owner / Location (parent folder name), or grid tiles
- List/grid view switching with Recent and Starred collections
- Owner column shows avatar + `me` for your own items, or avatar + owner name for others
- Global live search in the top bar on every page
- Advanced search panel (Drive-style filters: type, owner, location, modified, trash, starred, approvals, and more)
- Context menus and keyboard shortcuts

### 2. Sidebar: My Drive & Computers

- **My Drive** — primary file space with an expandable sidebar folder tree (lazy-loaded folders, expand/collapse chevrons, path sync on navigation)
- **Computers** — separate sidebar section for desktop backup/sync (isolated from My Drive root folders); the [`desktop/`](desktop/) Tauri client registers here and syncs local folders
- Drive-style pill highlights on nav rows, with chevrons inside the active/hover area

### 3. File Lifecycle

- Upload files via web UI
- Download encrypted blob payloads with metadata headers
- Rename and move files between folders
- Soft delete to Trash (files and folders, including folder subtrees)
- Restore from Trash (files and folders)
- Permanent delete (files and folders)
- Scheduled and admin trash purge removes old trashed **files** (blobs + rows) and **folders** (metadata rows)

### 4. Versioning Support

- File version records are kept when content is updated (can be disabled in admin settings)
- Configurable `keep_versions` retention per file
- List versions per file
- Restore an earlier version

### 5. User Profile & Security

- Profile settings modal (name, avatar photo)
- Forgot-password flow with SQLite-persisted reset tokens (survives server restart; single-use)
- **Cross-device encryption** — account key (UEK) and per-file keys sync via server; unlock with password on web or desktop to decrypt files on any device
- **Recovery code** — emergency unlock when password is forgotten (set up on first encryption or after key rotation)
- **Key rotation** — re-wrap account and file keys with a new password-derived key from Settings (web and desktop)
- Password reset can re-wrap the account key when crypto metadata is supplied
- Secure email change with confirmation link sent to the new address
- Security center for per-user email 2FA toggle
- When admin enables global `require_2fa`, all users must verify a 6-digit code at sign-in

### 6. Sharing Model

- User-to-user sharing data model (`user_shares`) with role-based access control enforced on read/write mutations
- Share-link data model (`share_links`) with optional password, expiry, and download limits
- **Shared with me** sidebar view for inbound shares (Google Drive-style — no separate “Shared by me” tab)
- Outbound shares stay in **My Drive**; manage recipients in the Share dialog; shared items show a people icon in the file list
- Find items you shared via advanced search: **Owner: Me** + **Shared to** (email)
- `GET /shares/by-me` API supports Share dialog, details panel, and search (not a separate nav view)
- Update share permission (`PATCH /shares/users/{id}`) and folder shares in shared listings

### 7. Comments & Approvals

- File comments with optional assignee (`assigned_to_email`) — visible in the details Activity tab
- Advanced search follow-ups: “Comments assigned to me only”, approval awaiting / requested filters
- Request approval from context menu or file details; approver can approve/reject in Activity tab
- `GET /approvals`, `POST /files/{id}/approvals`, `PATCH /approvals/{id}` workflow

### 8. Storage & Quota Awareness

- Per-user quota enforcement during uploads/content updates
- Server-wide capacity limit (`total_capacity_gb`) enforced from admin settings
- Used-bytes accounting on delete/restore/permanent-delete paths
- Disk usage endpoint for runtime visibility

### 9. Activity Logging

- File/folder actions are recorded in activity logs
- Login and failed-login events with client IP
- User and admin activity listing endpoints

### 10. Embedded App Delivery

- Frontend is embedded with `go:embed`
- Single process serves API + SPA + static assets

---

## Admin Capabilities

Admin routes are role-protected and available under `/api/v1/admin/*`.

In the Drive UI, users with the `admin` role see a shield icon in the top bar (next to Security) on every tab; it opens the admin panel at `/admin/dashboard`. The icon is hidden in admin-panel mode and for non-admin accounts.

### User Management

- List users
- Create users
- Update role, quota, username, email, suspension, and per-user 2FA
- Delete users (with self-delete protection)
- Trigger password reset email flow
- Revoke user sessions / revoke all sessions
- Send 2FA reminder emails to users without 2FA enabled

### Invite System

- Create invite links with:
  - role
  - max uses
  - quota bytes
- List invites
- Invite usage tracking and expiration checks

### Operational Controls

- View aggregate stats (`total_users`, `total_used`, `total_quota`)
- View global activity feed
- Save/retrieve admin settings (persisted to `data/settings.json`)
- Run backup snapshot for admin settings
- Scheduled settings backup (daily / weekly / monthly)
- Storage tools: purge trash (files + folders), list/purge duplicate blobs, wipe all data (danger zone)
- Trash auto-empty scheduler (`storage.trash_auto_empty`: 7 / 30 / 90 days or never)

### Security & Access Policy

Admin settings are enforced at runtime (not UI-only):

- **IP blocklist / allowlist** — applied on login, register, refresh, reset-password, and 2FA verification
- **Require 2FA for all users** — forces email verification at sign-in (no admin exemption)
- **Versioning** — enable/disable file versioning and set `keep_versions`
- **Total capacity** — server-wide storage cap blocks uploads and content updates when exceeded
- **Allowed file types** — extension whitelist in General settings; enable **Without limits** to accept any file type

Security panel also surfaces suspicious logins, active sessions, and session revocation.

### Email / SMTP

- SMTP test endpoint
- Password reset email dispatch
- Email change confirmation links
- Email 2FA sign-in codes
- 2FA reminder batch emails
- Configurable sender and TLS behavior

---

## Architecture

FreeDrive follows a clean layered structure:

- `api` layer: HTTP routes, handlers, middleware
- `service` layer: business logic (auth, file, folder)
- `repository` layer: persistence interfaces + SQLite implementations
- `storage` layer: disk blob IO

Runtime flow summary:

1. Request hits `chi` router
2. Global middleware stack executes (CORS, rate-limit, recover, logger)
3. Auth middleware validates JWT when required
4. Handler validates input and calls service/repo
5. Service applies policy (quota, ownership, versioning, capacity, activity, IP rules)
6. Response serialized as JSON

Admin settings are read from `data/settings.json` via the `adminsettings` package and applied consistently across handlers and services.

---

## Security Model

### Authentication

- Access token: JWT
- Refresh token: random token, stored hashed in DB
- Token rotation on refresh
- Logout revokes refresh token
- Optional **email 2FA**: 6-digit code sent via SMTP after password verification
- Global `require_2fa` admin setting forces 2FA for every account

### Authorization

- Protected API group requires valid access token
- Admin routes use explicit admin-role middleware
- User-scoped operations enforce ownership checks in services

### Network Access Control

- IP blocklist and allowlist from admin settings
- Enforced on all public auth endpoints (login, register, refresh, reset-password, verify-2fa)
- Loopback addresses (`127.0.0.1`, `::1`) are always allowed

### Secrets

- `FREEDRIVE_JWT_SECRET` can be provided via env
- If omitted, it is generated and stored in `data/jwt_secret.key`

### Rate Limiting

Global limiter enabled in router:

- `100 req/sec`
- `burst 400`

### Storage Note

File payloads are encrypted client-side (AES-GCM) before upload. The server stores ciphertext blobs and **wrapped** encryption keys only — it never receives the user's raw account key (UEK) or per-file keys.

- **Web UI** — WebCrypto encrypts/decrypts in the browser; keys are wrapped with a password-derived key and synced via `/api/v1/crypto/*`
- **Desktop client** — same UEK + file-key model; keys sync on sign-in, UEK cached in OS keyring, pending file keys flushed when back online
- **Unlock flows** — password unlock at sign-in; recovery code in Settings when locked; key rotation re-wraps all file keys
- **Password reset** — optional crypto metadata lets users re-wrap the account key without losing access to existing files (when recovery was configured)
- On plain HTTP (non-localhost), the web UI warns and may upload without browser-side encryption

If your threat model requires strict end-to-end guarantees, review key handling and server-side wrapped-key storage before production rollout.

---

## Quick Start

### Prerequisites

- Go (matching `go.mod` requirements)
- Linux/macOS/WSL recommended for local development

### Run Locally

```bash
go mod download
go run ./cmd/freedrive
```

Open:

- `http://localhost:8080`

Default bootstrap admin (if first user is auto-created):

- Email: `admin@freedrive.local`
- Password: `admin123`

Important: change defaults immediately in non-dev environments.

### Run Published Docker Image

Images are built by [GitHub Actions](.github/workflows/docker-publish.yml) on push to `master` and published to:

- **Docker Hub:** [`marcinx98x/freedrive`](https://hub.docker.com/r/marcinx98x/freedrive) — public, no login required
- **GHCR:** `ghcr.io/marcinx98x/freedrive` — may require GitHub login if the package is private

Tags: `latest`, `master`, `sha-<commit>`. Multi-arch: `linux/amd64`, `linux/arm64`.

**Docker Hub (recommended):**

```bash
docker pull marcinx98x/freedrive:latest
docker run -d \
  --name freedrive \
  -p 8080:8080 \
  -e FREEDRIVE_ADMIN_EMAIL=admin@freedrive.local \
  -e FREEDRIVE_ADMIN_PASSWORD=change-me-now \
  -v freedrive-data:/app/data \
  marcinx98x/freedrive:latest
```

**GHCR** (log in first if the package is private; GitHub PAT with `read:packages` scope):

```bash
echo $GITHUB_TOKEN | docker login ghcr.io -u marcinx98x --password-stdin
docker pull ghcr.io/marcinx98x/freedrive:latest
docker run -d \
  --name freedrive \
  -p 8080:8080 \
  -e FREEDRIVE_ADMIN_EMAIL=admin@freedrive.local \
  -e FREEDRIVE_ADMIN_PASSWORD=change-me-now \
  -v freedrive-data:/app/data \
  ghcr.io/marcinx98x/freedrive:latest
```

To make the GHCR package publicly pullable without login: GitHub → **Packages** → **freedrive** → **Package settings** → **Change visibility** → **Public**.

### Run With Docker Compose

`docker-compose.yml` pulls `marcinx98x/freedrive:latest` from Docker Hub, runs a single `freedrive` service, and maps a host folder to `/app/data` through a bind mount. To use GHCR instead, set `image: ghcr.io/marcinx98x/freedrive:latest` in the compose file.

Before the first start, edit `docker-compose.yml`: set a strong `FREEDRIVE_ADMIN_PASSWORD` (and `FREEDRIVE_ADMIN_EMAIL`), adjust the published port if needed, and change the bind-mount path (`/volume2/docker/freedrive/data`) to a folder that exists on your host.

```bash
docker compose pull
docker compose up -d
```

Open:

- `http://localhost:8080`

Runtime data (the `freedrive.db` database, encrypted `blobs/`, and `jwt_secret.key`) lives in the host folder you mapped to `/app/data`, so it survives container recreation and image updates.

### Automatic Updates (Watchtower, optional)

The default `docker-compose.yml` does not include Watchtower. If you want automatic updates, add a [Watchtower](https://containrrr.dev/watchtower/) container that periodically checks Docker Hub for a newer `latest` image and recreates the FreeDrive container:

- Run Watchtower with `--cleanup --label-enable --interval 3600` (hourly; `--cleanup` removes the old image after each update).
- Add the label `com.centurylinklabs.watchtower.enable=true` to the `freedrive` service so Watchtower only updates FreeDrive and leaves your other containers untouched.

### Synology: Update Without Losing Data

Important: your data is NOT deleted on update. The database (`freedrive.db`), the encrypted blobs (`blobs/`) and the `jwt_secret.key` all live in the folder you mapped to `/app/data`. They stay on the NAS.

What actually goes wrong: when you press "Update" or delete and recreate a single container in Container Manager, the GUI can drop the `/app/data` mapping (or you recreate it without re-selecting the exact same folder). The new container then starts on an empty `/app/data` inside its own writable layer, so the app shows no users and cannot read files — even though the real data is untouched in your folder. Do NOT rely on manual delete + recreate for updates.

**Step 0 — confirm your data is safe (SSH):**

```bash
ls -la /volume1/<your-path>/freedrive/data
# expected: freedrive.db, blobs/, jwt_secret.key
```

**Method A (recommended): run FreeDrive as a Project (compose) with a bind mount to your folder.**

The shipped `docker-compose.yml` already uses a host bind mount, so the mapping lives in the file and can never be dropped. Point it at a folder on your NAS:

```yaml
    volumes:
      - /volume1/<your-path>/freedrive/data:/app/data
```

Update from the Project view via pull + up (`docker compose pull && docker compose up -d`). Because the mapping lives in the file, every recreation reuses the exact same data folder.

**Method B (optional): add Watchtower for automatic updates — the mapping can never be lost.**

Watchtower clones the running container's full configuration (including your `/app/data` bind mount) before pulling the new image and recreating it, so the folder mapping is always preserved.

1. Add the label `com.centurylinklabs.watchtower.enable=true` to your `freedrive` container.
2. Run a Watchtower container (via Container Manager or a separate compose service) with `--cleanup --label-enable --interval 3600`.
3. Stop pressing "Update" in the GUI — Watchtower now updates FreeDrive on its own without touching your data.

**If you must recreate manually:** your data is safe in the folder; you only have to map that exact same folder back to `/app/data` when creating the new container. Then verify the container is reading it:

```bash
docker inspect --format '{{json .Mounts}}' freedrive
# the mount source must point to your /volume1/<your-path>/freedrive/data folder
```

**Verify you are actually running the new build (SSH):**

```bash
# digest the container is currently running
docker inspect --format '{{.Image}}' freedrive
# digest of the local latest image
docker inspect --format '{{index .RepoDigests 0}}' marcinx98x/freedrive:latest
```

After the app is updated, the browser fetches fresh frontend assets automatically (the server sends `ETag` + `Cache-Control: no-cache`); a hard refresh (Ctrl+F5) is only needed if you loaded a version built before this behavior existed.

---

## Production Install (systemd)

The repository includes `scripts/install.sh` for Linux host installation.

```bash
chmod +x scripts/install.sh
./scripts/install.sh
```

What it does:

- Prompts for admin credentials
- Downloads the latest release binary
- Installs binary to `/opt/freedrive/freedrive`
- Writes env file at `/etc/freedrive/freedrive.env`
- Creates/starts `freedrive.service`

To update an existing systemd installation to the latest release:

```bash
curl -fsSL https://abdullaabdullazade.github.io/freedrive/update.sh -o update.sh
chmod +x update.sh
./update.sh
```

The updater verifies the release checksum, keeps your existing data and `/etc/freedrive/freedrive.env`, backs up the current binary to `/opt/freedrive/freedrive.bak`, installs the new binary, and restarts `freedrive.service`.

Operational commands:

```bash
sudo systemctl status freedrive
sudo systemctl restart freedrive
sudo journalctl -u freedrive -f
```

Browser encryption note: uploads use WebCrypto when the browser allows it. Use `http://localhost:8080` or HTTPS for encrypted uploads; on plain HTTP server addresses, FreeDrive warns first and uploads without browser-side encryption.

Note: current systemd template runs service as `root`. For hardened production setups, consider a dedicated system user and tighter filesystem permissions.

---

## Configuration

Environment variables loaded by `internal/config/config.go`:

| Variable | Description | Default |
|---|---|---|
| `FREEDRIVE_PORT` | HTTP port | `8080` |
| `FREEDRIVE_DATA_DIR` | Data directory (DB, blobs, keys) | `./data` |
| `FREEDRIVE_JWT_SECRET` | JWT signing secret | auto-generated if empty |
| `FREEDRIVE_MAX_UPLOAD_MB` | Max upload size (MB) | `5120` |
| `FREEDRIVE_ADMIN_EMAIL` | Initial admin email | `admin@freedrive.local` |
| `FREEDRIVE_ADMIN_PASSWORD` | Initial admin password | `admin123` |

---

## API Reference

Base path: `/api/v1`

### Public Auth

- `POST /auth/register`
- `POST /auth/login` — returns tokens, or `{ requires_2fa, challenge_id, email_masked }` when 2FA is required
- `POST /auth/verify-2fa` — complete login with `{ challenge_id, code }`
- `POST /auth/refresh`
- `POST /auth/logout`
- `POST /auth/forgot-password` — `{ email }` — sends reset link if account exists (requires SMTP); generic response either way
- `POST /auth/reset-password` — `{ token, email, new_password, crypto_update? }` — consumes SQLite-stored token (survives server restart); optional `crypto_update` re-wraps the account encryption key after password change
- `POST /auth/reset-password/crypto-info` — `{ token, email }` — returns wrapped account key metadata for password-reset flows that preserve file access
- `POST /auth/confirm-email` — confirm pending email change from link

### Protected (Authenticated)

- `GET /me` — current user profile
- `PATCH /me` — update username, avatar, or `email_2fa_enabled`
- `POST /me/email-change/request` — start secure email change (confirmation link to new address)
- `GET /me/email-change/status` — pending email change status
- `GET /me/storage`
- `GET /activity`
- `GET /disk-stats`
- `GET /search` — advanced search (query + filters: type, owner, location, trash, starred, modified, approvals, follow-ups, …)
- `GET /approvals` — list approvals for current user (`?status=pending` optional)
- `PATCH /approvals/{id}` — `{ status: "approved" | "rejected" }` (approver only)

#### Encryption key sync

Password-wrapped account keys and per-file keys for cross-device decrypt. The server stores wrapped keys only; clients derive the UEK locally from the user's password.

- `GET /crypto/account` — account crypto metadata (`has_crypto`, `key_salt`, `wrapped_uek`, optional recovery wrap)
- `POST /crypto/account` — first-time setup (`key_salt`, `wrapped_uek`, optional `wrapped_uek_recovery`)
- `PUT /crypto/account` — update wraps after password change
- `GET /encryption-keys` — list file keys for current user (`?since=` for incremental sync)
- `POST /encryption-keys/bulk` — bulk import wrapped file keys after upload batches
- `GET /files/{id}/encryption-key` — wrapped key for one file
- `PUT /files/{id}/encryption-key` — store wrapped key for one file

#### Shares

User-to-user sharing and public links. Permissions: `viewer`/`commenter` → read; `editor` → write. Access is enforced on file/folder get, download, breadcrumb, and mutations.

- `GET /shares/with-me` — items shared with the current user (sidebar **Shared with me**)
- `GET /shares/by-me` — items the current user has shared (used by Share dialog and advanced search; no separate nav view)
- `POST /shares/users` — `{ file_id?, folder_id?, shared_with?, shared_email?, permission }`
- `PATCH /shares/users/{id}` — `{ permission }`
- `DELETE /shares/users/{id}`
- `GET /shares/links` — list share links created by the user
- `POST /shares/links` — `{ file_id?, folder_id?, permission, password?, expires_at?, max_downloads? }`
- `DELETE /shares/links/{id}`

#### Files

- `POST /files/upload`
- `GET /files`
- `GET /files/trash`
- `GET /files/{id}`
- `GET /files/{id}/comments`
- `POST /files/{id}/comments` — `{ content, parent_id?, assigned_to_email? }`
- `DELETE /files/{id}/comments/{commentId}`
- `POST /files/{id}/approvals` — `{ approver_id?, approver_email? }` (requires write access)
- `GET /files/{id}/download`
- `PATCH /files/{id}`
- `POST /files/{id}/content`
- `DELETE /files/{id}`
- `POST /files/{id}/restore`
- `DELETE /files/{id}/permanent`
- `GET /files/{id}/versions`
- `POST /files/{id}/versions/{version}/restore`

#### Folders

- `POST /folders`
- `GET /folders/root`
- `GET /folders/all`
- `GET /folders/trash`
- `GET /folders/{id}`
- `PATCH /folders/{id}`
- `DELETE /folders/{id}`
- `POST /folders/{id}/restore`
- `DELETE /folders/{id}/permanent`
- `GET /folders/{id}/breadcrumb`

#### Computers

- `GET /computers`
- `GET /computers/{id}`
- `POST /computers/register`
- `POST /computers/{id}/heartbeat`

### Public (no auth)

- `GET /public/share/{token}` — share link metadata (`?password=` if protected)
- `GET /public/share/{token}/download` — download file via share link (`?password=` if protected)

### Admin (Requires `admin` role)

- `GET /admin/users`
- `POST /admin/users`
- `PATCH /admin/users/{id}`
- `DELETE /admin/users/{id}`
- `POST /admin/users/{id}/reset-password`
- `POST /admin/users/{id}/revoke-sessions`
- `POST /admin/users/send-2fa-reminder`
- `POST /admin/sessions/revoke-all`
- `GET /admin/stats`
- `POST /admin/invites`
- `POST /admin/invites/resend`
- `GET /admin/invites`
- `DELETE /admin/invites/{id}`
- `GET /admin/activity`
- `GET /admin/settings`
- `POST /admin/settings`
- `POST /admin/test-email`
- `POST /admin/backup/run`
- `GET /admin/backup/list`
- `GET /admin/backup/download/{filename}`
- `POST /admin/backup/restore`
- `DELETE /admin/backup/{filename}`
- `POST /admin/storage/purge-trash?days=30` — permanently delete trashed files (blobs + rows) and folder rows older than N days; `days=0` purges all trash. Response: `{ removed_files, removed_folders, freed_bytes }`. Background auto-empty uses the `storage.trash_auto_empty` setting (7 / 30 / 90 / never).
- `GET /admin/storage/duplicates`
- `POST /admin/storage/duplicates/purge`
- `POST /admin/danger/wipe`

### Health

- `GET /health`

---

## Desktop Client (beta)

The [`desktop/`](desktop/) directory contains the **FreeDrive Desktop** sync app (Tauri 2 + React + Rust). It talks to the server over the same REST API as the web UI.

- Sign in, onboarding, folder sync, system tray, pause/resume
- **Cross-device decryption** — syncs password-wrapped account and file keys from the server; Explorer hydration decrypts files with the same keys as the web UI
- **Encryption status** — top bar shows Active/Locked; Settings supports recovery-code unlock and key rotation
- **Google Drive-style UI** — sidebar with SVG icons (Home, Sync activity, Notifications) and alert badge
- **Notifications** — alerts for sync errors, paused sync, and low storage (≥80% / ≥90%)
- **Profile menu** — server avatar from `GET /api/v1/me`, storage bar, Sign out / Sign in with another account
- **Silent background sync** — on restart, background verification without a full UI rescan (`Processing N/M`)
- **Windows Explorer (CfAPI)** — after sign-in, with the app running in the tray, open `%USERPROFILE%\FreeDrive\My Drive` in File Explorer (Windows 10 1809+); provider reconnects automatically before opening the folder
- **Explorer status** — desktop app exposes integration state (connected / registered / finalized) for diagnostics
- **My Drive in Explorer** — subfolder with server folders/files as cloud placeholders; files download when opened
- Independent release tags: `desktop-v0.1.0` (server tags remain `v1.x.x`)
- See [`desktop/README.md`](desktop/README.md) for dev setup, Explorer troubleshooting, and [`docs/desktop-api.md`](docs/desktop-api.md) for API endpoints used by the client

Quick start (from repo root):

```bash
go run ./cmd/freedrive          # terminal 1 — server
cd desktop && npm install && npm run tauri dev   # terminal 2 — desktop
```

---

## Project Structure

```text
cmd/freedrive/
  main.go                 # app bootstrap
  web/                    # embedded frontend (HTML/CSS/JS)

internal/
  adminsettings/          # persisted admin policy (IP, 2FA, capacity, backup)
  api/
    router.go             # route graph + middleware wiring
    handlers/             # HTTP handlers
    middleware/           # auth, CORS, rate limit, client IP
  config/                 # env config + secret generation
  domain/                 # core entities
  email/                  # shared SMTP sender
  repository/             # interfaces
  repository/sqlite/      # sqlite repos + migrations
  service/                # business logic
  storage/                # local disk blob storage

scripts/
  install.sh              # systemd installation helper

docs/
  index.html              # project landing page
  screenshots/            # marketing screenshots
  desktop-api.md          # REST endpoints used by the desktop client

desktop/                  # Tauri desktop sync client (React + Rust)
  src/                    # frontend UI
  src-tauri/              # native shell, sync engine, API client
  package.json
  README.md               # build & dev instructions
```

---

## Deployment Options

Typical demo/production options:

- VPS (`Hetzner`, `DigitalOcean`, `AWS EC2`) with systemd
- Containerized self-managed deployment (custom Dockerfile)
- PaaS for demo environments (`Railway`, `Render`, `Fly.io`)

Recommended production baseline:

- Reverse proxy (`Caddy` or `Nginx`) with HTTPS
- Periodic backup for `FREEDRIVE_DATA_DIR`
- Strong admin password and rotated JWT secret policy
- Non-root runtime user when possible

---

## Operations

### Data You Should Back Up

At minimum:

- SQLite DB (`$FREEDRIVE_DATA_DIR/*.db`)
- Blob storage (`$FREEDRIVE_DATA_DIR` file hierarchy)
- JWT secret (`$FREEDRIVE_DATA_DIR/jwt_secret.key` if auto-generated)
- Admin settings snapshot (`data/settings.json` and optional backup output)

### Upgrading

1. Stop service
2. Replace binary
3. Start service
4. Check logs and health endpoint

```bash
sudo systemctl stop freedrive
# replace binary
sudo systemctl start freedrive
curl -s http://localhost:8080/api/v1/health
```

---

## Star History

[![Star History Chart](https://api.star-history.com/svg?repos=marcinx98x/freedrive&type=Date&legend=top-left)](https://www.star-history.com/?repos=marcinx98x%2Ffreedrive&type=date&legend=top-left)

---

## Troubleshooting

### Service starts but UI fails

- Confirm process is running: `systemctl status freedrive`
- Check logs: `journalctl -u freedrive -f`
- Validate port mapping / firewall

### Login fails unexpectedly

- Verify JWT secret consistency across restarts
- Ensure system clock is correct
- Confirm refresh token table integrity
- Check whether your IP is blocked or not on the allowlist (Admin → Security)
- If 2FA is enabled, confirm SMTP is configured and the code has not expired (10 minutes)

### Upload returns size/form errors

- Check `FREEDRIVE_MAX_UPLOAD_MB`
- Ensure reverse proxy request body limits are aligned
- Check admin `total_capacity_gb` if uploads fail with a capacity error

### Email 2FA unavailable at sign-in

- Admin must configure SMTP under Admin → Settings → Email
- Global `require_2fa` cannot be satisfied without working outbound email
- Users can enable personal 2FA from Security in the profile menu

### SMTP test/reset mail fails

- Re-check server/port/auth/TLS settings
- Verify sender domain policy (SPF/DKIM/relay restrictions)

---

## Contributing

Contributions are welcome.

Suggested workflow:

1. Fork repository
2. Create feature branch
3. Add/adjust tests where applicable
4. Submit focused PR with clear change summary

If you are proposing architecture-level changes, open an issue first for design alignment.

---

## License

MIT License. See [LICENSE](LICENSE).
