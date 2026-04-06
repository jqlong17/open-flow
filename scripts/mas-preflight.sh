#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO_ROOT"

BUNDLE_ID="${OPEN_FLOW_BUNDLE_ID:-com.openflow.open-flow}"
APP_NAME="${OPEN_FLOW_APP_NAME:-Open Flow}"
ENTITLEMENTS_PATH="${OPEN_FLOW_MAS_ENTITLEMENTS:-$REPO_ROOT/packaging/macos/OpenFlowMAS.entitlements}"
PROFILE_DIR="${OPEN_FLOW_PROFILE_DIR:-$HOME/Library/MobileDevice/Provisioning Profiles}"

PASS_COUNT=0
WARN_COUNT=0
FAIL_COUNT=0

section() {
  echo
  echo "== $1 =="
}

pass() {
  echo "[PASS] $1"
  PASS_COUNT=$((PASS_COUNT + 1))
}

warn() {
  echo "[WARN] $1"
  WARN_COUNT=$((WARN_COUNT + 1))
}

fail() {
  echo "[FAIL] $1"
  FAIL_COUNT=$((FAIL_COUNT + 1))
}

extract_plist_value() {
  local plist_path="$1"
  local key_path="$2"
  /usr/libexec/PlistBuddy -c "Print $key_path" "$plist_path" 2>/dev/null || true
}

section "Project"
echo "App name: $APP_NAME"
echo "Bundle ID: $BUNDLE_ID"
echo "Repo root: $REPO_ROOT"

section "Toolchain"
if command -v xcodebuild >/dev/null 2>&1; then
  pass "Xcode command line tools available: $(xcode-select -p)"
else
  fail "xcodebuild not found"
fi

if command -v codesign >/dev/null 2>&1; then
  pass "codesign available"
else
  fail "codesign not found"
fi

section "Signing Identities"
IDENTITIES="$(security find-identity -v -p codesigning 2>/dev/null || true)"
if grep -Fq "Apple Development:" <<<"$IDENTITIES"; then
  pass "Apple Development identity is usable"
else
  warn "Apple Development identity not found in codesigning identities"
fi

if grep -Fq "Apple Distribution:" <<<"$IDENTITIES"; then
  pass "Apple Distribution identity is usable for App Store signing"
else
  DISTRIBUTION_CERT="$(security find-certificate -a -c 'Apple Distribution' -Z 2>/dev/null || true)"
  if [[ -n "$DISTRIBUTION_CERT" ]]; then
    warn "Apple Distribution certificate exists, but no usable codesigning identity was found. This usually means the private key is missing on this Mac."
  else
    warn "Apple Distribution certificate not found yet"
  fi
fi

if grep -Fq "Developer ID Application:" <<<"$IDENTITIES"; then
  pass "Developer ID Application identity is usable for direct-download builds"
else
  warn "Developer ID Application identity not found"
fi

section "Provisioning Profiles"
if [[ ! -d "$PROFILE_DIR" ]]; then
  warn "Provisioning profile directory does not exist yet: $PROFILE_DIR"
