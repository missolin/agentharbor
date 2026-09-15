# AgentHarbor · 智能体港

[English](README.md) · [简体中文](README.zh-CN.md)


> 多 agent 的**技能与 MCP 总管** —— 把「好几个 AI agent 各自维护一份技能库、各自配一遍
> MCP server」这件事收成一套有主次、有流水、能回滚的体系。
> Rust + 系统 WebView（Tauri 2），**不打包 Chromium**，安装包 2.4 MB。

## 它解决两个同构的问题

如果你同时在用好几个 AI agent（Claude Code、opencode、ZCode、Codex、pi、DeepSeek Harness……），
迟早会遇到这两件事——**它们的形状一模一样**：

| | 技能 | MCP server |
|---|---|---|
| 现在的样子 | 同一个技能在五六个目录里各躺一份，改了一处别处不知情 | 同一个 server 在五六个配置里各写一遍，token 也各算一遍 |
| 代价 | 有的 agent 把**全部技能正文**塞进上下文，几十万字 | 活着的 server 把**完整工具表**塞进每一次请求（chrome-devtools 29 个工具） |
| 出事之后 | 手滑改坏，没历史没备份 | 配置写坏了，不知道原来是啥样 |

AgentHarbor 的做法，两边完全对称：

1. **定义只存一份** —— 技能正文放一个中心仓库；MCP server 定义放一张中心注册表；
2. **每家只留一个入口** —— 技能：每家一个 `skills-index` 门牌（≈43 token）；
   MCP：每家一条 `mount-mcp`（平时**什么也不连**，要用哪个现挂）；
3. **谁改了什么都留痕** —— 每次动作写结构化流水 + 自动 git commit；
4. **旧版本不会消失** —— 覆盖前把旧的移进回收站 / 备份目录，随时捞回来。

## 界面

| 页 | 干什么 |
|---|---|
| **技能** | 中心仓库的全部技能（体积 / 字数 / 描述 / 哪些 agent 还留着副本）；直接编辑 SKILL.md，存盘自动同步并署名；新建技能 |
| **MCP** | 中心注册表 + 每家的 MCP 现状；扫描 / 收编 / 看收敛计划 / 收敛成 `mount-mcp`；挂载器自检 |
| **流水** | 谁在什么时候改了什么 —— 引擎的结构化日志 + git 提交历史合成一条时间线 |
| **备份** | 打整包快照（`.zip`）、整体回退、只回退某一个技能、删包；旧的 `.tar.gz` 也认 |
| **回收站** | 所有被清掉的旧版本，一键捞回，或在 Finder 里查看 |
| **各家配置** | 每个接入方指向哪个目录、什么模式、门牌挂没挂、门牌外还剩几个技能 |

顶栏动作：

- **一键索引模式** —— 把各家目录里多出来的技能收编进中心仓库、正文副本清进回收站，
  然后在每家只留一个随中心仓库**动态生成**的 `skills-index` 门牌；
- **恢复到各家** —— 上一条的**反操作**：把中心仓库的技能全部铺回每个目录、撤掉门牌、
  把模式翻成 `copies`。当某个工具不认门牌、或者你就是要每个目录里都有实体技能时用；
- **同步到各家** / **体检** / **备份并归档…**（打 `.zip` 快照并让你选一个目录再存一份）。

## MCP 那边具体怎么省

一个活着的 MCP server 会把自己的完整工具表塞进**每一次**请求，用不用都付。
`chrome-devtools` ≈ 29 个工具、`blender` ≈ 28 个，一起挂着就是十几 K token 的固定开销。

收敛之后，每家配置里只剩一条 `mount-mcp`。它平时**一个子 server 都不连**，只暴露四个小工具：

| 工具 | 作用 |
|---|---|
| `mcp_list` | 列出注册表里有哪些 server、当前哪些已挂载；带 `server` 参数则返回那个 server 的完整工具表 |
| `mcp_mount` | 挂上一个 server。挂上后它的工具以 `mcp__<server>__<tool>` 出现（并发 `notifications/tools/list_changed`） |
| `mcp_unmount` | 卸载：杀进程、工具从工具表消失、token 还回去 |
| `mcp_call` | 直接调（没挂会自动挂）—— **万能兜底**，客户端不支持动态工具表时照样能干活 |

