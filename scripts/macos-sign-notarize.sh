#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
DESKTOP_DIR="${ROOT_DIR}/apps/desktop-shell"
TAURI_DIR="${DESKTOP_DIR}/src-tauri"
cd "${ROOT_DIR}"

if [[ "${OSTYPE:-}" != darwin* ]]; then
  echo "[ERROR] This script only supports macOS."
  exit 1
fi

require_cmd() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "[ERROR] Missing required command: $1"
    exit 1
  fi
}

require_cmd cargo
require_cmd codesign
require_cmd ditto
require_cmd hdiutil
require_cmd node
require_cmd npm
require_cmd rustup
require_cmd security
require_cmd xcrun

staple_with_retry() {
  local path="$1"
  local attempts="${2:-5}"
  local delay_seconds="${3:-5}"
  local attempt=1

  while true; do
    if xcrun stapler staple "${path}"; then
      return 0
    fi

    if [[ "${attempt}" -ge "${attempts}" ]]; then
      echo "[ERROR] Failed to staple notarization ticket after ${attempts} attempts: ${path}"
      return 1
    fi

    echo "[staple] Retry ${attempt}/${attempts} failed for ${path}; sleeping ${delay_seconds}s before retry..."
    sleep "${delay_seconds}"
    attempt=$((attempt + 1))
  done
}

detect_default_target() {
  case "$(uname -m)" in
    arm64) echo "aarch64-apple-darwin" ;;
    x86_64) echo "x86_64-apple-darwin" ;;
    *)
      echo "[ERROR] Unsupported macOS architecture: $(uname -m)" >&2
      exit 1
      ;;
  esac
}

artifact_arch_label() {
  case "$1" in
    aarch64-apple-darwin|arm64-apple-darwin) echo "aarch64" ;;
    x86_64-apple-darwin) echo "x64" ;;
    *)
      echo "[ERROR] Unsupported macOS target: $1" >&2
      exit 1
      ;;
  esac
}

auto_detect_developer_id() {
  security find-identity -v -p codesigning 2>/dev/null | \
    sed -n 's/.*"\(Developer ID Application: [^"]*\)".*/\1/p' | \
    head -n 1
}

BUILD_TARGET="${TAURI_TARGET:-$(detect_default_target)}"
TIMESTAMP_URL="${APPLE_TIMESTAMP_URL:-http://timestamp.apple.com/ts01}"
APPLE_KEYCHAIN_PATH="${APPLE_KEYCHAIN_PATH:-}"
APPLE_NOTARY_PROFILE="${APPLE_NOTARY_PROFILE:-clawhub-notary}"

while [[ "$#" -gt 0 ]]; do
  case "$1" in
    --target)
      if [[ -z "${2:-}" ]]; then
        echo "[ERROR] --target requires a value."
        exit 1
      fi
      BUILD_TARGET="$2"
      shift 2
      ;;
    *)
      echo "[ERROR] Unsupported argument: $1"
      echo "        Supported: --target <triple>"
      exit 1
      ;;
  esac
done

if [[ -z "${APPLE_DEV_ID_APP:-}" ]]; then
  APPLE_DEV_ID_APP="$(auto_detect_developer_id)"
  export APPLE_DEV_ID_APP
fi

if [[ -z "${APPLE_DEV_ID_APP:-}" ]]; then
  echo "[ERROR] APPLE_DEV_ID_APP is required."
  echo "        Example: export APPLE_DEV_ID_APP='Developer ID Application: Your Name (TEAMID)'"
  exit 1
fi

if [[ -z "${APPLE_NOTARY_PROFILE:-}" ]]; then
  echo "[ERROR] APPLE_NOTARY_PROFILE is required."
  echo "        Example: export APPLE_NOTARY_PROFILE='clawhub-notary'"
  exit 1
fi

if ! rustup target list --installed | grep -qx "${BUILD_TARGET}"; then
  echo "[INFO] Installing Rust target: ${BUILD_TARGET}"
  rustup target add "${BUILD_TARGET}"
fi

echo "[backend] Building desktop-server (${BUILD_TARGET})..."
cargo build \
  --manifest-path "${ROOT_DIR}/rust/Cargo.toml" \
  --release \
  --bin desktop-server \
  --target "${BUILD_TARGET}"

echo "[bundle] Staging Tauri sidecar and ingest resources..."
mkdir -p "${TAURI_DIR}/binaries" "${TAURI_DIR}/scripts"
backend_src="${ROOT_DIR}/rust/target/${BUILD_TARGET}/release/desktop-server"
backend_dst="${TAURI_DIR}/binaries/desktop-server-${BUILD_TARGET}"
cp "${backend_src}" "${backend_dst}"

for file in wechat_fetcher.py defuddle_worker.js markitdown_worker.py package.json; do
  cp "${ROOT_DIR}/rust/crates/wiki_ingest/src/${file}" "${TAURI_DIR}/scripts/${file}"
done

echo "[build] Building unsigned Tauri app bundle..."
(
  cd "${DESKTOP_DIR}"
  npm run tauri:build -- --bundles app --no-sign --target "${BUILD_TARGET}"
)

TARGET_RELEASE_DIR="${TAURI_DIR}/target/${BUILD_TARGET}/release"
MACOS_DIR="${TARGET_RELEASE_DIR}/bundle/macos"
DMG_DIR="${TARGET_RELEASE_DIR}/bundle/dmg"
APP_PATH="$(find "${MACOS_DIR}" -maxdepth 1 -type d -name '*.app' | head -n 1)"

if [[ -z "${APP_PATH}" || ! -d "${APP_PATH}" ]]; then
  echo "[ERROR] App bundle not found in ${MACOS_DIR}"
  exit 1
