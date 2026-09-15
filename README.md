<div align="center">

# ⚓ AgentHarbor

**The harbor for your AI agents' skills and MCP servers.**

[English](README.md) · [简体中文](README.zh-CN.md)

*One place to keep every AI agent's skills in sync — and to stop MCP servers from
burning your token budget on tools you're not using.*

`Rust` · `Tauri 2` · `macOS 11+` · **no bundled Chromium** · installer ≈ 2.4 MB

[![Release](https://img.shields.io/github/v/release/missolin/agentharbor?include_prereleases&sort=semver)](../../releases)
[![License: MIT](https://img.shields.io/badge/License-MIT-0b6e6e.svg)](LICENSE)
[![Platform](https://img.shields.io/badge/platform-macOS%20Apple%20Silicon-lightgrey)](#install)

</div>

---

If you run several AI agents side by side (Claude Code, opencode, Codex, ZCode, pi,
DeepSeek Harness…), you end up maintaining **two parallel copies of the same mess**:

| | Skills | MCP servers |
|---|---|---|
| What it looks like | the same skill sits in five or six agent directories, each unaware of the others | the same server is configured in five or six config files |
| What it costs | some agents dump **every skill's full body** into the context | a live server injects its **entire tool schema into every single request** (`chrome-devtools` ≈ 29 tools, `blender` ≈ 28) |
| When it breaks | you fat-finger a skill and there's no history, no backup | you break a config and have no idea what it used to look like |

AgentHarbor fixes both with the **same symmetric idea**:

1. **One authoritative copy** — skill bodies live in a central repo; MCP server
   definitions live in one central registry.
2. **One entry point per agent** — skills: a single `skills-index` door file
   (≈43 tokens resident); MCP: a single `mount-mcp` entry that connects to **nothing**
   until you ask for it.
3. **Everything is logged** — structured journal + automatic git commit on every action.
4. **Nothing is ever lost** — old versions go to a trash dir / backup dir before being
   overwritten, always restorable.

## Pages

| Page | What it does |
|---|---|
| **Skills** | every skill in the central repo (size / chars / description / which agents still hold copies); edit `SKILL.md` in place — saving syncs and attributes the change; create new skills |
| **MCP** | central registry + per-vendor status; scan / import / show plan / collapse to `mount-mcp`; mount-server self-check |
| **Journal** | who changed what, when — the engine's structured log merged with git history into one timeline |
| **Backups** | full `.zip` snapshots, restore everything or just one skill, delete; old `.tar.gz` backups still work |
| **Trash** | every replaced version, one click to bring back, or reveal in Finder |
| **Agents** | where each integration points, which mode, whether its door file is mounted, how many stray copies are left |

Header actions:

- **Enable index mode** — adopt every stray skill into the central repo, clear the
  bodies out of agent dirs, leave each one with a single `skills-index` door file that is
  **regenerated dynamically** from the repo;
- **Restore to agents** — the *inverse*: spread every skill back into every agent dir,
  remove the door files, flip the mode to `copies`. Use it when a tool doesn't understand
  door files, or you simply want real skill directories everywhere;
- **Sync** / **Doctor** / **Backup & archive…** (`.zip` snapshot + pick an export folder).

## How the MCP side actually saves tokens

A live MCP server contributes its full tool schema to **every** request, used or not.
Collapsing to one `mount-mcp` entry fixes that — the entry itself connects to nothing
and exposes only four small tools:

| Tool | What it does |
|---|---|
| `mcp_list` | list configured servers and what's mounted; pass `server` to get that server's full tool schemas |
| `mcp_mount` | mount a server. Its tools then appear as `mcp__<server>__<tool>` (plus a `notifications/tools/list_changed`) |
| `mcp_unmount` | unmount: kill the child, tools vanish, tokens come back |
| `mcp_call` | call directly (auto-mounts) — the **universal fallback** for clients that don't support dynamic tool lists |

Each vendor's mechanism differs, because their plugin systems differ:

| Vendor | After collapsing |
|---|---|
| WorkBuddy / Claude Code / pi / Codex | config holds exactly one `mount-mcp` entry |
| opencode | native `mcp` section cleared; `mcp-servers.json` **symlinked** to the central registry, its own `mcp-on-demand.js` plugin keeps doing the mounting (in-process — cheaper than going through MCP at all) |
| DeepSeek Harness | has its own cordis plugin, left untouched |

> That's why opencode shows **0 resident servers** — not "not configured", but
> "doesn't need this anymore".

**Every vendor's original config is backed up** to `~/AgentSkillHub/mcp/.backups/`
before anything is written.

## The one iron rule

**This app never writes repository content itself** (other than the one skill you
explicitly save in the editor). Every sync / backup / restore / MCP operation is
handed to the external engine, so:

- the GUI and the CLI share the **same safety net** (back up before overwrite,
  trash instead of delete, suspicious shrinkage is blocked);
- if the UI crashes, your data is fine;
- none of this is GUI-only — everything is available from the command line.

## Prerequisites (important)

**This repo is only the GUI.** All real work is done by an external engine:

```
~/AgentSkillHub/bin/skill-sync.py             # sync engine (separate project)
~/AgentSkillHub/skills/                        # central skill repo
~/AgentSkillHub/mcp/mount-mcp/server.py        # universal on-demand MCP mount server
```

Without them the window opens and looks fine, but every button reports
"engine script not found". Bring `~/AgentSkillHub` along when moving machines.

## Install

Download `AgentHarbor-<version>.dmg` from [Releases](../../releases) and drag
`AgentHarbor.app` into **Applications**.

This app is **ad-hoc signed** (no Apple Developer certificate), so the first launch
needs **right-click → Open** to approve once. If it still complains:

```bash
xattr -dr com.apple.quarantine /Applications/AgentHarbor.app
```

## Build from source

Requires Rust 1.77+ and Node 18+ (only to fetch the prebuilt Tauri CLI).

```bash
npm install                     # just to get the tauri CLI
npx tauri dev                   # dev mode (frontend hot reload)

# production
cd src-tauri && cargo build --release -j 8
cd .. && npx tauri build        # bundles AgentHarbor.app
./tools/install_app.sh          # install to /Applications + re-sign + retire old versions
./tools/make_dmg.sh             # distributable DMG (defaults to ~/Desktop)
```

`agentharbor --selfcheck` runs every read-only backend command against the real repo
and prints the result — skills, journal, backups, trash, per-agent config, the MCP
registry and a `mount-mcp` self-check. Use it when the window won't open or you suspect
the UI is showing stale data.

## Architecture

```
ui/                     pure static frontend (HTML + vanilla JS + CSS, no bundler)
  index.html  app.js  style.css
src-tauri/src/main.rs   Rust backend: read-only data shaping + one command per action
                        —— every write is a shell-out to skill-sync.py
src-tauri/tauri.conf.json
tools/install_app.sh    install to /Applications + re-sign + refresh LaunchServices
tools/make_dmg.sh       DMG (app + Applications symlink + readme)
tools/make_icon.py      icon drawn with a signed distance field (pure stdlib, reproducible)
```

The backend exposes two kinds of commands:

- **Read-only**: `status` / `skills` / `skill_body` / `timeline` / `backups` / `trash` /
  `dictionary` / `mcp_status` / `mcp_plan` / `mcp_selfcheck`
- **Writing**: `run_sync` / `enable_index` / `reindex` / `spread_copies` / `make_backup` /
  `restore_backup` / `save_skill` / `new_skill` / `restore_trash` / `mcp_import` / `mcp_apply`
  — all delegated to `skill-sync.py`.

## Resource budget (first build, M4 Max)

- `target/` ≈ 1.5–2.5 GB; incremental rebuild ≈ 20 s with a warm crate cache
- `-j 8` on purpose, not the default parallelism: 16 concurrent `rustc` peak at 5–6 GB,
  which on a 48 GB machine pushes free memory under 1 GB and starts swapping;
  `-j 8` peaks around 2–3 GB
- to clean: `rm -rf src-tauri/target`

## The icon

The icon is **drawn in code** with a signed distance field — not AI-generated — so the
colors are exact and it's reproducible any time:

```bash
python3 tools/make_icon.py --icns   # source-1024.png + icon.icns + every png size
```

Tweak the constants at the top of the script and re-run.

## License

MIT — see [LICENSE](LICENSE).
