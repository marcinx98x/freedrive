<p align="center">
  <img src="docs/favicon.svg" width="78" alt="FreeDrive Logo" />
</p>

<h1 align="center">FreeDrive</h1>

<p align="center">
  <strong>Self-hosted cloud storage with a Drive-like UX.</strong><br/>
  Single Go binary · SQLite · disk blobs · web UI · desktop sync · Android app
</p>
<p align="center"><strong>MIT License</strong></p>

<p align="center">
  <a href="https://github.com/marcinx98x/freedrive/releases"><img src="https://img.shields.io/github/v/release/marcinx98x/freedrive?style=flat-square" alt="Release"/></a>
  <a href="https://github.com/marcinx98x/freedrive/stargazers"><img src="https://img.shields.io/github/stars/marcinx98x/freedrive?style=flat-square" alt="Stars"/></a>
  <a href="https://github.com/marcinx98x/freedrive/blob/master/LICENSE"><img src="https://img.shields.io/github/license/marcinx98x/freedrive?style=flat-square" alt="License"/></a>
  <a href="https://hub.docker.com/r/marcinx98x/freedrive"><img src="https://img.shields.io/docker/pulls/marcinx98x/freedrive?style=flat-square" alt="Docker pulls"/></a>
</p>

## Table of contents

- [Overview](#overview)
- [Screenshots](#screenshots)
- [Features](#features)
- [Quick start](#quick-start)
- [Configuration](#configuration)
- [Clients](#clients)
- [API](#api)
- [Operations](#operations)
- [Troubleshooting](#troubleshooting)
- [Contributing](#contributing)
- [License](#license)

## Overview

FreeDrive is an open-source, self-hosted Drive-style storage platform:

- One Go process serves the API and embedded web UI (`go:embed`)
- SQLite + local disk blobs (no external database)
- JWT sessions with rotating refresh tokens; optional email 2FA
- Client-side AES-GCM encryption (server stores ciphertext and wrapped keys only)
- Admin panel for users, invites, SMTP, capacity, IP policy, backups

## Screenshots

### User workspace

![User Workspace](docs/screenshots/user.png)

### Admin workspace

![Admin Workspace](docs/screenshots/admin.png)

### Desktop

![FreeDrive Desktop](docs/screenshots/FreeDrive%20Desktop.png)

## Features

**Web**

- My Drive, Computers, Recent, Starred, Shared with me, Trash; list/grid; live + advanced search
- Upload / download / rename / move; soft delete, restore, permanent delete, versioning
- In-browser Docs, Photos, Sheets; PDF / video / audio / JSON viewers; Open with…
- Sharing (user-to-user + links), comments, approvals
- Quotas, activity log, profile, devices (remote logout), recovery code / key rotation

**Admin** (`/api/v1/admin/*`, shield in the top bar)

- Users, invites, sessions, SMTP, IP allow/block, require 2FA, capacity, file-type policy
- Trash purge, duplicates, settings backup, danger wipe

**Architecture (short)**

`chi` router → middleware (CORS, rate limit, auth/`sid`) → handlers → services → SQLite / disk storage. Admin policy from `data/settings.json`.

**Security (short)**

Protected routes need a valid JWT with a live session. IP rules apply on public auth endpoints. Rate limit: 100 req/s, burst 400. Prefer HTTPS (or localhost) for WebCrypto uploads.

## Quick start

### Local

```bash
go mod download
go run ./cmd/freedrive
```

Open `http://localhost:8080`. First-run admin (change immediately): `admin@freedrive.local` / `admin123`.

### Docker

Images: [`marcinx98x/freedrive`](https://hub.docker.com/r/marcinx98x/freedrive) (and GHCR). Tags: `latest`, `master`, `sha-*` (`amd64` / `arm64`).

```bash
docker pull marcinx98x/freedrive:latest
docker run -d --name freedrive -p 8080:8080 \
  -e FREEDRIVE_ADMIN_EMAIL=admin@freedrive.local \
  -e FREEDRIVE_ADMIN_PASSWORD=change-me-now \
  -v freedrive-data:/app/data \
  marcinx98x/freedrive:latest
```

### Docker Compose

Edit `docker-compose.yml` (admin password, port, host path for `/app/data`), then:

```bash
docker compose pull && docker compose up -d
```

Data (`freedrive.db`, `blobs/`, `jwt_secret.key`) lives in the mapped folder — keep the bind mount on updates (compose/Watchtower). Avoid recreating the container in Synology GUI without remapping the same folder.

### systemd

```bash
chmod +x scripts/install.sh && ./scripts/install.sh
```

Installs binary to `/opt/freedrive`, env to `/etc/freedrive/freedrive.env`, and starts `freedrive.service`. Update helpers and ops: `systemctl status|restart freedrive`, `journalctl -u freedrive -f`.

## Configuration

| Variable | Description | Default |
|---|---|---|
| `FREEDRIVE_PORT` | HTTP port | `8080` |
| `FREEDRIVE_DATA_DIR` | DB, blobs, keys | `./data` |
| `FREEDRIVE_JWT_SECRET` | JWT signing secret | auto-generated if empty |
| `FREEDRIVE_MAX_UPLOAD_MB` | Max upload size (MB) | `5120` |
| `FREEDRIVE_ADMIN_EMAIL` | Initial admin email | `admin@freedrive.local` |
| `FREEDRIVE_ADMIN_PASSWORD` | Initial admin password | `admin123` |

## Clients

### Desktop (beta)

[`desktop/`](desktop/) — Tauri sync client (folder sync, tray, Windows Explorer / CfAPI). Details: [`desktop/README.md`](desktop/README.md).

```bash
go run ./cmd/freedrive
cd desktop && npm install && npm run tauri dev   # or scripts\dev.cmd on Windows
```

Build: `scripts\dev.cmd build` → NSIS installer under `desktop/src-tauri/target/release/bundle/nsis/` (preferred for uninstall cleanup).

### Mobile (MVP)

[`mobile/`](mobile/) — Expo Android app (Drive UI, encrypted open/upload, image/video galleries). Details: [`mobile/README.md`](mobile/README.md).

```bash
go run ./cmd/freedrive
cd mobile && npm install && npm start
```

APK (Windows): `powershell -File mobile\scripts\build-apk.ps1` → `mobile\dist\FreeDrive-1.0.0.apk`.

## API

Base path `/api/v1`. Full route list: [`docs/api.md`](docs/api.md). Desktop-oriented notes: [`docs/desktop-api.md`](docs/desktop-api.md). Health: `GET /api/v1/health`.

## Project structure

```text
cmd/freedrive/          # binary + embedded web/
internal/               # api, service, repository, storage, config, …
scripts/install.sh
docs/                   # api.md, desktop-api.md, screenshots, landing
desktop/                # Tauri client
mobile/                 # Expo Android client
```

## Operations

**Back up:** `$FREEDRIVE_DATA_DIR` (SQLite DB, `blobs/`, `jwt_secret.key`, `settings.json`).

**Upgrade (systemd):** stop → replace binary → start → `curl -s http://localhost:8080/api/v1/health`.

**Production baseline:** reverse proxy with HTTPS, strong admin password, regular data-dir backups, non-root user when possible.

## Troubleshooting

| Issue | Check |
|-------|--------|
| UI / service | `systemctl status freedrive`, `journalctl -u freedrive -f`, port/firewall |
| Login | JWT secret stability, clock, IP allow/block, SMTP if 2FA required |
| Uploads | `FREEDRIVE_MAX_UPLOAD_MB`, proxy body limits, admin `total_capacity_gb` |
| Mail / 2FA | Admin → Settings → Email (SMTP); SPF/DKIM on sender domain |

## Contributing

Fork → feature branch → focused PR. Open an issue first for larger design changes.

## License

MIT — see [LICENSE](LICENSE).

[![Star History Chart](https://api.star-history.com/svg?repos=marcinx98x/freedrive&type=Date&legend=top-left)](https://www.star-history.com/?repos=marcinx98x%2Ffreedrive&type=date&legend=top-left)
