#!/bin/bash
# 把 SkillDock 打成一个可以直接分发的 DMG：
#   打开后左边是 SkillDock.app，右边是「应用程序」软链 —— 拖一下就算装好。
# 里面还带一份使用说明.txt（首次打开被 Gatekeeper 拦怎么办、前置依赖是啥）。
#
# 用法：
#   tools/make_dmg.sh                 # 输出到 ~/Desktop/SkillDock-<版本>.dmg
#   tools/make_dmg.sh /tmp/out        # 输出到指定目录
#
# 为什么从 /Applications 里的那份拿 app：那是已经改好显示名、重新 ad-hoc 签名、
# 并且跑过 --selfcheck 的东西。直接拿 target/ 里的原始产物会漏掉改名和重签的步骤。
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APP_NAME="SkillDock"
INSTALLED="/Applications/$APP_NAME.app"
BUILT="$ROOT/src-tauri/target/release/bundle/macos/$APP_NAME.app"

VERSION="$(/usr/bin/python3 -c "import json;print(json.load(open('$ROOT/src-tauri/tauri.conf.json'))['version'])" 2>/dev/null || echo "0.0.0")"
OUTDIR="${1:-$HOME/Desktop}"
OUT="$OUTDIR/$APP_NAME-$VERSION.dmg"

if [ -d "$INSTALLED" ]; then
  SRC="$INSTALLED"
  echo "用已安装的那份（已改名 + 已重签）：$SRC"
elif [ -d "$BUILT" ]; then
  SRC="$BUILT"
  echo "还没装过，用编译产物：$SRC"
else
  echo "找不到 $APP_NAME.app。先跑： cd $ROOT && npx tauri build && ./tools/install_app.sh"
  exit 1
fi

STAGE="$(mktemp -d /tmp/skilldock-dmg.XXXXXX)"
trap 'rm -rf "$STAGE"' EXIT

echo "准备内容…"
cp -R "$SRC" "$STAGE/$APP_NAME.app"
ln -s /Applications "$STAGE/应用程序"

cat > "$STAGE/使用说明.txt" <<'TXT'
SkillDock · 技能坞 —— 多 agent 技能总管
========================================

安装
----
1. 把左边的 SkillDock.app 拖进右边的「应用程序」
2. 第一次打开：在「应用程序」里右键点它 → 选「打开」→ 再点一次「打开」。
   这个 app 是本地自签名的，没有苹果开发者签名，直接双击会被 Gatekeeper 拦住；
   用「右键 → 打开」放行一次之后就正常了。
   要是还不行，打开「终端」粘这一行回车（去掉下载隔离标记）：
       xattr -dr com.apple.quarantine /Applications/SkillDock.app

前置依赖（重要）
----------------
这个 app 本身只是界面，真正的同步 / 备份 / 恢复全部交给本机的技能引擎：

    ~/AgentSkillHub/bin/skill-sync.py

也就是说 ~/AgentSkillHub 这个目录得在。换台机器只装 app 是不够的 ——
界面能打开，但一点按钮就会报「找不到引擎脚本」。
把 ~/AgentSkillHub 一起带过去即可（或者在终端跑 skill-sync init 先建一个空库）。

这个 app 干什么
---------------
· 技能          看全部技能正文，能直接改（存盘自动同步，带署名）
· 一键索引模式  把各家 agent 目录里多出来的技能收进中心库，
                然后每家只留一个 skills-index 门牌（正文只存一份，省上下文）
· 同步到各家    常规同步
· 备份并归档…   打一个 .zip 整包快照，并让你选一个目录再存一份
· 体检          配置 / 权限 / 软链健康度
· 流水 / 回收站 / 各家配置
                谁在什么时候改了什么；旧版本全在回收站，随时能捞回来

同一套东西的命令行版：skill-sync（引擎）、skill-tui（终端界面）
TXT

echo "打包成 DMG…"
rm -f "$OUT"
hdiutil create \
  -volname "$APP_NAME" \
  -srcfolder "$STAGE" \
  -fs HFS+ \
  -format UDZO \
  -ov \
  "$OUT" >/dev/null

echo "校验…"
hdiutil verify "$OUT" >/dev/null && echo "  DMG 校验通过"

SIZE="$(du -h "$OUT" | awk '{print $1}')"
SHA="$(shasum -a 256 "$OUT" | awk '{print $1}')"

echo
echo "打好了：$OUT"
echo "  体积    $SIZE"
echo "  版本    $VERSION"
echo "  SHA256  $SHA"
echo
echo "分发提醒：里面的 app 是 ad-hoc 签名，别人第一次打开要「右键 → 打开」放行一次；"
echo "而且对方机器上也得有 ~/AgentSkillHub 才能真的用起来（详见包里的 使用说明.txt）。"
