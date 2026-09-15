# SkillDock · 技能坞

> 多 agent 技能总管 —— 把「好几个 AI agent 各自维护一份技能库」这件事收成一套有主次、
> 有流水、能回滚的体系。Rust + 系统 WebView（Tauri 2），**不打包 Chromium**，安装包 2.3 MB。

## 它解决什么问题

如果你同时在用好几个 AI agent（Claude Code、opencode、ZCode、Codex、pi……），迟早会遇到：

- 同一个技能在**五六个目录里各躺一份**，改了一处，别处毫不知情；
- 有的 agent 会把**全部技能正文一次性塞进上下文**，几十万字，钱和注意力都烧在没用的技能上；
- 手滑改坏了某个技能，**没有历史、没有备份**，只能凭记忆往回写。

SkillDock 的做法是：

1. **正文只存一份** —— 全部技能正文放在一个中心仓库，那是唯一权威副本；
2. **每家只挂一个门牌** —— 每个 agent 目录里只放一个 `skills-index`（一份索引清单，
   约 3 千字 ≈ 3 千 token），要用哪个技能再去读那一份正文。常驻开销从每家几千 token
   降到几十 token；
3. **谁改了什么都留痕** —— 每次同步写一条结构化流水，并且自动 git commit，
   全历史可回溯；
4. **旧版本不会消失** —— 覆盖前先把旧版移进回收站，随时捞回来；随时能打整包快照回滚。

## 界面

| 页 | 干什么 |
|---|---|
| **技能** | 列出中心仓库的全部技能（体积 / 字数 / 描述 / 哪些 agent 还留着副本）；直接编辑 SKILL.md，存盘自动同步并署名；新建技能 |
| **流水** | 谁在什么时候改了什么 —— 引擎的结构化日志 + git 提交历史合成一条时间线 |
| **备份** | 打整包快照（`.zip`）、整体回退到某个时间点、只回退某一个技能、删包；旧的 `.tar.gz` 备份也认 |
| **回收站** | 所有被清掉的旧版本，一键捞回中心仓库，或在 Finder 里查看 |
| **各家配置** | 每个接入方指向哪个目录、什么模式、**门牌挂没挂**、门牌外还剩几个技能 |

顶栏四个动作：

- **一键索引模式** —— 把所有接入方目录里多出来的技能收编进中心仓库、把正文副本清进回收站，
  然后在每家只留一个随中心仓库**动态生成**的 `skills-index` 门牌；
- **同步到各家** —— 常规同步；
- **体检** —— 配置 / 权限 / 软链健康度；
- **备份并归档…** —— 打一个 `.zip` 快照，并让你选一个目录再存一份（方便归档到别处）。

## 一条铁律

**这个 app 自己不改仓库内容**（除了你在编辑器里明确保存的那一个技能）。
同步 / 备份 / 恢复全部转交给外部引擎 `skill-sync.py` 执行，所以：

- 图形界面和命令行走的是**同一套安全网**（覆盖先备份、旧版进回收站、可疑瘦身会被拦下）；
- 界面本身崩了也**不会把技能搞坏**；
- 这套东西不是 GUI 独占的 —— 全套能力在命令行里一样有。

## 前置依赖（重要）

**这个仓库只是图形界面。**它靠 shell 调用外部引擎来完成所有实际动作：

```
~/AgentSkillHub/bin/skill-sync.py      # 同步引擎（另一个项目）
~/AgentSkillHub/                        # 技能中心仓库（技能正文都在这儿）
```

没有这两个东西，界面能打开、能看，但一点按钮就会报「找不到引擎脚本」。
这个 GUI 与 `skill-sync` 引擎是配套的：引擎负责所有安全机制，GUI 只负责看得清、点得动。

## 安装

从 [Releases](../../releases) 下载 `SkillDock-<版本>.dmg`，打开后把 `SkillDock.app`
拖进「应用程序」。

这个 app 是**本地 ad-hoc 签名**的，没有 Apple 开发者签名，所以第一次打开要
**右键点图标 → 打开** 放行一次（之后正常）。要是还不行，去掉下载隔离标记：

```bash
xattr -dr com.apple.quarantine /Applications/SkillDock.app
```

## 从源码构建

需要：Rust 1.77+、Node 18+（只为拿 Tauri 官方预编译 CLI）。

```bash
npm install                     # 只为拿 tauri CLI
npx tauri dev                   # 开发模式（前端热重载）

# 出成品
cd src-tauri && cargo build --release -j 8
cd .. && npx tauri build        # 打包成 SkillDock.app
./tools/install_app.sh          # 装到 /Applications，并重新 ad-hoc 签名
./tools/make_dmg.sh             # 打可分发的 DMG（默认输出到桌面）
```

`skill-hub --selfcheck` 是个不开窗口的自检：把后端所有**只读**命令对着真实仓库跑一遍并打印。
界面起不来或者怀疑"界面拿到的数据不对"时，先跑它。

## 架构

```
ui/                     纯静态前端（HTML + 原生 JS + CSS，没有打包器）
  index.html  app.js  style.css
src-tauri/src/main.rs   Rust 后端：只读数据的整理 + 一个命令一个动作
                        —— 写操作全部 shell out 给 skill-sync.py
src-tauri/tauri.conf.json
tools/install_app.sh    装到 /Applications + 重签名 + 刷新 LaunchServices
tools/make_dmg.sh       打 DMG（app + 「应用程序」软链 + 使用说明）
tools/make_icon.py      用有符号距离场画图标（纯标准库，不调 AI，可复现）
```

后端对外只有两类命令：

- **只读**：`status` / `skills` / `skill_body` / `timeline` / `backups` / `trash` / `dictionary`
  —— 直接读文件系统、日志和 git 历史；
- **写**：`run_sync` / `enable_index` / `reindex` / `make_backup` / `restore_backup` /
  `save_skill` / `new_skill` / `restore_trash` —— 全部转交 `skill-sync.py`。

## 资源账（首次编译，M4 Max）

- `target/` 约 1.5–2.5 GB；crate 缓存命中时增量编译 20 秒左右
- 用 `-j 8` 而不是默认并行度：16 个 rustc 同时跑峰值会到 5–6 GB，
  48 GB 机器上会把空闲内存压到 1 GB 以下触发换页；`-j 8` 峰值约 2–3 GB
- 想清干净：`rm -rf src-tauri/target`

## 图标

图标是**代码画出来的**，不是 AI 生成的，所以颜色精确、随时可复现：

```bash
python3 tools/make_icon.py --icns   # 出 source-1024.png + icon.icns + 各尺寸 png
```

改配色、改构图只要动脚本顶部几个常量重新跑一遍。

## 许可

MIT，见 [LICENSE](LICENSE)。
