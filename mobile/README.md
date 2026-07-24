# FreeDrive Mobile

Android-first Expo / React Native client for [FreeDrive](https://github.com/marcinx98x/freedrive). Part of the monorepo (`mobile/`); server lives in the repo root.

## Features

- Sign in (server URL, email/password, 2FA) with SecureStore session and stable device ID
- Drive UI: Home / Starred / Shared / Files; portrait bottom tabs; landscape NavRail; drawer (Recent, Bin, Settings, Help)
- Files: My Drive | Computers, folders, list/grid, search, paginated lists, Upload / New folder
- Encrypted open/upload (AES-GCM); previews for images, video, text, PDF
- Image/video galleries (swipe in the current list); text edit and image rotate + save
- Share sheet; Android Downloads via MediaStore + status notifications (download / video playback — no persistent “app running” icon)

## Requirements

- Node.js 20+
- Android SDK + JDK 17 (for APK builds)

## Development

```bash
cd mobile
npm install
npm start
```

Sign in with your FreeDrive server URL (e.g. `http://192.168.x.x:8080`).

```bash
npm run typecheck
node scripts/generate-assets.mjs   # icon/splash from desktop logo
```

## Build APK (Windows)

Always build from the short path `C:\fdm` (not long `Desktop\…` paths). Output: `mobile\dist\FreeDrive-1.0.0.apk`.

```powershell
powershell -File mobile\scripts\build-apk.ps1
```

Use `-Clean` only for a first build or when native config changes (`app.json` plugins, new native modules). Routine TS/UI rebuilds do not need `expo prebuild`.
