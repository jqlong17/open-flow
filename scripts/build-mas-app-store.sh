#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO_ROOT"

PROFILE_PATH="${1:-${OPEN_FLOW_PROVISIONING_PROFILE:-}}"
if [[ -z "$PROFILE_PATH" ]]; then
  echo "Usage:"
  echo "  ./scripts/build-mas-app-store.sh /absolute/path/to/profile.provisionprofile"
  echo ""
  echo "Or set OPEN_FLOW_PROVISIONING_PROFILE in the environment."
  exit 1
fi

if [[ ! -f "$PROFILE_PATH" ]]; then
  echo "Provisioning profile not found: $PROFILE_PATH"
  exit 1
fi

APP_NAME="${OPEN_FLOW_APP_NAME:-Open Flow MAS}"
BUNDLE_ID="${OPEN_FLOW_BUNDLE_ID:-com.ruska.openflow.mas}"
SIGN_IDENTITY="${OPEN_FLOW_SIGN_IDENTITY:-Apple Distribution: jiaqi long (T626DDUR47)}"
ENTITLEMENTS_PATH="${OPEN_FLOW_SIGN_ENTITLEMENTS:-$REPO_ROOT/packaging/macos/OpenFlowMAS.entitlements}"
BUNDLED_MODEL_SOURCE="${OPEN_FLOW_BUNDLED_MODEL_SOURCE:-}"

if [[ -z "$BUNDLED_MODEL_SOURCE" ]]; then
  for candidate in \
    "$HOME/Library/Application Support/com.openflow.open-flow/models/sensevoice-small" \
    "$HOME/Library/Application Support/com.openflow.open-flow-mas-dev/models/sensevoice-small" \
    "$HOME/Library/Application Support/Shandianshuo/models/sensevoice-small"
  do
    if [[ -f "$candidate/model.onnx" || -f "$candidate/model_quant.onnx" ]]; then
      BUNDLED_MODEL_SOURCE="$candidate"
      break
    fi
  done
fi

if [[ -z "$BUNDLED_MODEL_SOURCE" ]]; then
  echo "No bundled model source was found."
  echo "Set OPEN_FLOW_BUNDLED_MODEL_SOURCE to a directory containing model.onnx/model_quant.onnx, tokens.json, am.mvn, and config.yaml."
  exit 1
fi

echo "Building Mac App Store package with:"
echo "  App name: $APP_NAME"
echo "  Bundle ID: $BUNDLE_ID"
echo "  Sign identity: $SIGN_IDENTITY"
echo "  Provisioning profile: $PROFILE_PATH"
echo "  Entitlements: $ENTITLEMENTS_PATH"
echo "  Bundled model source: $BUNDLED_MODEL_SOURCE"

OPEN_FLOW_APP_NAME="$APP_NAME" \
OPEN_FLOW_BUNDLE_ID="$BUNDLE_ID" \
OPEN_FLOW_INSTALL_TO_APPLICATIONS="${OPEN_FLOW_INSTALL_TO_APPLICATIONS:-0}" \
OPEN_FLOW_CARGO_FEATURES="${OPEN_FLOW_CARGO_FEATURES:-mas}" \
OPEN_FLOW_LSUIELEMENT="${OPEN_FLOW_LSUIELEMENT:-0}" \
OPEN_FLOW_SIGN_IDENTITY="$SIGN_IDENTITY" \
OPEN_FLOW_SIGN_ENTITLEMENTS="$ENTITLEMENTS_PATH" \
OPEN_FLOW_PROVISIONING_PROFILE="$PROFILE_PATH" \
OPEN_FLOW_BUNDLED_MODEL_SOURCE="$BUNDLED_MODEL_SOURCE" \
./scripts/build-app.sh
