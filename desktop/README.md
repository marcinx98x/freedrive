# FreeDrive Desktop

Desktop sync client for [FreeDrive](https://github.com/marcinx98x/freedrive) — self-hosted cloud storage with a Drive-like web UI.

Part of the **FreeDrive monorepo** (`desktop/`). The server lives in the repo root (`cmd/freedrive`, `internal/`).

## Features

- **Sign in** to your FreeDrive server (JWT auth + 2FA support)
- **Single-instance** — a second launch focuses the existing main window
- **Onboarding wizard** — choose folders to sync (Desktop, Documents, Downloads, or custom)
- **Background sync** — uploads local changes, polls for remote changes; skips `.git`, `node_modules`, and `.svn` folders during scan
- **Local deletes → server trash** — removing a file from a sync folder (including Explorer Delete, which moves it out of the tree) soft-deletes the matching server file; periodic verify (~5 min) and pre-upload same-name cleanup catch missed events and avoid live duplicates
- **Silent background verify** — on restart, verifies files in the background without a full UI rescan; if initial sync was never completed, startup resumes full sync with a “Resuming sync…” status
- **Home & Sync activity** — status dashboard inspired by Google Drive for desktop
- **Google Drive-style sidebar** — SVG icons for Home, Sync activity, and Notifications with alert badge; top bar uses matching SVG icons (pause/play, settings, help, lock)
- **Preferences window** — dedicated window opened from the gear icon: **My computer** (manage sync folders), **FreeDrive** (Windows Explorer / CfAPI status), **Settings** (encryption, launch on login, open sync log)
- **Notifications** — alerts for sync errors, paused sync, and storage warnings
- **Profile menu** — server avatar, storage bar, Manage storage, Sign out
- **Sign out** — stops CfAPI and clears contents of `%USERPROFILE%\FreeDrive\My Drive` (folder kept for next login)
- **Device identity** — reports the computer hostname and keeps a stable installation ID, so signing in again updates the same entry in the server's Devices list instead of creating a duplicate
- **Non-blocking sign-in** — crypto unlock, sync restore, and Explorer (CfAPI) start in the background so login does not block the UI
- **System tray** — minimize to tray, pause/resume sync from the menu
- **Windows Explorer integration** (Windows 10 1809+) — open `%USERPROFILE%\FreeDrive` in File Explorer (address bar or **Open Drive folder** in the app) while the desktop client is running; **My Drive** shows server folders/files as cloud placeholders (download on open)

## Prerequisites

- [Node.js](https://nodejs.org/) 18+
- [Rust](https://www.rust-lang.org/tools/install) (for Tauri)
- [Visual Studio Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/) with the **Desktop development with C++** workload (Windows)
- **Windows 10 version 1809 or later** for Explorer Cloud Files (CfAPI) integration
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

> Prefer the **NSIS** installer for uninstall cleanup: it unregisters the CfAPI sync root and removes `%USERPROFILE%\FreeDrive\My Drive`. The MSI target does not run that cleanup yet.

> Use **`npm run build:exe:clean`** after changing the logo — it regenerates icons and runs `cargo clean` so Windows embeds the new `.ico` in the exe. Do not run an old copy from `freedrive-app/`.

## Releases

Desktop releases use tags **`desktop-v*`** (e.g. `desktop-v0.1.0`). Server releases use **`v*`** tags. See [`.github/workflows/release-desktop.yml`](../.github/workflows/release-desktop.yml).

## Troubleshooting

### Sync appears stuck or files do not upload

- Ensure the FreeDrive **server** is running and reachable at the URL you entered during sign-in.
- Check **Sync activity** in the app for per-file errors.
- Check `%APPDATA%\FreeDrive\sync.log` for detailed sync steps.
- Do not run the app as Administrator (different `%APPDATA%` profile).

### Deleted locally but still on the server

- Soft-delete on the server can take a few seconds (journal). Look for `Removed from cloud` in Sync activity or `file_delete` lines in `sync.log`.
- Explorer Delete moves the file out of the sync tree; the client treats that as a local delete. If an event was missed, the next periodic verify (~5 min) or app restart should soft-delete orphans.
- Re-uploading the same name after a missed delete used to leave two live files; the client now trashes same-name siblings in that remote folder before a fresh upload.

### Encryption notes

- Desktop uploads use **AES-GCM-256** (same contract as the web UI).
- Encryption keys are stored locally in `%APPDATA%\FreeDrive\sync.db`.
- Files uploaded from the **web browser** may not decrypt on desktop unless the key was stored on this PC.

### Windows Explorer (FreeDrive sync root)

- Sign in and keep the desktop app running (system tray).
- Open File Explorer and go to `%USERPROFILE%\FreeDrive` (or use **Open Drive folder** in the app).
- Open **My Drive** inside that folder to browse cloud content. Files download when you open them.
- Requires **Windows 10 1809+**. CfAPI connects synchronously on startup / login (`connect-first` recovery if Windows already has the sync root registered).
- Integration state lives in `%APPDATA%\FreeDrive\sync.db` (`cf_sync_root_registered`, `cf_finalize_complete`). Updating or reinstalling the app does not reset a working registration.
- If you previously used a build that registered Explorer sidebar entries, run **Unregister Explorer integration** in app settings (or `unregister_explorer_integration`) once to clear stale shell registry from older builds.

#### CfAPI recovery (`0x80070057` / “cloud file provider is not running”)

If Explorer shows *cloud file provider is not running* or the terminal logs `CfRegisterSyncRoot failed: 0x80070057`, local DB state may be out of sync with Windows. The app now auto-recovers on startup (connect-first). For manual recovery:

```powershell
# Option A — restore DB flag (when Windows still has the sync root registered)
@'
import sqlite3
c = sqlite3.connect(r"%APPDATA%\FreeDrive\sync.db")
c.execute("INSERT OR REPLACE INTO app_config (key, value) VALUES ('cf_sync_root_registered', 'true')")
c.commit()
print("ok")
'@ | python -
```

Restart the app (`npm run tauri dev` or the installed build). Check `%APPDATA%\FreeDrive\sync.log` for `cfapi: explorer integration started`.

```text
# Option B — full reset via Tauri devtools console (app must be running)
await window.__TAURI__.core.invoke('unregister_explorer_integration')
```

Then restart the app for a clean re-registration.

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
| `%USERPROFILE%/FreeDrive/` | CfAPI sync root (Windows Explorer provider) |
| `%USERPROFILE%/FreeDrive/My Drive/` | My Drive view — server folders/files as placeholders |

## License

MIT — same as the FreeDrive server project.
