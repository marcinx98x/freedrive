#!/usr/bin/env bash
set -euo pipefail

if [[ "${EUID}" -eq 0 ]]; then
  echo "Please run this as a normal user with sudo access (not root)."
  exit 1
fi

REPO_URL="${FREEDRIVE_REPO_URL:-https://github.com/abdullaxows/freedrive.git}"
TMP_DIR="/tmp/freedrive-src-$$"

if ! command -v git >/dev/null 2>&1; then
  echo "git is required."
  exit 1
fi
if ! command -v go >/dev/null 2>&1; then
  echo "go is required. Install Go first, then rerun."
  exit 1
fi

read -rp "Admin email: " ADMIN_EMAIL
if [[ -z "${ADMIN_EMAIL}" ]]; then
  echo "Admin email is required."
  exit 1
fi
read -rsp "Admin password: " ADMIN_PASSWORD
echo
if [[ -z "${ADMIN_PASSWORD}" ]]; then
  echo "Admin password is required."
  exit 1
fi

echo "Cloning FreeDrive source..."
git clone --depth 1 "$REPO_URL" "$TMP_DIR"

chmod +x "$TMP_DIR/scripts/install.sh"
FREEDRIVE_ADMIN_EMAIL="$ADMIN_EMAIL" FREEDRIVE_ADMIN_PASSWORD="$ADMIN_PASSWORD" bash "$TMP_DIR/scripts/install.sh" <<INPUT
$ADMIN_EMAIL
$ADMIN_PASSWORD
INPUT

rm -rf "$TMP_DIR"