各家机制不一样，这是它们各自的插件体系决定的，不是我们偷懒：

| 厂商 | 收敛后是什么 |
|---|---|
| WorkBuddy / Claude Code / pi / Codex | 配置里只剩一条 `mount-mcp`，指向 `mcp/mount-mcp/server.py` |
| opencode | 自带 `mcp-on-demand.js` 插件，把它原生的 `mcp` 段清空，插件的配置**软链**到中心注册表 |
| DeepSeek Harness | 自带 cordis 插件 `dsh-mcp-on-demand`，不动它 |

**改配置前每一家的原文件都会备份**到 `~/AgentSkillHub/mcp/.backups/`。

## 一条铁律

**这个 app 自己不改仓库内容**（除了你在编辑器里明确保存的那一个技能）。
同步 / 备份 / 恢复 / MCP 收编全部转交给外部引擎 `skill-sync.py` 执行，所以：

- 图形界面和命令行走的是**同一套安全网**（覆盖先备份、旧版进回收站、可疑瘦身会被拦下）；
- 界面本身崩了也**不会把东西搞坏**；
- 这套东西不是 GUI 独占的 —— 全套能力在命令行里一样有。

## 前置依赖（重要）

**这个仓库只是图形界面。**它靠 shell 调用外部引擎来完成所有实际动作：

```
~/AgentSkillHub/bin/skill-sync.py             # 引擎（另一个项目）
~/AgentSkillHub/skills/                        # 技能中心仓库（技能正文都在这儿）
~/AgentSkillHub/mcp/mount-mcp/server.py        # 通用 MCP 按需挂载器（零依赖 Python）
```

没有这些，界面能打开、能看，但一点按钮就会报「找不到引擎脚本」。

## 安装

从 [Releases](../../releases) 下载 `AgentHarbor-<版本>.dmg`，打开后把 `AgentHarbor.app`
拖进「应用程序」。

这个 app 是**本地 ad-hoc 签名**的，没有 Apple 开发者签名，所以第一次打开要
**右键点图标 → 打开** 放行一次（之后正常）。要是还不行，去掉下载隔离标记：

```bash
xattr -dr com.apple.quarantine /Applications/AgentHarbor.app
```

## 从源码构建

需要：Rust 1.77+、Node 18+（只为拿 Tauri 官方预编译 CLI）。

```bash
npm install                     # 只为拿 tauri CLI
npx tauri dev                   # 开发模式（前端热重载）

# 出成品
cd src-tauri && cargo build --release -j 8
cd .. && npx tauri build        # 打包成 AgentHarbor.app
./tools/install_app.sh          # 装到 /Applications，并重新 ad-hoc 签名
./tools/make_dmg.sh             # 打可分发的 DMG（默认输出到桌面）
```

`agentharbor --selfcheck` 是个不开窗口的自检：把后端所有**只读**命令对着真实仓库跑一遍并打印，
包括技能、流水、备份、回收站、各家配置，以及 MCP 注册表和 `mount-mcp` 自检。
界面起不来或者怀疑"界面拿到的数据不对"时，先跑它。

## 架构

```
ui/                     纯静态前端（HTML + 原生 JS + CSS，没有打包器）
  index.html  app.js  style.css
src-tauri/src/main.rs   Rust 后端：只读数据的整理 + 一个命令一个动作
                        —— 写操作全部 shell out 给 skill-sync.py
src-tauri/tauri.conf.json
tools/install_app.sh    装到 /Applications + 重签名 + 刷新 LaunchServices + 撤下改名前旧版本
tools/make_dmg.sh       打 DMG（app + 「应用程序」软链 + 使用说明）
tools/make_icon.py      用有符号距离场画图标（纯标准库，不调 AI，可复现）
```

后端对外只有两类命令：

- **只读**：`status` / `skills` / `skill_body` / `timeline` / `backups` / `trash` / `dictionary` /
  `mcp_status` / `mcp_plan` / `mcp_selfcheck` —— 直接读文件系统、日志和 git 历史；
- **写**：`run_sync` / `enable_index` / `reindex` / `spread_copies` / `make_backup` /
  `restore_backup` / `save_skill` / `new_skill` / `restore_trash` / `mcp_import` / `mcp_apply`
  —— 全部转交 `skill-sync.py`。

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
