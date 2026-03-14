#!/usr/bin/env bash
# 一键发版：打 CLI tar.gz + .app zip，并创建 GitHub Release（仅 macOS）
# 用法：先改 Cargo.toml version，commit 并 push，再执行 ./scripts/release.sh [可选：release notes]
#
# 若需要同时发布 macOS + Linux + Windows：先不要执行本脚本，改为只打 tag 并推送：
#   git tag vX.Y.Z && git push origin vX.Y.Z
# 然后由 GitHub Actions（.github/workflows/release.yml）自动构建三端并创建 Release（macOS 含 .app，Linux/Windows 仅 CLI）。
set -e

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO_ROOT"

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
"$REPO_ROOT/scripts/build-app.sh"

# 3) .app 打成 zip 供用户下载
APP_ZIP="Open-Flow-${VERSION}-macos-${TARGET}.app.zip"
echo ""
echo "Zipping .app..."
(cd dist && zip -rq "$APP_ZIP" "Open Flow.app")
echo "  -> dist/$APP_ZIP"

# 4) GitHub Release
RELEASE_NOTES="${1:-}"
if [[ -z "$RELEASE_NOTES" ]]; then
  RELEASE_NOTES="## v${VERSION}

- **CLI**：下载 \`open-flow-${TARGET}.tar.gz\`，解压后放入 PATH，运行 \`open-flow start\`
- **App**：下载 \`${APP_ZIP}\`，解压得到「Open Flow.app」，拖到应用程序即可双击运行

平台：当前仅提供 ${TARGET} 预编译包。"
fi

echo ""
echo "Creating GitHub Release $TAG..."
gh release create "$TAG" \
  "dist/open-flow-${TARGET}.tar.gz" \
  "dist/${APP_ZIP}" \
  --title "$TAG" \
  --notes "$RELEASE_NOTES"

echo ""
echo "Done. Release: https://github.com/jqlong17/open-flow/releases/tag/$TAG"
