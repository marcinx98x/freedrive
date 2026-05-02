#!/usr/bin/env bash
set -euo pipefail

REPO="${FREEDRIVE_REPO:-abdullaabdullazade/freedrive}"
RELEASE_BASE_URL="https://github.com/${REPO}/releases/latest/download"
APP_DIR="/opt/freedrive"
BIN_PATH="$APP_DIR/freedrive"
BACKUP_PATH="$APP_DIR/freedrive.bak"
SERVICE_NAME="freedrive.service"
TMP_DIR="$(mktemp -d)"
TMP_BIN="${TMP_DIR}/freedrive-linux-amd64"
TMP_SUMS="${TMP_DIR}/checksums.txt"

cleanup() {
  rm -rf "$TMP_DIR"
}
trap cleanup EXIT

if [[ "$(uname -s)" != "Linux" ]]; then
  echo "This updater supports Linux only."
  exit 1
fi

if [[ "$(uname -m)" != "x86_64" && "$(uname -m)" != "amd64" ]]; then
  echo "This updater currently supports amd64 only."
  exit 1
fi

if [[ ! -x "$BIN_PATH" ]]; then
  echo "FreeDrive is not installed at $BIN_PATH."
  echo "Run the installer first, then use this updater for future releases."
  exit 1
fi

if ! command -v curl >/dev/null 2>&1; then
  echo "curl is required."
  exit 1
fi
if ! command -v sha256sum >/dev/null 2>&1; then
  echo "sha256sum is required."
  exit 1
fi
if ! command -v systemctl >/dev/null 2>&1; then
  echo "systemctl is required."
  exit 1
fi

echo "Downloading latest FreeDrive release binary..."
curl -fsSL "${RELEASE_BASE_URL}/freedrive-linux-amd64" -o "$TMP_BIN"
curl -fsSL "${RELEASE_BASE_URL}/checksums.txt" -o "$TMP_SUMS"

EXPECTED_SHA="$(awk '/freedrive-linux-amd64$/ {print $1; exit}' "$TMP_SUMS" | sed 's/^sha256://')"
if [[ -z "${EXPECTED_SHA}" ]]; then
  echo "Could not find checksum for freedrive-linux-amd64 in checksums.txt"
  exit 1
fi

ACTUAL_SHA="$(sha256sum "$TMP_BIN" | awk '{print $1}')"
if [[ "${EXPECTED_SHA}" != "${ACTUAL_SHA}" ]]; then
  echo "Checksum verification failed."
  echo "Expected: ${EXPECTED_SHA}"
  echo "Actual:   ${ACTUAL_SHA}"
  exit 1
fi
echo "Checksum verified."

echo "Updating FreeDrive binary..."
sudo install -m 0755 "$BIN_PATH" "$BACKUP_PATH"
sudo install -m 0755 "$TMP_BIN" "$BIN_PATH"

echo "Restarting ${SERVICE_NAME}..."
sudo systemctl restart "$SERVICE_NAME"
sudo systemctl --no-pager --full status "$SERVICE_NAME" || true

echo
echo "FreeDrive has been updated to the latest release."
echo "Backup binary: $BACKUP_PATH"
echo "Logs: sudo journalctl -u freedrive -f"
