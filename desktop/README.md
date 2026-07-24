# FreeDrive Desktop

Windows sync client (Tauri 2 + React + Rust) for [FreeDrive](https://github.com/marcinx98x/freedrive). Lives in `desktop/`; server is at the repo root.

## Features

- Sign in (JWT + 2FA), single-instance, tray, pause/resume
- Sync local folders to **Computers** (not My Drive root); local deletes → server trash
- Preferences (sync folders, Explorer/CfAPI, encryption, launch on login); Help → GitHub
- Fixed window sizes; Drive-like UI and scrollbar
- Windows Explorer **My Drive** via CfAPI (stream/mirror); app data in `%APPDATA%\FreeDrive`
- NSIS uninstall cleans My Drive + `%APPDATA%\FreeDrive` (prefer NSIS over MSI)

## Prerequisites

- Node.js 18+, Rust, MSVC Build Tools (C++), WebView2
- Windows 10 1809+ for Explorer integration
- Running FreeDrive server (`go run ./cmd/freedrive` from repo root)

```powershell
winget install Microsoft.VisualStudio.2022.BuildTools --accept-package-agreements --accept-source-agreements --override "--wait --passive --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"
winget install Rustlang.Rustup
winget install Microsoft.EdgeWebView2Runtime
```

Then `rustup default stable` and restart the terminal.

## Development

```bash
go run ./cmd/freedrive          # repo root — server
cd desktop && npm install
```

Windows (recommended — sets MSVC env):

```cmd
scripts\dev.cmd
```

Or `npm run tauri dev` if `link.exe` is already on PATH. Sign in at `http://localhost:8080`.

## Build

```cmd
scripts\dev.cmd build
```

Or `npm run build:exe` / `npm run build:exe:clean` (clean after logo/icon changes).

Outputs:

- `desktop/src-tauri/target/release/freedrive-desktop.exe`
- `bundle/nsis/` (recommended) and `bundle/msi/`

Tags: `desktop-v*` (see [release-desktop.yml](../.github/workflows/release-desktop.yml)).

## Data & troubleshooting

| Path | Purpose |
|------|---------|
| `%APPDATA%\FreeDrive\sync.db` | Sync state / keys |
| `%APPDATA%\FreeDrive\auth.json` | Session |
| `%APPDATA%\FreeDrive\sync.log` | Debug log |
| `%USERPROFILE%\FreeDrive\My Drive\` | Explorer placeholders |

- Sync issues → Sync activity + `sync.log`; do not run as Administrator
- Explorer: keep the app in the tray; open `%USERPROFILE%\FreeDrive\My Drive`
- CfAPI errors → Preferences → unregister Explorer integration, then restart
- API used by the client: [`docs/desktop-api.md`](../docs/desktop-api.md)

## License

MIT — same as the FreeDrive server.
