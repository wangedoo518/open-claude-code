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

decode_base64() {
  if base64 --help 2>&1 | grep -q -- "--decode"; then
    base64 --decode
    return
  fi

  if base64 -D </dev/null >/dev/null 2>&1; then
    base64 -D
    return
  fi

  base64 -d
}

require_cmd base64
require_cmd security
require_cmd xcrun

store_notary_credentials=1
while [[ "$#" -gt 0 ]]; do
  case "$1" in
    --skip-notary-credentials)
      store_notary_credentials=0
      shift
      ;;
    *)
      echo "[ERROR] Unsupported argument: $1" >&2
      echo "        Supported: --skip-notary-credentials" >&2
      exit 1
      ;;
  esac
done

require_env APPLE_CERTIFICATE_P12_BASE64
require_env APPLE_CERTIFICATE_PASSWORD
require_env APPLE_DEV_ID_APP
require_env APPLE_KEYCHAIN_PASSWORD
if [[ "${store_notary_credentials}" -eq 1 ]]; then
  require_env APPLE_NOTARY_APPLE_ID
  require_env APPLE_NOTARY_APP_PASSWORD
  require_env APPLE_NOTARY_TEAM_ID
fi

run_dir="${RUNNER_TEMP:-$(mktemp -d)}"
cert_path="${run_dir}/apple-dev-id.p12"
keychain_path="${run_dir}/buddy-release.keychain-db"
notary_profile="${APPLE_NOTARY_PROFILE:-buddy-notary}"

printf '%s' "${APPLE_CERTIFICATE_P12_BASE64}" | decode_base64 > "${cert_path}"

security create-keychain -p "${APPLE_KEYCHAIN_PASSWORD}" "${keychain_path}"
security set-keychain-settings -lut 21600 "${keychain_path}"
security unlock-keychain -p "${APPLE_KEYCHAIN_PASSWORD}" "${keychain_path}"

existing_keychains=()
while IFS= read -r keychain; do
  keychain="${keychain//\"/}"
  [[ -n "${keychain}" ]] && existing_keychains+=("${keychain}")
done < <(security list-keychains -d user)

security list-keychains -d user -s "${keychain_path}" "${existing_keychains[@]}"
security default-keychain -d user -s "${keychain_path}"

security import "${cert_path}" \
  -k "${keychain_path}" \
  -P "${APPLE_CERTIFICATE_PASSWORD}" \
  -T /usr/bin/codesign \
  -T /usr/bin/security \
  -T /usr/bin/productsign

security set-key-partition-list \
  -S apple-tool:,apple: \
  -s \
  -k "${APPLE_KEYCHAIN_PASSWORD}" \
  "${keychain_path}"

if ! security find-identity -v -p codesigning "${keychain_path}" | grep -F "${APPLE_DEV_ID_APP}" >/dev/null 2>&1; then
  echo "[ERROR] Developer ID identity not found in imported keychain: ${APPLE_DEV_ID_APP}" >&2
  exit 1
fi

if [[ -n "${GITHUB_ENV:-}" ]]; then
  {
    echo "APPLE_KEYCHAIN_PATH=${keychain_path}"
  } >> "${GITHUB_ENV}"
fi

if [[ "${store_notary_credentials}" -eq 1 ]]; then
  xcrun notarytool store-credentials "${notary_profile}" \
    --apple-id "${APPLE_NOTARY_APPLE_ID}" \
    --team-id "${APPLE_NOTARY_TEAM_ID}" \
    --password "${APPLE_NOTARY_APP_PASSWORD}" \
    --keychain "${keychain_path}"

  if [[ -n "${GITHUB_ENV:-}" ]]; then
    echo "APPLE_NOTARY_PROFILE=${notary_profile}" >> "${GITHUB_ENV}"
  fi
fi

echo "[info] imported signing identity into ${keychain_path}"
if [[ "${store_notary_credentials}" -eq 1 ]]; then
  echo "[info] stored notary profile ${notary_profile}"
fi
