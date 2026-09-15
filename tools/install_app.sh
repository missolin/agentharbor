#!/bin/bash
# 把编译好的 SkillDock.app 装到 /Applications/SkillDock.app 并刷新图标缓存。
#
# 为什么要重新签名：Tauri 会做 ad-hoc 签名，而我们改了 Info.plist（显示名），
# 一改签名就失效，macOS 会直接拒绝启动。所以改完必须再 ad-hoc 签一次。
#
# 顺带会把改名前的旧 app（技能中心.app）安全撤下 —— 移到废纸篓，不硬删，
# 免得 /Applications 里同时躺着两个图标让人犯迷糊。
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SRC="$ROOT/src-tauri/target/release/bundle/macos/SkillDock.app"
DEST="/Applications/SkillDock.app"
BUNDLE_ID="local.skilldock.app"
DISPLAY_NAME="SkillDock"

# 改名前的旧身份，遇到就撤下
OLD_DEST="/Applications/技能中心.app"
OLD_BUNDLE_ID="local.agentskillhub.skillhub"

LSREG="/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister"

if [ ! -d "$SRC" ]; then
  echo "找不到编译产物: $SRC"
  echo "先跑： cd $ROOT && npx tauri build"
  exit 1
fi

# 撤下旧 app：先注销 LaunchServices，再移进废纸篓（可还原）
if [ -e "$OLD_DEST" ]; then
  GOT="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$OLD_DEST/Contents/Info.plist" 2>/dev/null || echo "")"
  if [ "$GOT" = "$OLD_BUNDLE_ID" ]; then
    echo "撤下改名前的旧 app：$OLD_DEST → 废纸篓"
    "$LSREG" -u "$OLD_DEST" >/dev/null 2>&1 || true
    mkdir -p "$HOME/.Trash"
    mv "$OLD_DEST" "$HOME/.Trash/SkillDock-改名前的技能中心.app" 2>/dev/null \
      || echo "  （移废纸篓失败，先留在原地，不影响新 app 安装）"
  else
    echo "注意：$OLD_DEST 存在但不是我们的旧版本（bundle id=$GOT），不动它。"
  fi
fi

# 只在确认是「我们自己那个 app」的前提下才覆盖，避免误删同名目录
if [ -e "$DEST" ]; then
  GOT="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$DEST/Contents/Info.plist" 2>/dev/null || echo "")"
  if [ "$GOT" != "$BUNDLE_ID" ]; then
    echo "⚠️  $DEST 已存在，但它的 bundle id 是 '$GOT'，不是我们的（$BUNDLE_ID）。"
    echo "    为了不误删别人的东西，我不动它。请手工处理后再装。"
    exit 1
  fi
  echo "覆盖旧版本…"
  rm -rf "$DEST"
fi

echo "复制到 $DEST"
cp -R "$SRC" "$DEST"

PL="$DEST/Contents/Info.plist"
/usr/libexec/PlistBuddy -c "Set :CFBundleDisplayName $DISPLAY_NAME" "$PL" 2>/dev/null \
  || /usr/libexec/PlistBuddy -c "Add :CFBundleDisplayName string $DISPLAY_NAME" "$PL"
/usr/libexec/PlistBuddy -c "Set :CFBundleName $DISPLAY_NAME" "$PL" 2>/dev/null \
  || /usr/libexec/PlistBuddy -c "Add :CFBundleName string $DISPLAY_NAME" "$PL"

echo "重新 ad-hoc 签名（改了 Info.plist 必须重签，否则启动被拒）"
codesign --force --deep --sign - "$DEST" 2>&1 | sed 's/^/    /'
codesign --verify --verbose=1 "$DEST" 2>&1 | sed 's/^/    /' || true

echo "刷新 LaunchServices / 图标缓存"
"$LSREG" -f "$DEST" || true
touch "$DEST"

echo
echo "装好了：$DEST"
echo "现在可以在启动台/访达里双击打开，也可以拖到程序坞。"
