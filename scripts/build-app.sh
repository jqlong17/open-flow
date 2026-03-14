#!/usr/bin/env bash
# 将 open-flow 二进制打成 macOS .app，含图标，双击即启动（等效 open-flow start --foreground）
set -e

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO_ROOT"

BINARY_NAME="open-flow"
APP_NAME="Open Flow"
BUNDLE_ID="com.openflow.open-flow"
APP_DIR="$REPO_ROOT/dist/${APP_NAME}.app"

# Kill any running open-flow processes before rebuilding
echo "Stopping any running Open Flow instances..."
pkill -f "open-flow" 2>/dev/null || true
pkill -f "OpenFlowSettings" 2>/dev/null || true
sleep 1

echo "Building release binary..."
cargo build --release

echo "Building settings app..."
cd "$REPO_ROOT/settings-app"
swift build -c release
cd "$REPO_ROOT"

echo "Creating .app structure..."
rm -rf "$APP_DIR"
mkdir -p "$APP_DIR/Contents/MacOS"
mkdir -p "$APP_DIR/Contents/Resources"

# 直接使用 Rust 二进制作为 app 主可执行文件，避免权限记录落在壳脚本上。
cp "$REPO_ROOT/target/release/$BINARY_NAME" "$APP_DIR/Contents/MacOS/open-flow"
chmod +x "$APP_DIR/Contents/MacOS/open-flow"

# Settings app helper
cp "$REPO_ROOT/settings-app/.build/release/OpenFlowSettings" "$APP_DIR/Contents/MacOS/OpenFlowSettings"
chmod +x "$APP_DIR/Contents/MacOS/OpenFlowSettings"

# 图标
if [[ -f "$REPO_ROOT/assets/AppIcon.icns" ]]; then
  cp "$REPO_ROOT/assets/AppIcon.icns" "$APP_DIR/Contents/Resources/"
fi

# Info.plist（版本号从 Cargo.toml 读取）
VERSION=$(grep '^version' "$REPO_ROOT/Cargo.toml" | head -1 | sed 's/version = "\(.*\)"/\1/' | tr -d ' ')
cat > "$APP_DIR/Contents/Info.plist" << EOF
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
	<key>NSSpeechRecognitionUsageDescription</key>
	<string>Open Flow uses a local speech recognition model to transcribe voice to text.</string>
	<key>NSAccessibilityUsageDescription</key>
	<string>Open Flow needs accessibility permission to detect global hotkeys and inject text.</string>
</dict>
</plist>
EOF

echo "Ad-hoc signing app bundle..."
# Use --identifier to keep a stable identity across rebuilds (preserves TCC permissions)
codesign --force --deep --sign - --identifier "${BUNDLE_ID}" "$APP_DIR/Contents/MacOS/open-flow"
codesign --force --deep --sign - --identifier "${BUNDLE_ID}.settings" "$APP_DIR/Contents/MacOS/OpenFlowSettings"
codesign --force --deep --sign - "$APP_DIR"

echo "Done: $APP_DIR"
echo "  Double-click to run (same as: open-flow start --foreground)"
echo ""
echo "  ⚡ To preserve permissions across rebuilds, install to /Applications:"
echo "     cp -R \"$APP_DIR\" /Applications/"
