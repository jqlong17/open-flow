#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO_ROOT"

PROFILE_PATH="${1:-${OPEN_FLOW_PROVISIONING_PROFILE:-}}"
APP_PATH="${OPEN_FLOW_MAS_APP_PATH:-$REPO_ROOT/dist/Open Flow MAS.app}"
PKG_PATH="${OPEN_FLOW_MAS_PKG_PATH:-$REPO_ROOT/dist/Open-Flow-MAS.pkg}"
INSTALLER_IDENTITY="${OPEN_FLOW_INSTALLER_SIGN_IDENTITY:-3rd Party Mac Developer Installer: jiaqi long (T626DDUR47)}"

if [[ -z "$PROFILE_PATH" ]]; then
  echo "Usage:"
  echo "  ./scripts/build-mas-app-store-pkg.sh /path/to/profile.provisionprofile"
  echo ""
  echo "Or set OPEN_FLOW_PROVISIONING_PROFILE."
  exit 1
fi

if [[ ! -f "$PROFILE_PATH" ]]; then
  echo "Provisioning profile not found: $PROFILE_PATH"
  exit 1
fi

echo "Preparing Mac App Store upload package with:"
echo "  Provisioning profile: $PROFILE_PATH"
echo "  App path            : $APP_PATH"
echo "  Pkg path            : $PKG_PATH"
echo "  Installer identity  : $INSTALLER_IDENTITY"

"$REPO_ROOT/scripts/build-mas-app-store.sh" "$PROFILE_PATH"

if [[ ! -d "$APP_PATH" ]]; then
  echo "Expected app bundle not found: $APP_PATH"
  exit 1
fi

rm -f "$PKG_PATH"

productbuild \
  --component "$APP_PATH" /Applications \
  --sign "$INSTALLER_IDENTITY" \
  "$PKG_PATH"

echo ""
echo "Package signature:"
pkgutil --check-signature "$PKG_PATH"

echo ""
echo "Done:"
echo "  Upload package: $PKG_PATH"
