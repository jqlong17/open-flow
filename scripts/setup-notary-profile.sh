#!/usr/bin/env bash
# 配置 macOS notarization 所需的 notarytool profile。
# 支持两种方式：
# 1) Apple ID + app-specific password
# 2) App Store Connect API key
#
# 用法示例：
#   ./scripts/setup-notary-profile.sh
#   OPEN_FLOW_NOTARY_PROFILE=openflow-notary ./scripts/setup-notary-profile.sh
#   OPEN_FLOW_NOTARY_PROFILE=openflow-notary OPEN_FLOW_NOTARY_MODE=apple-id ./scripts/setup-notary-profile.sh
#   OPEN_FLOW_NOTARY_PROFILE=openflow-notary OPEN_FLOW_NOTARY_MODE=api-key ./scripts/setup-notary-profile.sh
set -euo pipefail

PROFILE_NAME="${OPEN_FLOW_NOTARY_PROFILE:-openflow-notary}"
MODE="${OPEN_FLOW_NOTARY_MODE:-}"

echo "Setting up notarytool profile: $PROFILE_NAME"
echo ""
echo "Choose credential mode:"
echo "  1) Apple ID + app-specific password"
echo "  2) App Store Connect API key"
echo ""

if [[ -z "$MODE" ]]; then
  read -r -p "Enter mode (1 or 2): " MODE_CHOICE
  case "$MODE_CHOICE" in
    1) MODE="apple-id" ;;
    2) MODE="api-key" ;;
    *)
      echo "Invalid selection: $MODE_CHOICE"
      exit 1
      ;;
  esac
fi

if [[ "$MODE" == "apple-id" ]]; then
  read -r -p "Apple ID email: " APPLE_ID
  read -r -p "Developer Team ID: " TEAM_ID
  echo ""
  echo "An app-specific password is required."
  echo "You can generate it at: https://appleid.apple.com/account/manage"
  echo ""
  xcrun notarytool store-credentials "$PROFILE_NAME" \
    --apple-id "$APPLE_ID" \
    --team-id "$TEAM_ID"
elif [[ "$MODE" == "api-key" ]]; then
  read -r -p "Path to AuthKey_XXXXXX.p8: " KEY_PATH
  read -r -p "Key ID: " KEY_ID
  read -r -p "Issuer ID (leave empty for individual key): " ISSUER_ID

  STORE_ARGS=(
    "$PROFILE_NAME"
    --key "$KEY_PATH"
    --key-id "$KEY_ID"
  )

  if [[ -n "$ISSUER_ID" ]]; then
    STORE_ARGS+=(--issuer "$ISSUER_ID")
  fi

  xcrun notarytool store-credentials "${STORE_ARGS[@]}"
else
  echo "Unsupported OPEN_FLOW_NOTARY_MODE: $MODE"
  echo "Expected: apple-id or api-key"
  exit 1
fi

echo ""
echo "Profile created. You can now publish with:"
echo "  OPEN_FLOW_NOTARY_PROFILE=\"$PROFILE_NAME\" ./scripts/release.sh"
