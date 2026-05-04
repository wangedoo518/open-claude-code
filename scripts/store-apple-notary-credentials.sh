#!/usr/bin/env bash
set -euo pipefail

require_cmd() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "[ERROR] Missing required command: $1" >&2
    exit 1
  fi
}

require_env() {
  local name="$1"
  if [[ -z "${!name:-}" ]]; then
    echo "[ERROR] Missing required environment variable: ${name}" >&2
    exit 1
  fi
}

require_cmd xcrun

require_env APPLE_KEYCHAIN_PATH
require_env APPLE_NOTARY_APPLE_ID
require_env APPLE_NOTARY_APP_PASSWORD
require_env APPLE_NOTARY_TEAM_ID

if [[ ! -f "${APPLE_KEYCHAIN_PATH}" ]]; then
  echo "[ERROR] APPLE_KEYCHAIN_PATH does not exist: ${APPLE_KEYCHAIN_PATH}" >&2
  exit 1
fi

notary_profile="${APPLE_NOTARY_PROFILE:-buddy-notary}"

xcrun notarytool store-credentials "${notary_profile}" \
  --apple-id "${APPLE_NOTARY_APPLE_ID}" \
  --team-id "${APPLE_NOTARY_TEAM_ID}" \
  --password "${APPLE_NOTARY_APP_PASSWORD}" \
  --keychain "${APPLE_KEYCHAIN_PATH}"

if [[ -n "${GITHUB_ENV:-}" ]]; then
  echo "APPLE_NOTARY_PROFILE=${notary_profile}" >> "${GITHUB_ENV}"
fi

echo "[info] stored notary profile ${notary_profile}"
