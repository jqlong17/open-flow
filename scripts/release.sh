#!/usr/bin/env bash
# 一键发版：使用当前 Mac 本机签名打包 CLI tar.gz + .app zip，并上传到 GitHub Release（仅 macOS）
# 用法：先改 Cargo.toml version，commit 并 push，再执行 ./scripts/release.sh [可选：release notes]
#
# 若需要同时发布 macOS + Linux + Windows CLI：先打 tag 并推送：
#   git tag vX.Y.Z && git push origin vX.Y.Z
# 然后由 GitHub Actions（.github/workflows/release.yml）自动构建三端 CLI 并创建 Release。
# macOS .app 请继续在本机执行本脚本上传，避免 CI 生成 ad-hoc 签名导致升级时重新请求权限。
set -e

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO_ROOT"
NOTARY_PROFILE="${OPEN_FLOW_NOTARY_PROFILE:-}"
ALLOW_UNNOTARIZED_RELEASE="${OPEN_FLOW_ALLOW_UNNOTARIZED_RELEASE:-0}"

# 从 Cargo.toml 读版本
VERSION=$(grep '^version' Cargo.toml | head -1 | sed 's/version = "\(.*\)"/\1/' | tr -d ' ')
if [[ -z "$VERSION" ]]; then
  echo "Could not read version from Cargo.toml"
  exit 1
fi

TAG="v${VERSION}"
ARCH=$(uname -m)
if [[ "$ARCH" == "arm64" ]]; then
  TARGET="aarch64-apple-darwin"
else
  TARGET="x86_64-apple-darwin"
fi

echo "Version: $VERSION | Tag: $TAG | Target: $TARGET"
echo ""
echo "This script publishes the macOS .app signed by the current machine."
echo "For public macOS distribution, use Developer ID signing and notarization."
echo ""

# 若 tag 不存在则创建并推送
if ! git rev-parse "$TAG" >/dev/null 2>&1; then
  echo "Creating tag $TAG..."
  git tag "$TAG"
  git push origin "$TAG"
else
  echo "Tag $TAG already exists, skipping git push"
fi

echo ""
echo "Building release binary..."
cargo build --release

mkdir -p dist

# 1) CLI 用 tar.gz
echo "Packaging CLI tar.gz..."
cp target/release/open-flow dist/
(cd dist && tar -czf "open-flow-${TARGET}.tar.gz" open-flow)
echo "  -> dist/open-flow-${TARGET}.tar.gz"

# 2) .app（调用 build-app.sh）
echo ""
OPEN_FLOW_BUILD_PROFILE=distribution OPENFLOW_PERF_DEV_UI=0 "$REPO_ROOT/scripts/build-app.sh"

APP_PATH="$REPO_ROOT/dist/Open Flow.app"
SIGNING_AUTHORITY="$(codesign -dv --verbose=4 "$APP_PATH" 2>&1 | sed -n 's/^Authority=//p' | head -1)"
echo "App signing authority: ${SIGNING_AUTHORITY:-unknown}"

if [[ "$SIGNING_AUTHORITY" != *"Developer ID Application"* ]]; then
  if [[ "$ALLOW_UNNOTARIZED_RELEASE" == "1" ]]; then
    echo "WARNING: app is not signed with Developer ID Application."
    echo "This package is suitable only for internal/manual distribution."
  else
    echo "ERROR: public release build must use a Developer ID Application certificate."
    echo "Current signing authority: ${SIGNING_AUTHORITY:-none}"
    echo "If you only want an internal test package, rerun with OPEN_FLOW_ALLOW_UNNOTARIZED_RELEASE=1."
    exit 1
  fi
fi

# 3) .app 打成 zip 供用户下载
APP_ZIP="Open-Flow-${VERSION}-macos-${TARGET}.app.zip"
echo ""
echo "Packaging .app with ditto..."
rm -f "dist/$APP_ZIP"
(cd dist && ditto -c -k --keepParent "Open Flow.app" "$APP_ZIP")
echo "  -> dist/$APP_ZIP"

if [[ -n "$NOTARY_PROFILE" ]]; then
  echo ""
  echo "Submitting app for notarization with profile: $NOTARY_PROFILE"
  xcrun notarytool submit "dist/$APP_ZIP" --keychain-profile "$NOTARY_PROFILE" --wait
  echo "Stapling notarization ticket..."
  xcrun stapler staple "$APP_PATH"
  xcrun stapler validate "$APP_PATH"
  echo "Repackaging stapled .app..."
  rm -f "dist/$APP_ZIP"
  (cd dist && ditto -c -k --keepParent "Open Flow.app" "$APP_ZIP")
elif [[ "$ALLOW_UNNOTARIZED_RELEASE" == "1" ]]; then
  echo ""
  echo "WARNING: notarization skipped. Downloaded app may still be blocked by Gatekeeper on other Macs."
else
  echo ""
  echo "ERROR: OPEN_FLOW_NOTARY_PROFILE is not set."
  echo "Public macOS downloads normally require notarization after Developer ID signing."
  echo "Set OPEN_FLOW_NOTARY_PROFILE to a configured notarytool keychain profile,"
  echo "or rerun with OPEN_FLOW_ALLOW_UNNOTARIZED_RELEASE=1 for internal-only sharing."
  exit 1
fi

# 4) GitHub Release
RELEASE_NOTES="${1:-}"
if [[ -z "$RELEASE_NOTES" ]]; then
  RELEASE_NOTES="## v${VERSION}

- **CLI**：下载 \`open-flow-${TARGET}.tar.gz\`，解压后放入 PATH，运行 \`open-flow start\`
- **App**：下载 \`${APP_ZIP}\`，解压得到「Open Flow.app」，拖到应用程序即可双击运行

平台：当前仅提供 ${TARGET} 预编译包。"
fi

echo ""
echo "Uploading macOS artifacts to GitHub Release $TAG..."
if gh release view "$TAG" >/dev/null 2>&1; then
  gh release upload "$TAG" \
    "dist/open-flow-${TARGET}.tar.gz" \
    "dist/${APP_ZIP}" \
    --clobber
  if [[ -n "$RELEASE_NOTES" ]]; then
    gh release edit "$TAG" --title "$TAG" --notes "$RELEASE_NOTES"
  fi
else
  gh release create "$TAG" \
    "dist/open-flow-${TARGET}.tar.gz" \
    "dist/${APP_ZIP}" \
    --title "$TAG" \
    --notes "$RELEASE_NOTES"
fi

echo ""
echo "Done. Release: https://github.com/jqlong17/open-flow/releases/tag/$TAG"
