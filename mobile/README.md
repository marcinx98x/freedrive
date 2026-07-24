# FreeDrive Mobile (MVP)

React Native + Expo client for [FreeDrive](https://github.com/marcinx98x/freedrive). Android-first. Sign in to your self-hosted server and browse My Drive, Computers, Starred, Shared, Recent, and Bin.

Part of the **FreeDrive monorepo** (`mobile/`). The server lives in the repo root (`cmd/freedrive`, `internal/`).

## Features

- **Sign in** — server URL, email, password, and email 2FA when enabled
- **Secure session** — tokens in SecureStore, profile cache in AsyncStorage (supports large avatar data-URLs), auto-refresh on 401
- **Bottom tabs** — Home, Starred, Shared, Files (with active pill indicator) on phone portrait (and tablet portrait)
- **Landscape NavRail** — when width > height: narrow left rail (menu ≡, Create `+`, then Home / Starred / Shared / Files vertically centered like Google Drive); bottom tabs hidden; Create FAB hidden (rail `+` → Upload / Folder); hamburger on the rail, not in the search bar
- **Files stack** — Files tab hosts My Drive home + nested Folder screens (Drive-like); Shared can deep-link into a folder under Files
- **Files** — My Drive | Computers, folder navigation, list/grid, sort chip; folder listings use server pagination and load more on scroll (same contract as web/desktop)
- **Grid tiles** — square previews (`aspectRatio: 1`); column count scales with content width so landscape tiles stay phone-sized
- **Create FAB** — Upload, New folder, Document (`.txt`), and Spreadsheet (`.csv`) on Files / Folder (camera stub reserved); FAB respects safe-area insets
- **Upload** — multi-file picker; client-side AES-GCM encrypt, write ciphertext to cache, then native `FileSystem.uploadAsync` multipart (`POST /api/v1/files/upload`) — avoids Hermes Blob / FormDataPart limits
- **New folder** — dialog → `POST /api/v1/folders` in the current folder (or My Drive root)
- **New Document / Spreadsheet** — encrypted empty `.txt` / starter `.csv`; opens in the text preview editor (full Sheets UI is web-only)
- **Drawer** — hamburger slides in Recent, Bin, Settings, Help, and storage usage
- **Search** — search files by name from the top bar
- **Branding** — same FreeDrive logo as desktop (`scripts/generate-assets.mjs`); SVG icons aligned with desktop `NavIcons`
- **User avatar** — photo from `avatar_url` on `GET /api/v1/me`, or initials fallback
- **Devices** — appears as `Mobile (…)` and keeps a stable installation ID, so re-login updates the same entry in the account Devices list instead of creating a duplicate
- **File actions** — item menu for opening, sharing a copy, downloading, starring, and moving files to Bin
- **Client-side decryption** — account and per-file keys sync from the server so encrypted files can be opened on Android
- **In-app preview** — images, video (native-controls player via `expo-video`), plain text (Markdown/JSON/CSV), and PDF (open with another app)
- **Image gallery** — swipe left/right between photos in the same loaded list (Folder / Files / Starred / Home); counter shows position; neighbors decrypt in the background
- **Video gallery** — same swipe between videos in the loaded list; only the active page mounts the player; bottom controls respect Android safe-area insets
- **Text edit** — Edit / Save on text previews; content is re-encrypted and uploaded via the same native multipart helper (`POST /api/v1/files/{id}/content`)
- **Image rotate** — Rotate (90°) in the preview header, then Save to replace the file on the server (same content endpoint)
- **Share a copy** — opens the Android share sheet with the decrypted file
- **Download** — native MediaStore save via `FreeDriveDownloads` (config plugin in `plugins/with-freedrive-downloads/`); ongoing “Downloading…” status bar notification, then tappable “Download complete”; Android 13+ asks for notification permission
- **Status notifications** — no persistent “app is running” icon (not appropriate for a Drive client); status-bar presence only for downloads and while a video is playing (media session / native controls)

Offline files are planned for later releases. Docs/Sheets-style office editors and PDF annotation are out of scope for the mobile MVP.

## Requirements

- Node.js 20+
- Expo Go on Android (same Wi-Fi as the server, or a public HTTPS URL)
- For APK builds: Android SDK + JDK 17

## Setup (Expo Go)

```bash
cd mobile
npm install
npm start
```

Scan the QR code with Expo Go.

## Sign-in

1. Enter your FreeDrive server URL (e.g. `https://drive.example.com` or `http://192.168.x.x:8080`)
2. Email + password
3. Complete email 2FA if enabled

## Scripts

```bash
npm start          # Expo dev server
npm run android    # open Android
npm run typecheck  # tsc --noEmit
node scripts/generate-assets.mjs   # regenerate icon/splash from desktop logo
```

## Build release APK

**Canonical procedure (Windows):** always build from `C:\fdm`, copy to `mobile\dist\FreeDrive-1.0.0.apk`. Do not build from `Desktop\Projekty\...` (CMake MAX_PATH).

### Routine rebuild (TS/UI changes, ~1–5 min)

```powershell
powershell -File mobile\scripts\build-apk.ps1
```

Manual steps (same as the script):

```powershell
robocopy "C:\Users\marci\Desktop\Projekty\freedrive-master\mobile" "C:\fdm" /E /XD node_modules android .expo dist
$env:CI = "1"
$env:ANDROID_HOME = "$env:LOCALAPPDATA\Android\Sdk"
$env:JAVA_HOME = "C:\Program Files\Microsoft\jdk-17.0.19.10-hotspot"
cd C:\fdm\android
.\gradlew.bat assembleRelease
Copy-Item "C:\fdm\android\app\build\outputs\apk\release\app-release.apk" "C:\Users\marci\Desktop\Projekty\freedrive-master\mobile\dist\FreeDrive-1.0.0.apk" -Force
```

Do **not** run `expo prebuild` on every rebuild — only when `C:\fdm\android` is missing or **native** config changed (`app.json` plugins, new Expo native module such as `expo-image-manipulator`, or `plugins/with-freedrive-downloads/`).

### First build or native changes only

```powershell
powershell -File mobile\scripts\build-apk.ps1 -Clean
```

Or after syncing to `C:\fdm`: `npx expo prebuild --platform android`, then `gradlew assembleRelease`.

APK output for install: `mobile\dist\FreeDrive-1.0.0.apk` (debug-signed unless you configure a release keystore).
