#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO_ROOT"

PKG_PATH="${1:-${OPEN_FLOW_MAS_PKG_PATH:-$REPO_ROOT/dist/Open-Flow-MAS.pkg}}"
APPLE_ID="${OPEN_FLOW_APPLE_ID:-}"
BUNDLE_ID="${OPEN_FLOW_BUNDLE_ID:-com.ruska.openflow.mas}"
SHORT_VERSION="${OPEN_FLOW_APP_SHORT_VERSION:-}"
BUNDLE_VERSION="${OPEN_FLOW_APP_BUNDLE_VERSION:-}"
APPLE_NUMERIC_ID="${OPEN_FLOW_ASC_APPLE_ID:-}"
PROVIDER_PUBLIC_ID="${OPEN_FLOW_ASC_PROVIDER_PUBLIC_ID:-}"
TEAM_ID="${OPEN_FLOW_TEAM_ID:-}"

if [[ ! -f "$PKG_PATH" ]]; then
  echo "Upload package not found: $PKG_PATH"
  exit 1
fi

if [[ -z "$SHORT_VERSION" || -z "$BUNDLE_VERSION" ]]; then
  if [[ -d "$REPO_ROOT/dist/Open Flow MAS.app" ]]; then
    PLIST="$REPO_ROOT/dist/Open Flow MAS.app/Contents/Info.plist"
    if [[ -f "$PLIST" ]]; then
      SHORT_VERSION="${SHORT_VERSION:-$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$PLIST" 2>/dev/null || true)}"
      BUNDLE_VERSION="${BUNDLE_VERSION:-$(/usr/libexec/PlistBuddy -c 'Print :CFBundleVersion' "$PLIST" 2>/dev/null || true)}"
    fi
  fi
fi

if [[ -z "$APPLE_NUMERIC_ID" ]]; then
  echo "Missing OPEN_FLOW_ASC_APPLE_ID (the numeric Apple ID from App Store Connect)."
  exit 1
fi

if [[ -z "$SHORT_VERSION" || -z "$BUNDLE_VERSION" ]]; then
  echo "Could not determine app version information."
  echo "Set OPEN_FLOW_APP_SHORT_VERSION and OPEN_FLOW_APP_BUNDLE_VERSION."
  exit 1
fi

AUTH_ARGS=()
if [[ -n "${OPEN_FLOW_ASC_API_KEY_ID:-}" && -n "${OPEN_FLOW_ASC_API_ISSUER_ID:-}" ]]; then
  AUTH_ARGS=(
    --api-key "$OPEN_FLOW_ASC_API_KEY_ID"
    --api-issuer "$OPEN_FLOW_ASC_API_ISSUER_ID"
  )
  if [[ -n "${OPEN_FLOW_ASC_API_KEY_PATH:-}" ]]; then
    AUTH_ARGS+=(--p8-file-path "$OPEN_FLOW_ASC_API_KEY_PATH")
  fi
elif [[ -n "${OPEN_FLOW_ASC_USERNAME:-}" && -n "${OPEN_FLOW_ASC_PASSWORD_REF:-}" ]]; then
  AUTH_ARGS=(
    --username "$OPEN_FLOW_ASC_USERNAME"
    --password "$OPEN_FLOW_ASC_PASSWORD_REF"
  )
  if [[ -n "$PROVIDER_PUBLIC_ID" ]]; then
    AUTH_ARGS+=(--provider-public-id "$PROVIDER_PUBLIC_ID")
  elif [[ -n "$TEAM_ID" ]]; then
    AUTH_ARGS+=(--team-id "$TEAM_ID")
  fi
else
  echo "Missing upload credentials."
  echo "Use either:"
  echo "  OPEN_FLOW_ASC_API_KEY_ID + OPEN_FLOW_ASC_API_ISSUER_ID [+ OPEN_FLOW_ASC_API_KEY_PATH]"
  echo "or:"
  echo "  OPEN_FLOW_ASC_USERNAME + OPEN_FLOW_ASC_PASSWORD_REF"
  echo ""
  echo "OPEN_FLOW_ASC_PASSWORD_REF can be '@keychain:<item>' or '@env:<var>' for altool."
  exit 1
fi

echo "Uploading Mac App Store package:"
echo "  Package      : $PKG_PATH"
echo "  Apple ID     : $APPLE_NUMERIC_ID"
echo "  Bundle ID    : $BUNDLE_ID"
echo "  Version      : $SHORT_VERSION ($BUNDLE_VERSION)"

xcrun altool \
  --upload-package "$PKG_PATH" \
  --platform macos \
  --apple-id "$APPLE_NUMERIC_ID" \
  --bundle-id "$BUNDLE_ID" \
  --bundle-short-version-string "$SHORT_VERSION" \
  --bundle-version "$BUNDLE_VERSION" \
  --wait \
  "${AUTH_ARGS[@]}"
