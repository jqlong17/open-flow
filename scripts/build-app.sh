#!/usr/bin/env bash
# 将 open-flow 二进制打成 macOS .app，含图标，双击即启动（等效 open-flow start --foreground）
set -e

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO_ROOT"

BINARY_NAME="open-flow"
APP_NAME="Open Flow"
BUNDLE_ID="com.openflow.open-flow"
DIST_APP_DIR="$REPO_ROOT/dist/${APP_NAME}.app"
INSTALL_APP_DIR="/Applications/${APP_NAME}.app"
SIGN_IDENTITY="${OPEN_FLOW_SIGN_IDENTITY:-}"
BUILD_PROFILE="${OPEN_FLOW_BUILD_PROFILE:-local}"

if [[ "$BUILD_PROFILE" != "local" && "$BUILD_PROFILE" != "distribution" ]]; then
  echo "Unsupported OPEN_FLOW_BUILD_PROFILE: $BUILD_PROFILE"
  echo "Expected: local or distribution"
  exit 1
fi

# Kill any running open-flow processes before rebuilding
echo "Stopping any running Open Flow instances..."
pkill -f "open-flow" 2>/dev/null || true
pkill -f "OpenFlowSettings" 2>/dev/null || true
sleep 1

echo "Building release binary..."
cargo build --release

echo "Building settings app..."
cd "$REPO_ROOT/settings-app"
if [[ "${OPENFLOW_PERF_DEV_UI:-0}" == "1" ]]; then
  echo "Developer performance UI: ENABLED"
else
  echo "Developer performance UI: disabled (default for distributable app builds)"
fi
swift build -c release
SETTINGS_BIN_DIR="$(swift build -c release --show-bin-path)"
cd "$REPO_ROOT"

SETTINGS_HELPER_PATH="$SETTINGS_BIN_DIR/OpenFlowSettings"
if [[ ! -f "$SETTINGS_HELPER_PATH" ]]; then
  LEGACY_SETTINGS_HELPER_PATH="$REPO_ROOT/settings-app/.build/release/OpenFlowSettings"
  if [[ -f "$LEGACY_SETTINGS_HELPER_PATH" ]]; then
    SETTINGS_HELPER_PATH="$LEGACY_SETTINGS_HELPER_PATH"
  else
    echo "Failed to locate OpenFlowSettings helper binary."
    echo "Checked:"
    echo "  $SETTINGS_HELPER_PATH"
    echo "  $LEGACY_SETTINGS_HELPER_PATH"
    exit 1
  fi
fi

echo "Using settings helper: $SETTINGS_HELPER_PATH"

SYSTEM_AUDIO_HELPER_PATH="$SETTINGS_BIN_DIR/OpenFlowSystemAudioHelper"
if [[ ! -f "$SYSTEM_AUDIO_HELPER_PATH" ]]; then
  LEGACY_SYSTEM_AUDIO_HELPER_PATH="$REPO_ROOT/settings-app/.build/release/OpenFlowSystemAudioHelper"
  if [[ -f "$LEGACY_SYSTEM_AUDIO_HELPER_PATH" ]]; then
    SYSTEM_AUDIO_HELPER_PATH="$LEGACY_SYSTEM_AUDIO_HELPER_PATH"
  else
    echo "Failed to locate OpenFlowSystemAudioHelper binary."
    echo "Checked:"
    echo "  $SYSTEM_AUDIO_HELPER_PATH"
    echo "  $LEGACY_SYSTEM_AUDIO_HELPER_PATH"
    exit 1
  fi
fi

echo "Using system audio helper: $SYSTEM_AUDIO_HELPER_PATH"
echo "Build profile: $BUILD_PROFILE"

echo "Creating .app structure..."
rm -rf "$DIST_APP_DIR"
mkdir -p "$DIST_APP_DIR/Contents/MacOS"
mkdir -p "$DIST_APP_DIR/Contents/Resources"

# 直接使用 Rust 二进制作为 app 主可执行文件，避免权限记录落在壳脚本上。
cp "$REPO_ROOT/target/release/$BINARY_NAME" "$DIST_APP_DIR/Contents/MacOS/open-flow"
chmod +x "$DIST_APP_DIR/Contents/MacOS/open-flow"

# Settings app helper
cp "$SETTINGS_HELPER_PATH" "$DIST_APP_DIR/Contents/MacOS/OpenFlowSettings"
chmod +x "$DIST_APP_DIR/Contents/MacOS/OpenFlowSettings"

# System audio helper
cp "$SYSTEM_AUDIO_HELPER_PATH" "$DIST_APP_DIR/Contents/MacOS/OpenFlowSystemAudioHelper"
chmod +x "$DIST_APP_DIR/Contents/MacOS/OpenFlowSystemAudioHelper"

# 图标
if [[ -f "$REPO_ROOT/assets/AppIcon.icns" ]]; then
  cp "$REPO_ROOT/assets/AppIcon.icns" "$DIST_APP_DIR/Contents/Resources/"
fi

