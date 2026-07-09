# FreeDrive Desktop

Desktop sync client for [FreeDrive](https://github.com/marcinx98x/freedrive) — self-hosted cloud storage with a Drive-like web UI.

Part of the **FreeDrive monorepo** (`desktop/`). The server lives in the repo root (`cmd/freedrive`, `internal/`).

## Features

- **Sign in** to your FreeDrive server (JWT auth + 2FA support)
- **Onboarding wizard** — choose folders to sync (Desktop, Documents, Downloads, or custom)
- **Background sync** — uploads local changes, polls for remote changes
- **Silent background verify** — on restart, verifies files in the background without a full UI rescan
- **Home & Sync activity** — status dashboard inspired by Google Drive for desktop
- **Google Drive-style sidebar** — SVG icons for Home, Sync activity, and Notifications with alert badge
- **Notifications** — alerts for sync errors, paused sync, and storage warnings
- **Profile menu** — server avatar, storage bar, Manage storage, Sign out / Sign in with another account
- **System tray** — minimize to tray, pause/resume sync from the menu

## Prerequisites

- [Node.js](https://nodejs.org/) 18+
- [Rust](https://www.rust-lang.org/tools/install) (for Tauri)
- [Visual Studio Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/) with the **Desktop development with C++** workload (Windows)
- A running **FreeDrive server** (from repo root: `go run ./cmd/freedrive`)

Install Visual Studio Build Tools (C++ compiler + linker):

```powershell
winget install Microsoft.VisualStudio.2022.BuildTools --accept-package-agreements --accept-source-agreements --override "--wait --passive --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"
```

Install Rust on Windows:

```powershell
winget install Rustlang.Rustup
```

Then restart your terminal and run `rustup default stable`.

## Development

1. Start the FreeDrive server (from **repo root**):

```bash
go run ./cmd/freedrive
```

2. Install dependencies and run the desktop app (from **`desktop/`**):

```bash
cd desktop
npm install
```

**Windows (recommended)** — uses MSVC environment automatically:

```cmd
scripts\dev.cmd
```

Or manually:

```bash
npm run tauri dev
```

> On Windows, `npm run tauri dev` requires the MSVC linker in PATH. If you see `link.exe not found`, use `scripts\dev.cmd` instead or open **Developer PowerShell for VS 2022**.

3. Sign in with `http://localhost:8080` and your account credentials.

## Build installer (Windows)

```cmd
scripts\dev.cmd build
```

Or from `desktop/`:

```bash
npm run build:exe:clean
```

Outputs (monorepo path):

- `desktop/src-tauri/target/release/freedrive-desktop.exe`
- `desktop/src-tauri/target/release/bundle/msi/` and `bundle/nsis/` (installers)

> Use **`npm run build:exe:clean`** after changing the logo — it regenerates icons and runs `cargo clean` so Windows embeds the new `.ico` in the exe. Do not run an old copy from `freedrive-app/`.

## Releases

Desktop releases use tags **`desktop-v*`** (e.g. `desktop-v0.1.0`). Server releases use **`v*`** tags. See [`.github/workflows/release-desktop.yml`](../.github/workflows/release-desktop.yml).

## Troubleshooting

### Sync appears stuck or files do not upload

- Ensure the FreeDrive **server** is running and reachable at the URL you entered during sign-in.
- Check **Sync activity** in the app for per-file errors.
- Check `%APPDATA%\FreeDrive\sync.log` for detailed sync steps.
- Do not run the app as Administrator (different `%APPDATA%` profile).

### Encryption notes

- Desktop uploads use **AES-GCM-256** (same contract as the web UI).
- Encryption keys are stored locally in `%APPDATA%\FreeDrive\sync.db`.
- Files uploaded from the **web browser** may not decrypt on desktop unless the key was stored on this PC.

### `link.exe` not found

Install MSVC Build Tools (see Prerequisites), restart the terminal, or use `scripts\dev.cmd`.

### WebView2

```powershell
winget install Microsoft.EdgeWebView2Runtime
```

## Architecture

| Layer | Stack |
|-------|-------|
| UI | React + TypeScript + Vite |
| Native | Tauri 2 (Rust) |
| Local state | SQLite (`%APPDATA%/FreeDrive/sync.db`) |
| Credentials | `%APPDATA%/FreeDrive/auth.json` |
| Sync | `notify` file watcher + REST API |

## API integration

See [`docs/desktop-api.md`](../docs/desktop-api.md) for the endpoint list used by this client.

## Data locations

| Path | Purpose |
|------|---------|
| `%APPDATA%/FreeDrive/sync.db` | Sync state database |
| `%APPDATA%/FreeDrive/auth.json` | Session tokens |
| `%APPDATA%/FreeDrive/sync.log` | Sync debug log |
| `%USERPROFILE%/FreeDrive/` | Downloaded cloud files mirror |

## License

MIT — same as the FreeDrive server project.