else
  shopt -s nullglob
  PROFILE_FILES=("$PROFILE_DIR"/*.provisionprofile "$PROFILE_DIR"/*.mobileprovision)
  shopt -u nullglob

  if [[ ${#PROFILE_FILES[@]} -eq 0 ]]; then
    warn "No provisioning profiles found in $PROFILE_DIR"
  else
    pass "Found ${#PROFILE_FILES[@]} provisioning profile(s)"
    MATCHING_PROFILE_FOUND=0
    for profile in "${PROFILE_FILES[@]}"; do
      tmp_plist="$(mktemp)"
      if ! security cms -D -i "$profile" > "$tmp_plist" 2>/dev/null; then
        rm -f "$tmp_plist"
        continue
      fi

      profile_name="$(extract_plist_value "$tmp_plist" ":Name")"
      profile_uuid="$(extract_plist_value "$tmp_plist" ":UUID")"
      profile_team="$(extract_plist_value "$tmp_plist" ":TeamIdentifier:0")"
      profile_app_id="$(extract_plist_value "$tmp_plist" ":Entitlements:application-identifier")"
      if [[ -z "$profile_app_id" ]]; then
        profile_app_id="$(extract_plist_value "$tmp_plist" ":Entitlements:com.apple.application-identifier")"
      fi

      if [[ "$profile_app_id" == *".${BUNDLE_ID}" || "$profile_app_id" == "$BUNDLE_ID" ]]; then
        MATCHING_PROFILE_FOUND=1
        echo "  - Match: ${profile_name:-unknown} | UUID=${profile_uuid:-unknown} | Team=${profile_team:-unknown}"
      fi

      rm -f "$tmp_plist"
    done

    if [[ "$MATCHING_PROFILE_FOUND" == "1" ]]; then
      pass "At least one provisioning profile matches $BUNDLE_ID"
    else
      warn "No provisioning profile currently matches $BUNDLE_ID"
    fi
  fi
fi

section "Entitlements"
if [[ ! -f "$ENTITLEMENTS_PATH" ]]; then
  fail "MAS entitlements file missing: $ENTITLEMENTS_PATH"
else
  pass "Found MAS entitlements file: $ENTITLEMENTS_PATH"

  sandbox_enabled="$(extract_plist_value "$ENTITLEMENTS_PATH" ":'com.apple.security.app-sandbox'")"
  audio_input_enabled="$(extract_plist_value "$ENTITLEMENTS_PATH" ":'com.apple.security.device.audio-input'")"
  network_client_enabled="$(extract_plist_value "$ENTITLEMENTS_PATH" ":'com.apple.security.network.client'")"

  if [[ "$sandbox_enabled" == "1" || "$sandbox_enabled" == "true" || "$sandbox_enabled" == "YES" ]]; then
    pass "App Sandbox entitlement enabled"
  else
    fail "App Sandbox entitlement is not enabled"
  fi

  if [[ "$audio_input_enabled" == "1" || "$audio_input_enabled" == "true" || "$audio_input_enabled" == "YES" ]]; then
    pass "Audio input entitlement enabled"
  else
    warn "Audio input entitlement is not enabled"
  fi

  if [[ "$network_client_enabled" == "1" || "$network_client_enabled" == "true" || "$network_client_enabled" == "YES" ]]; then
    warn "Network client entitlement is enabled. Keep it only while first-run model download still exists."
  else
    pass "Network client entitlement is not enabled"
  fi
fi

section "Current Repository Risks"
if rg -n "osascript" src/cli/daemon.rs >/dev/null 2>&1; then
  warn "Current repository still contains osascript-based update/elevation paths. These must stay out of the Mac App Store build."
else
  pass "No osascript path found in current App Store-sensitive code"
fi

if rg -n "system_audio" src settings-app >/dev/null 2>&1; then
  warn "System audio code paths still exist in the repository. Keep them gated out of the Mac App Store build."
else
  pass "No system audio code path found"
fi

section "Suggested Next Steps"
if [[ "$FAIL_COUNT" -gt 0 ]]; then
  echo "1. Fix the FAIL items first."
elif [[ "$WARN_COUNT" -gt 0 ]]; then
  echo "1. Resolve the WARN items before preparing an App Store archive."
else
  echo "1. Preflight is clean enough to move on to archive/signing validation."
fi
echo "2. If Apple Distribution is only a certificate but not an identity, re-create or download the certificate together with its private key on this Mac."
echo "3. Create an App Store provisioning profile for $BUNDLE_ID once App ID and capabilities are finalized."

section "Summary"
echo "PASS=$PASS_COUNT WARN=$WARN_COUNT FAIL=$FAIL_COUNT"
if [[ "$FAIL_COUNT" -gt 0 ]]; then
  exit 1
fi