# Info.plist（版本号从 Cargo.toml 读取）
VERSION=$(grep '^version' "$REPO_ROOT/Cargo.toml" | head -1 | sed 's/version = "\(.*\)"/\1/' | tr -d ' ')
cat > "$DIST_APP_DIR/Contents/Info.plist" << EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>CFBundleExecutable</key>
	<string>open-flow</string>
	<key>CFBundleIdentifier</key>
	<string>${BUNDLE_ID}</string>
	<key>CFBundleName</key>
	<string>${APP_NAME}</string>
	<key>CFBundleDisplayName</key>
	<string>${APP_NAME}</string>
	<key>CFBundleIconFile</key>
	<string>AppIcon</string>
	<key>CFBundlePackageType</key>
	<string>APPL</string>
	<key>CFBundleShortVersionString</key>
	<string>${VERSION}</string>
	<key>NSHighResolutionCapable</key>
	<true/>
	<key>LSUIElement</key>
	<true/>
	<key>NSMicrophoneUsageDescription</key>
	<string>Open Flow needs microphone access to record audio for real-time speech-to-text transcription.</string>
	<key>NSAudioCaptureUsageDescription</key>
	<string>Open Flow needs audio capture access for experimental system audio transcription features.</string>
	<key>NSSpeechRecognitionUsageDescription</key>
	<string>Open Flow uses a local speech recognition model to transcribe voice to text.</string>
	<key>NSAccessibilityUsageDescription</key>
	<string>Open Flow needs accessibility permission to detect global hotkeys and inject text.</string>
</dict>
</plist>
EOF

if [[ -z "$SIGN_IDENTITY" ]]; then
  if command -v security >/dev/null 2>&1; then
    IFS=$'\n' read -r -d '' -a SIGN_IDENTITIES < <(security find-identity -v -p codesigning 2>/dev/null | sed -n 's/.*"\(.*\)"/\1/p'; printf '\0')

    if [[ "$BUILD_PROFILE" == "distribution" ]]; then
      PREFERRED_PATTERNS=("Developer ID Application" "Apple Development")
    else
      PREFERRED_PATTERNS=("Apple Development" "Developer ID Application")
    fi

    for pattern in "${PREFERRED_PATTERNS[@]}"; do
      for id in "${SIGN_IDENTITIES[@]}"; do
        if [[ "$id" == *"$pattern"* ]]; then
          SIGN_IDENTITY="$id"
          break 2
        fi
      done
    done

    if [[ -z "$SIGN_IDENTITY" && ${#SIGN_IDENTITIES[@]} -gt 0 ]]; then
      SIGN_IDENTITY="${SIGN_IDENTITIES[0]}"
    fi
  fi
fi

if [[ -n "$SIGN_IDENTITY" ]]; then
  echo "Signing app bundle with identity: $SIGN_IDENTITY"
else
  SIGN_IDENTITY="-"
  echo "No code-sign identity found. Falling back to ad-hoc signing (-)."
  echo "Tip: export OPEN_FLOW_SIGN_IDENTITY=\"Apple Development: Your Name (TEAMID)\""
fi

SIGN_ARGS=(--force --sign "$SIGN_IDENTITY")
BUNDLE_SIGN_ARGS=(--force --deep --sign "$SIGN_IDENTITY")

if [[ "$SIGN_IDENTITY" == *"Developer ID Application"* ]]; then
  SIGN_ARGS+=(--options runtime --timestamp)
  BUNDLE_SIGN_ARGS+=(--options runtime --timestamp)
fi

codesign "${SIGN_ARGS[@]}" --identifier "${BUNDLE_ID}" "$DIST_APP_DIR/Contents/MacOS/open-flow"
codesign "${SIGN_ARGS[@]}" --identifier "${BUNDLE_ID}.settings" "$DIST_APP_DIR/Contents/MacOS/OpenFlowSettings"
codesign "${SIGN_ARGS[@]}" --identifier "${BUNDLE_ID}.system-audio-helper" "$DIST_APP_DIR/Contents/MacOS/OpenFlowSystemAudioHelper"
codesign "${BUNDLE_SIGN_ARGS[@]}" "$DIST_APP_DIR"

echo "Signing result:"
codesign -dv --verbose=2 "$DIST_APP_DIR" 2>&1 | sed -n 's/^Identifier=/  Identifier=/p; s/^TeamIdentifier=/  TeamIdentifier=/p; s/^Authority=/  Authority=/p'

echo "Installing to $INSTALL_APP_DIR ..."
rm -rf "$INSTALL_APP_DIR" 2>/dev/null || true
if ! cp -R "$DIST_APP_DIR" "/Applications/" 2>/dev/null; then
  sudo rm -rf "$INSTALL_APP_DIR"
  sudo cp -R "$DIST_APP_DIR" "/Applications/"
fi

echo "Done:"
echo "  Build artifact: $DIST_APP_DIR"
echo "  Installed app : $INSTALL_APP_DIR"
echo "  Run: open \"$INSTALL_APP_DIR\""