fi

APP_BUNDLE_NAME="$(basename "${APP_PATH}")"
APP_VERSION="$(node -e "const fs=require('fs'); const p='${TAURI_DIR}/tauri.conf.json'; process.stdout.write(JSON.parse(fs.readFileSync(p,'utf8')).version)")"
ARCH_LABEL="$(artifact_arch_label "${BUILD_TARGET}")"

echo "[sign] Signing app bundle with ${APPLE_DEV_ID_APP}..."
APP_CODESIGN_ARGS=(
  --force
  --deep
  --options runtime
  --timestamp="${TIMESTAMP_URL}"
  --sign "${APPLE_DEV_ID_APP}"
)
if [[ -n "${APPLE_KEYCHAIN_PATH}" ]]; then
  APP_CODESIGN_ARGS+=(--keychain "${APPLE_KEYCHAIN_PATH}")
fi
codesign "${APP_CODESIGN_ARGS[@]}" "${APP_PATH}"
codesign --verify --deep --strict --verbose=2 "${APP_PATH}"

echo "[dmg] Building signed release DMG..."
mkdir -p "${DMG_DIR}"
DMG_PATH="${DMG_DIR}/Buddy_${APP_VERSION}_${ARCH_LABEL}.dmg"
STAGE_DIR="$(mktemp -d "${TMPDIR:-/tmp}/buddy-dmg-stage.XXXXXX")"
cleanup() {
  rm -rf "${STAGE_DIR}"
}
trap cleanup EXIT

cp -R "${APP_PATH}" "${STAGE_DIR}/${APP_BUNDLE_NAME}"
ln -s /Applications "${STAGE_DIR}/Applications"
hdiutil create \
  -volname "Buddy" \
  -srcfolder "${STAGE_DIR}" \
  -fs HFS+ \
  -ov \
  -format UDZO \
  "${DMG_PATH}" >/dev/null

if ! hdiutil imageinfo "${DMG_PATH}" | grep -Eq 'Apple_HFS|partition-filesystems:[[:space:]]+HFS\+'; then
  echo "[ERROR] DMG filesystem verification failed: expected HFS+ image for release compatibility."
  exit 1
fi

DMG_CODESIGN_ARGS=(
  --force
  --timestamp="${TIMESTAMP_URL}"
  --sign "${APPLE_DEV_ID_APP}"
)
if [[ -n "${APPLE_KEYCHAIN_PATH}" ]]; then
  DMG_CODESIGN_ARGS+=(--keychain "${APPLE_KEYCHAIN_PATH}")
fi
codesign "${DMG_CODESIGN_ARGS[@]}" "${DMG_PATH}"
codesign --verify --verbose=2 "${DMG_PATH}"

echo "[notary] Submitting DMG to Apple notarization profile ${APPLE_NOTARY_PROFILE}..."
NOTARY_JSON="$(mktemp "${TMPDIR:-/tmp}/buddy-notary.XXXXXX")"
NOTARY_SUBMIT_ARGS=(
  notarytool submit "${DMG_PATH}"
  --keychain-profile "${APPLE_NOTARY_PROFILE}"
  --wait
  --output-format json
)
if [[ -n "${APPLE_KEYCHAIN_PATH}" ]]; then
  NOTARY_SUBMIT_ARGS+=(--keychain "${APPLE_KEYCHAIN_PATH}")
fi
xcrun "${NOTARY_SUBMIT_ARGS[@]}" > "${NOTARY_JSON}"

NOTARY_STATUS="$(grep -Eo '"status"[[:space:]]*:[[:space:]]*"[^"]+"' "${NOTARY_JSON}" | head -n1 | sed -E 's/.*"([^"]+)"/\1/')"
NOTARY_ID="$(grep -Eo '"id"[[:space:]]*:[[:space:]]*"[^"]+"' "${NOTARY_JSON}" | head -n1 | sed -E 's/.*"([^"]+)"/\1/')"

if [[ "${NOTARY_STATUS}" != "Accepted" ]]; then
  echo "[ERROR] Notarization failed. status=${NOTARY_STATUS:-unknown}"
  if [[ -n "${NOTARY_ID:-}" ]]; then
    echo "[INFO] Fetching notarization log for id=${NOTARY_ID}..."
    NOTARY_LOG_ARGS=(notarytool log "${NOTARY_ID}" --keychain-profile "${APPLE_NOTARY_PROFILE}")
    if [[ -n "${APPLE_KEYCHAIN_PATH}" ]]; then
      NOTARY_LOG_ARGS+=(--keychain "${APPLE_KEYCHAIN_PATH}")
    fi
    xcrun "${NOTARY_LOG_ARGS[@]}" || true
  fi
  exit 1
fi

echo "[staple] Stapling notarization tickets..."
staple_with_retry "${APP_PATH}"
staple_with_retry "${DMG_PATH}"

APP_ZIP_PATH="${MACOS_DIR}/Buddy_${APP_VERSION}_${ARCH_LABEL}.app.zip"
rm -f "${APP_ZIP_PATH}"
ditto -c -k --keepParent "${APP_PATH}" "${APP_ZIP_PATH}"

echo "[verify] Local Gatekeeper assessment..."
spctl --assess --type execute --verbose=4 "${APP_PATH}" || true
spctl --assess --type open --context context:primary-signature --verbose=4 "${DMG_PATH}" || true

echo
echo "[DONE] Notarized macOS bundle ready:"
echo "       app: ${APP_PATH}"
echo "       dmg: ${DMG_PATH}"
echo "       app_zip: ${APP_ZIP_PATH}"
