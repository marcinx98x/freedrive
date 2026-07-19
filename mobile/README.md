# FreeDrive Mobile (MVP)

React Native + Expo client for FreeDrive. Android-first. Sign in to your self-hosted server and browse My Drive, Computers, Starred, and Shared.

## Requirements

- Node.js 20+
- Expo Go on your Android phone (same Wi‑Fi as the server, or use a public HTTPS URL)

## Setup

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

The phone appears on the account **Devices** list (as a web session named `Mobile (…)`) after sign-in.

## MVP features

- Login + 2FA + secure token storage + auto refresh
- Bottom tabs: Home, Starred, Shared, Files
- Files: My Drive | Computers tabs, folder navigation, list/grid, sort
- Search files by name
- Pull-to-refresh

Upload, download, offline files, and E2E preview are planned for later releases.

## Scripts

```bash
npm start          # Expo dev server
npm run android    # open Android
npx tsc --noEmit   # typecheck
```
