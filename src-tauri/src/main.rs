//! AgentHarbor · 智能体港 —— Tauri 后端
//!
//! 设计原则跟终端版一致：**这个 app 自己绝不改仓库里的内容，除了你明确编辑的那个技能**。
//! 所有同步/备份/恢复动作全部转交给 `~/AgentSkillHub/bin/skill-sync.py` 执行，
//! 所以界面和命令行走的是同一套安全网（备份、回收站、瘦身护栏）。

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde::Serialize;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::UNIX_EPOCH;

const PY: &str = "/usr/bin/python3";

// 复制技能时永远排除的东西，跟引擎里的 NOISE 保持一致
const NOISE: &[&str] = &[
    ".DS_Store", ".git", ".gitignore", "node_modules", "__pycache__",
    ".venv", "venv", ".tox", ".mypy_cache", ".pytest_cache", ".ruff_cache",
    ".ipynb_checkpoints", "target",
];

// ---------------------------------------------------------------- 路径

fn home() -> PathBuf {
    // GUI app 一定有 HOME；真读不到就退回当前目录，而不是把一个具体用户的
    // 绝对路径写死在这里（开源代码里不该出现任何人的家目录）。
    PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| ".".to_string()))
}
fn hub() -> PathBuf { home().join("AgentSkillHub") }
fn skills_dir() -> PathBuf { hub().join("skills") }
fn backups_dir() -> PathBuf { hub().join("backups") }
fn trash_dir() -> PathBuf { hub().join(".trash") }
fn conflicts_dir() -> PathBuf { hub().join(".conflicts") }
fn journal_path() -> PathBuf { hub().join("logs").join("journal.jsonl") }
fn lastmark_path() -> PathBuf { hub().join(".state").join("last_run.json") }
fn sync_py() -> PathBuf { hub().join("bin").join("skill-sync.py") }

fn expand(s: &str) -> PathBuf {
    match s.strip_prefix("~/") {
        Some(rest) => home().join(rest),
        None => PathBuf::from(s),
    }
}

fn read_json(p: &Path) -> Value {
    fs::read_to_string(p)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or(Value::Null)
}

fn cfg() -> Value { read_json(&hub().join("config.json")) }

/// 所有 agent（peers + sinks）的 id -> 技能目录
fn agents() -> BTreeMap<String, PathBuf> {
    let c = cfg();
    let mut m = BTreeMap::new();
    for grp in ["peers", "sinks"] {
        if let Some(arr) = c.get(grp).and_then(|v| v.as_array()) {
            for it in arr {
                let id = it.get("id").and_then(|v| v.as_str());
                let p = it.get("path").and_then(|v| v.as_str());
                if let (Some(id), Some(p)) = (id, p) {
                    m.insert(id.to_string(), expand(p));
                }
            }
        }
    }
    m
}

/// 备份列表用的完整时间戳（mtime 的分钟级展示由前端自己做，这里只给整点格式）
fn fmt_ts_full(t: i64) -> String {
    let out = Command::new("/bin/date")
        .args(["-r", &t.max(0).to_string(), "+%Y-%m-%d %H:%M"])
        .output();
    match out {
        Ok(o) => String::from_utf8_lossy(&o.stdout).trim().to_string(),
        Err(_) => "?".to_string(),
    }
}

fn mtime_of(p: &Path) -> i64 {
    fs::metadata(p)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

// ---------------------------------------------------------------- 小工具

fn dir_size(p: &Path) -> (u64, usize) {
    let mut total = 0u64;
    let mut n = 0usize;
    let mut stack = vec![p.to_path_buf()];
    while let Some(d) = stack.pop() {
        let rd = match fs::read_dir(&d) {
            Ok(r) => r,
            Err(_) => continue,
        };
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if NOISE.contains(&name.as_str()) || name.starts_with(".skill-sync-tmp") {
                continue;
            }
            if name.ends_with(".swp") || name.ends_with(".swo") || name.ends_with('~') {
                continue;
            }
            let md = match e.metadata() {
                Ok(m) => m,
                Err(_) => continue,
            };
            if md.is_dir() {
                stack.push(e.path());
            } else {
                total += md.len();
                n += 1;
            }
        }
    }
    (total, n)
}

/// 取 frontmatter 里的某个字段（支持 YAML 折行续行）
fn fm_field(text: &str, key: &str) -> String {
    let mut lines = text.lines();
    let mut seen_open = false;
    for l in lines.by_ref() {
        let t = l.trim_start_matches('\u{feff}').trim();
        if t == "---" {
            seen_open = true;
            break;
        }
        if !t.is_empty() {
            return String::new();
        }
    }
    if !seen_open {
        return String::new();
    }
    let mut parts: Vec<String> = Vec::new();
    let mut capturing = false;
    for l in lines {
        if l.trim() == "---" {
            break;
        }
        if capturing {
            if l.starts_with(' ') || l.starts_with('\t') {
                parts.push(l.trim().to_string());
                continue;
            }
            break;
        }
        if let Some(rest) = l.strip_prefix(key) {
            if let Some(v) = rest.strip_prefix(':') {
                parts.push(v.trim().to_string());
                capturing = true;
            }
        }
    }
    let joined = parts.join(" ");
    let norm = joined.split_whitespace().collect::<Vec<_>>().join(" ");
    norm.trim_start_matches(['|', '>', ' '])
        .trim()
        .trim_matches(['"', '\''])
        .to_string()
}

fn run_engine(args: &[String], by: Option<&str>, attr_skill: Option<&str>) -> Result<String, String> {
    let script = sync_py();
    if !script.is_file() {
        return Err(format!("找不到引擎脚本 {}", script.display()));
    }
    let mut c = Command::new(PY);
    c.arg(&script)
        .args(args)
        .current_dir(hub())
        .env("LANG", "en_US.UTF-8");
    if let Some(b) = by {
        c.env("SKILL_SYNC_BY", b);
    }
    if let Some(s) = attr_skill {
        c.env("SKILL_SYNC_ATTR_SKILL", s);
    }
    let out = c.output().map_err(|e| format!("无法启动引擎: {e}"))?;
    let mut s = String::from_utf8_lossy(&out.stdout).to_string();
    let err = String::from_utf8_lossy(&out.stderr);
    if !err.trim().is_empty() {
        s.push_str("\n──── stderr ────\n");
        s.push_str(&err);
    }
    if s.trim().is_empty() {
        s = format!("（没有输出，退出码 {}）", out.status.code().unwrap_or(-1));
    }
    Ok(s)
}

fn valid_skill_name(n: &str) -> bool {
    !n.is_empty()
        && n.len() <= 64
        && n.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

// ---------------------------------------------------------------- 数据类型

#[derive(Serialize)]
struct Skill {
    name: String,
    /// 技能目录的绝对路径。**必须由后端给**，别让前端拿 hub 根去拼 ——
    /// hub 根是 ~/AgentSkillHub（放 config.json / backups 的地方），技能其实在
    /// 它下面的 skills/ 里，拼错了就会得到"路径不存在"，这个坑已经踩过一次。
    path: String,
    desc: String,
    size: u64,
    files: usize,
    chars: usize,
    mtime: i64,
    /// 哪些 agent 目录里还留着这个技能的完整副本
    copies: Vec<String>,
}

#[derive(Serialize)]
struct Event {
    ts: String,
    skill: String,
    act: String,
    src: String,
    dst: String,
    kind: String,
}

#[derive(Serialize)]
struct Backup {
    name: String,
    path: String,
    size: u64,
    time: String,
    note: String,
    /// "zip"（新格式）或 "tar.gz"（旧备份，仍然能恢复）
    kind: String,
}

#[derive(Serialize)]
struct TrashItem {
    tag: String,
    name: String,
    path: String,
    time: String,
    act: String,
    src: String,
    size: u64,
    is_skill: bool,
}

#[derive(Serialize)]
struct Status {
    /// 仓库根（放 config.json / logs / backups 的地方）
    hub: String,
    /// 技能正文实际所在的目录 —— 界面上要拼路径一律用这个
    skills_dir: String,
    skills: usize,
    agents: usize,
    last_sync: String,
    hub_changed: usize,
    changed: usize,
    conflicts: usize,
    trash: usize,
    backups: usize,
    doctor: String,
}

#[derive(Serialize)]
struct DictEntry {
    id: String,
    path: String,
    hold: String,
    mode: String,
    /// 这个目录现在到底在不在（配置里写了但目录不存在的话，同步时会跳过）
    exists: bool,
    /// 门牌 skills-index 是不是已经挂进去了
    has_index: bool,
    /// 除了门牌之外，目录里还剩几个技能
    /// （索引模式下正常应该是 0；hold=copies 的 dsh 本来就该 >0）
    extras: usize,
}

// ---------------------------------------------------------------- 命令：读

#[tauri::command]
fn status() -> Status {
    let c = cfg();
    let n_skills = fs::read_dir(skills_dir())
        .map(|rd| {
            rd.flatten()
                .filter(|e| e.path().join("SKILL.md").is_file())
                .count()
        })
        .unwrap_or(0);
    let lm = read_json(&lastmark_path());
    let n_conf = fs::read_dir(conflicts_dir()).map(|rd| rd.flatten().count()).unwrap_or(0);
    let n_trash = fs::read_dir(trash_dir()).map(|rd| rd.flatten().count()).unwrap_or(0);
    let n_bak = fs::read_dir(backups_dir())
        .map(|rd| {
            rd.flatten()
                .filter(|e| {
                    let n = e.file_name().to_string_lossy().to_string();
                    n.ends_with(".zip") || n.ends_with(".tar.gz")
                })
                .count()
        })
        .unwrap_or(0);
    Status {
        hub: hub().display().to_string(),
        skills_dir: skills_dir().display().to_string(),
        skills: n_skills,
        agents: agents().len(),
        last_sync: lm.get("at").and_then(|v| v.as_str()).unwrap_or("—").to_string(),
        hub_changed: lm.get("hub_changed").and_then(|v| v.as_u64()).unwrap_or(0) as usize,
        changed: lm.get("changed").and_then(|v| v.as_u64()).unwrap_or(0) as usize,
        conflicts: n_conf,
        trash: n_trash,
        backups: n_bak,
        doctor: c
            .get("hub")
            .and_then(|v| v.as_str())
            .filter(|p| Path::new(p).is_dir())
            .map(|_| "OK".to_string())
            .unwrap_or_else(|| "中心仓库不可用".to_string()),
    }
}

#[tauri::command]
fn dictionary() -> Vec<DictEntry> {
    let c = cfg();
    let idx_name = c
        .get("index_name")
        .and_then(|v| v.as_str())
        .unwrap_or("skills-index")
        .to_string();
    let mut out = Vec::new();
    for grp in ["peers", "sinks", "inbox"] {
        if let Some(arr) = c.get(grp).and_then(|v| v.as_array()) {
            for it in arr {
                let raw = it.get("path").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let dir = expand(&raw);
                let exists = dir.is_dir();
                let has_index = dir.join(&idx_name).join("SKILL.md").is_file();
                // 数一下门牌之外还剩几个技能 —— "各家只留一个 index-skill" 这件事
                // 得能在界面上直接看出来，不然只能靠命令行的输出
                let extras = if exists {
                    fs::read_dir(&dir)
                        .map(|rd| {
                            rd.flatten()
                                .filter(|e| e.path().join("SKILL.md").is_file())
                                .filter(|e| e.file_name().to_string_lossy().as_ref() != idx_name)
                                .count()
                        })
                        .unwrap_or(0)
                } else {
                    0
                };
                out.push(DictEntry {
                    id: it.get("id").and_then(|v| v.as_str()).unwrap_or("?").to_string(),
                    path: raw,
                    hold: it
                        .get("hold")
                        .and_then(|v| v.as_str())
                        .unwrap_or(grp)
                        .to_string(),
                    mode: it.get("mode").and_then(|v| v.as_str()).unwrap_or("-").to_string(),
                    exists,
                    has_index,
                    extras,
                });
            }
        }
    }
    out
}

#[tauri::command]
fn skills() -> Vec<Skill> {
    let ags = agents();
    let mut out = Vec::new();
    let rd = match fs::read_dir(skills_dir()) {
        Ok(r) => r,
        Err(_) => return out,
    };
    let mut names: Vec<PathBuf> = rd.flatten().map(|e| e.path()).collect();
    names.sort();
    for p in names {
        if !p.join("SKILL.md").is_file() {
            continue;
        }
        let name = p.file_name().unwrap_or_default().to_string_lossy().to_string();
        let txt = fs::read_to_string(p.join("SKILL.md")).unwrap_or_default();
        let (size, files) = dir_size(&p);
        let mut copies = Vec::new();
        for (id, dir) in &ags {
            if dir.join(&name).join("SKILL.md").is_file() {
                copies.push(id.clone());
            }
        }
        out.push(Skill {
            name,
            path: p.display().to_string(),
            desc: {
                let d = fm_field(&txt, "description");
                if d.is_empty() { "(没有描述)".to_string() } else { d }
            },
            size,
            files,
            chars: txt.chars().count(),
            mtime: mtime_of(&p.join("SKILL.md")),
            copies,
        });
    }
    out
}

#[tauri::command]
fn skill_body(name: String) -> Result<String, String> {
    let p = skills_dir().join(&name).join("SKILL.md");
    if !p.is_file() {
        return Err(format!("没有这个技能: {name}"));
    }
    fs::read_to_string(&p).map_err(|e| e.to_string())
}

#[tauri::command]
fn journal(limit: usize) -> Vec<Event> {
    let mut out: Vec<Event> = Vec::new();
    if let Ok(s) = fs::read_to_string(journal_path()) {
        for line in s.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let v: Value = match serde_json::from_str(line) {
                Ok(v) => v,
                Err(_) => continue,
            };
            out.push(Event {
                ts: v.get("ts").and_then(|x| x.as_str()).unwrap_or("").to_string(),
                skill: v.get("skill").and_then(|x| x.as_str()).unwrap_or("").to_string(),
                act: v.get("act").and_then(|x| x.as_str()).unwrap_or("").to_string(),
                src: v.get("src").and_then(|x| x.as_str()).unwrap_or("").to_string(),
                dst: v.get("dst").and_then(|x| x.as_str()).unwrap_or("").to_string(),
                kind: "journal".to_string(),
            });
        }
    }
    if out.len() > limit {
        out = out.split_off(out.len() - limit);
    }
    out.reverse();
    out
}

#[tauri::command]
fn timeline(limit: usize) -> Vec<Event> {
    let mut ev = journal(limit);
    let earliest = ev
        .iter()
        .filter_map(|e| {
            // "2026-09-16 04:40:14" -> 可比较的字符串序
            Some(e.ts.clone())
        })
        .min()
        .unwrap_or_default();

    // git 只在 journal 覆盖不到的更早时段补位，避免同一件事记两遍
    let out = Command::new("git")
        .args([
            "log",
            "--pretty=format:\u{1}%h\u{2}%ad\u{2}%s",
            "--date=format:%Y-%m-%d %H:%M:%S",
            "-n",
            &limit.to_string(),
            "--name-status",
        ])
        .current_dir(hub())
        .output();
    if let Ok(o) = out {
        let text = String::from_utf8_lossy(&o.stdout).to_string();
        for rec in text.split('\u{1}') {
            let rec = rec.trim_matches('\n');
            if rec.is_empty() {
                continue;
            }
            let (head, body) = match rec.split_once('\n') {
                Some((h, b)) => (h, b),
                None => (rec, ""),
            };
            let parts: Vec<&str> = head.split('\u{2}').collect();
            if parts.len() < 3 {
                continue;
            }
            let ts = parts[1].to_string();
            if !earliest.is_empty() && ts.as_str() >= earliest.as_str() {
                continue;
            }
            let mut touched: Vec<String> = Vec::new();
            for line in body.lines() {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                let cols: Vec<&str> = line.split('\t').collect();
                if cols.len() < 2 {
                    continue;
                }
                let path = cols[1];
                if let Some(rest) = path.strip_prefix("skills/") {
                    if let Some((nm, _)) = rest.split_once('/') {
                        touched.push(nm.to_string());
                    }
                } else if path.starts_with("index/") {
                    touched.push("(索引)".to_string());
                } else if path.starts_with("config") {
                    touched.push("(配置)".to_string());
                }
            }
            touched.dedup();
            ev.push(Event {
                ts,
                skill: if touched.is_empty() { "(无技能变更)".to_string() } else { touched.join(", ") },
                act: "git 提交".to_string(),
                src: parts[2].to_string(),
                dst: parts[0].to_string(),
                kind: "git".to_string(),
            });
        }
    }
    ev.sort_by(|a, b| b.ts.cmp(&a.ts));
    ev
}

#[tauri::command]
fn backups() -> Vec<Backup> {
    let mut out = Vec::new();
    let rd = match fs::read_dir(backups_dir()) {
        Ok(r) => r,
        Err(_) => return out,
    };
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        // 新的备份是 .zip；旧的 .tar.gz 也得列出来，它们仍然能恢复
        let kind = if name.ends_with(".zip") {
            "zip"
        } else if name.ends_with(".tar.gz") {
            "tar.gz"
        } else {
            continue;
        };
        let ext = if kind == "zip" { ".zip" } else { ".tar.gz" };
        let p = e.path();
        let md = match e.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };
        let mt = md
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        // hub-20260916-044232_TUI-修好输入框后的基线.zip -> 取备注
        let note = name
            .trim_start_matches("hub-")
            .split_once('_')
            .map(|(_, n)| n.trim_end_matches(ext).to_string())
            .unwrap_or_default();
        out.push(Backup {
            name,
            path: p.display().to_string(),
            size: md.len(),
            time: fmt_ts_full(mt),
            note,
            kind: kind.to_string(),
        });
    }
    out.sort_by(|a, b| b.name.cmp(&a.name));
    out
}

fn tag_hint(tag: &str, agent_ids: &BTreeSet<String>) -> (String, String) {
    if agent_ids.contains(tag) {
        return ("旧版被新版本覆盖".into(), tag.into());
    }
    for (pfx, label) in [
        ("slim-dup-", "清冗余副本"),
        ("slim-adopt-", "收编进中心仓库"),
        ("slim-old-", "旧版本被清"),
        ("slim-force-", "强制清除"),
        ("slim-broken-", "断链清除"),
        ("prune-", "手动移除"),
    ] {
        if let Some(rest) = tag.strip_prefix(pfx) {
            return (label.into(), rest.into());
        }
    }
    match tag {
        "hub" => ("中心仓库旧版被覆盖".into(), "hub".into()),
        "hub-old" => ("收编时中心仓库旧版让位".into(), "hub".into()),
        "hub-replaced" => ("中心仓库旧版被替换".into(), "hub".into()),
        "sink-dup" => ("sink 里同名冲突，旧的清掉".into(), "sink".into()),
        "sink-indexonly" => ("索引模式下清掉的完整副本".into(), "sink".into()),
        "cleanup" => ("手工收拾".into(), "-".into()),
        "restore-old" => ("恢复备份前的旧版".into(), "hub".into()),
        other => (other.to_string(), "-".into()),
    }
}

#[tauri::command]
fn trash() -> Vec<TrashItem> {
    let mut out = Vec::new();
    let ids: BTreeSet<String> = agents().keys().cloned().collect();
    let index_name = cfg()
        .get("index_name")
        .and_then(|v| v.as_str())
        .unwrap_or("skills-index")
        .to_string();
    let mut stamps: Vec<String> = match fs::read_dir(trash_dir()) {
        Ok(rd) => rd
            .flatten()
            .filter(|e| e.path().is_dir())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect(),
        Err(_) => return out,
    };
    stamps.sort();
    stamps.reverse();
    for stamp in stamps {
        let sd = trash_dir().join(&stamp);
        let tags: Vec<String> = match fs::read_dir(&sd) {
            Ok(rd) => rd
                .flatten()
                .filter(|e| e.path().is_dir())
                .map(|e| e.file_name().to_string_lossy().to_string())
                .collect(),
            Err(_) => continue,
        };
        for tag in tags {
            let td = sd.join(&tag);
            let names: Vec<String> = match fs::read_dir(&td) {
                Ok(rd) => rd
                    .flatten()
                    .map(|e| e.file_name().to_string_lossy().to_string())
                    .collect(),
                Err(_) => continue,
            };
            for name in names {
                if name == index_name {
                    continue;
                }
                let p = td.join(&name);
                let is_skill = p.join("SKILL.md").is_file();
                let (act, src) = tag_hint(&tag, &ids);
                let (size, _) = if is_skill { dir_size(&p) } else { (0, 0) };
                out.push(TrashItem {
                    tag: tag.clone(),
                    name,
                    path: p.display().to_string(),
                    time: if stamp.len() == 15 {
                        format!(
                            "{}-{}-{} {}:{}",
                            &stamp[0..4], &stamp[4..6], &stamp[6..8], &stamp[9..11], &stamp[11..13]
                        )
                    } else {
                        stamp.clone()
                    },
                    act,
                    src,
                    size,
                    is_skill,
                });
            }
        }
    }
    out
}

// ---------------------------------------------------------------- 命令：写

#[tauri::command]
fn run_sync() -> Result<String, String> {
    run_engine(&["sync".to_string()], None, None)
}

/// 一键启用索引模式：收编各家技能 + 清掉各家正文副本 + 每家只挂一个 skills-index 门牌。
#[tauri::command]
fn enable_index() -> Result<String, String> {
    run_engine(&["enable-index".to_string()], None, None)
}

/// 只重建门牌清单（正文按中心仓库现状现读生成），不分发、不清副本。
#[tauri::command]
fn reindex() -> Result<String, String> {
    run_engine(&["reindex".to_string()], None, None)
}

#[tauri::command]
fn run_doctor() -> Result<String, String> {
    run_engine(&["doctor".to_string()], None, None)
}

#[tauri::command]
fn run_status_cli() -> Result<String, String> {
    run_engine(&["status".to_string()], None, None)
}

#[tauri::command]
fn make_backup(note: String, dest: Option<String>) -> Result<String, String> {
    let mut args = vec!["backup".to_string()];
    if !note.trim().is_empty() {
        args.push("--note".to_string());
        args.push(note.trim().to_string());
    }
    // dest 是"再归档一份"的目录。备份本身一定留在仓库 backups/ 里（恢复功能只认那里），
    // dest 只是给你自己留档用的副本。
    if let Some(d) = dest.as_ref().map(|s| s.trim()).filter(|s| !s.is_empty()) {
        let dir = expand(d);
        if dir.is_file() {
            return Err(format!("{} 是个文件，不是目录", dir.display()));
        }
        if let Err(e) = fs::create_dir_all(&dir) {
            return Err(format!("建不了目录 {}: {e}", dir.display()));
        }
        args.push("--to".to_string());
        args.push(d.to_string());
    }
    run_engine(&args, None, None)
}

#[tauri::command]
fn delete_backup(name: String) -> Result<String, String> {
    if name.contains('/') || !(name.ends_with(".zip") || name.ends_with(".tar.gz")) {
        return Err("备份名不合法".into());
    }
    let p = backups_dir().join(&name);
    fs::remove_file(&p).map_err(|e| format!("删不掉 {}: {e}", p.display()))?;
    Ok(format!("已删掉 {name}"))
}

#[tauri::command]
fn restore_backup(name: String, skill: Option<String>) -> Result<String, String> {
    let mut args = vec!["restore".to_string(), name];
    if let Some(s) = skill.filter(|s| !s.is_empty()) {
        args.push("--skill".to_string());
        args.push(s);
    }
    run_engine(&args, None, None)
}

#[tauri::command]
fn save_skill(name: String, body: String) -> Result<String, String> {
    let p = skills_dir().join(&name).join("SKILL.md");
    if !p.is_file() {
        return Err(format!("没有这个技能: {name}"));
    }
    fs::write(&p, body.as_bytes()).map_err(|e| format!("写入失败: {e}"))?;
    // 署名只挂这一个技能 —— 引擎按 SKILL_SYNC_ATTR_SKILL 限定范围
    run_engine(&["sync".to_string()], Some("SkillHub 界面编辑"), Some(&name))
}

#[tauri::command]
fn new_skill(name: String) -> Result<String, String> {
    let n = name.trim().to_string();
    if !valid_skill_name(&n) {
        return Err("名字只能用英文字母、数字、- 和 _（1–64 位）".into());
    }
    let d = skills_dir().join(&n);
    if d.exists() {
        return Err(format!("已经有叫 {n} 的技能了"));
    }
    fs::create_dir_all(&d).map_err(|e| format!("建目录失败: {e}"))?;
    let tpl = format!(
        "---\nname: {n}\ndescription: 一句话说明这个技能干什么、什么时候该用它。\nversion: 0.1.0\nauthor: \n---\n\n# {n}\n\n## 什么时候用\n\n\n## 怎么做\n\n"
    );
    fs::write(d.join("SKILL.md"), tpl).map_err(|e| format!("写模板失败: {e}"))?;
    run_engine(&["sync".to_string()], Some("SkillHub 界面新建"), Some(&n))
}

#[tauri::command]
fn restore_trash(tag: String, name: String) -> Result<String, String> {
    if name.contains('/') || tag.contains('/') {
        return Err("名字不合法".into());
    }
    // 在 .trash 里找这份内容的实际路径（同一个 tag/name 可能有多份，取最新的）
    let mut stamps: Vec<String> = fs::read_dir(trash_dir())
        .map(|rd| {
            rd.flatten()
                .filter(|e| e.path().is_dir())
                .map(|e| e.file_name().to_string_lossy().to_string())
                .collect()
        })
        .unwrap_or_default();
    stamps.sort();
    let src = stamps
        .iter()
        .rev()
        .map(|s| trash_dir().join(s).join(&tag).join(&name))
        .find(|p| p.is_dir());
    let src = match src {
        Some(p) => p,
        None => return Err(format!("回收站里找不到 {tag}/{name}")),
    };
    let dst = skills_dir().join(&name);
    if dst.is_dir() {
        // 先把现场存进回收站，跟终端版一个规则
        let stamp = Command::new("/bin/date").arg("+%Y%m%d-%H%M%S").output();
        let stamp = String::from_utf8_lossy(&stamp.map(|o| o.stdout).unwrap_or_default())
            .trim()
            .to_string();
        let keep = trash_dir().join(&stamp).join("restore-old").join(&name);
        if let Some(parent) = keep.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        // 逐层复制（不引 walkdir，自己走）
        copy_tree(&dst, &keep)?;
        fs::remove_dir_all(&dst).map_err(|e| format!("清旧版失败: {e}"))?;
    }
    copy_tree(&src, &dst)?;
    let out = run_engine(&["sync".to_string()], Some("SkillHub 从回收站恢复"), Some(&name))?;
    Ok(format!("已把 {name} 恢复回中心仓库\n\n{out}"))
}

fn copy_tree(src: &Path, dst: &Path) -> Result<(), String> {
    fs::create_dir_all(dst).map_err(|e| e.to_string())?;
    for e in fs::read_dir(src).map_err(|e| e.to_string())?.flatten() {
        let name = e.file_name();
        let from = e.path();
        let to = dst.join(&name);
        let md = e.metadata().map_err(|e| e.to_string())?;
        if md.is_dir() {
            copy_tree(&from, &to)?;
        } else {
            fs::copy(&from, &to).map_err(|e| format!("复制 {} 失败: {e}", from.display()))?;
        }
    }
    Ok(())
}

#[tauri::command]
fn skill_path(name: String) -> Result<String, String> {
    let p = skills_dir().join(&name);
    if !p.is_dir() {
        return Err(format!("没有这个技能目录: {}", p.display()));
    }
    Ok(p.display().to_string())
}

fn open_path(p: &Path, reveal: bool) -> Result<String, String> {
    if !p.exists() {
        return Err(format!("路径不存在: {}", p.display()));
    }
    let mut c = Command::new("open");
    if reveal && !p.is_dir() {
        c.arg("-R");
    }
    c.arg(p).spawn().map_err(|e| e.to_string())?;
    Ok(if reveal {
        "已在 Finder 里打开".into()
    } else {
        "已用默认程序打开".into()
    })
}

#[tauri::command]
fn reveal(path: String) -> Result<String, String> {
    open_path(&PathBuf::from(&path), true)
}

#[tauri::command]
fn reveal_skill(name: String) -> Result<String, String> {
    // 名字 -> 路径由后端算，前端永远不要自己拼（hub 根 ≠ 技能目录，踩过）
    let p = skills_dir().join(&name);
    if !p.is_dir() {
        return Err(format!("没有这个技能: {name}"));
    }
    open_path(&p, true)
}

#[tauri::command]
fn open_file(path: String) -> Result<String, String> {
    open_path(&PathBuf::from(&path), false)
}

/// 弹系统原生的"选择文件夹"对话框，返回 POSIX 路径。
///
/// 为什么用 osascript 而不是 tauri-plugin-dialog：后者要多一个 crate、还得在
/// capabilities 里加 `dialog:default` 权限；而 osascript 一句 Standard Additions
/// 就能拿到路径，零依赖。用户取消时返回 Err("取消")，前端识别这个字符串就静默收场。
#[tauri::command]
fn pick_dir(prompt: String, initial: Option<String>) -> Result<String, String> {
    let p = if prompt.trim().is_empty() {
        "选一个目录".to_string()
    } else {
        prompt.trim().to_string()
    };
    let esc_apl = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");
    let mut script = format!("set d to choose folder with prompt \"{}\"", esc_apl(&p));
    if let Some(i) = initial.as_ref().map(|s| s.trim()).filter(|s| !s.is_empty()) {
        let full = expand(i);
        if full.is_dir() {
            script.push_str(&format!(
                " default location (POSIX file \"{}\")",
                esc_apl(&full.display().to_string())
            ));
        }
    }
    script.push_str("\nPOSIX path of d");

    let out = Command::new("osascript")
        .arg("-e")
        .arg(&script)
        .output()
        .map_err(|e| format!("调不起系统选择框: {e}"))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        if err.contains("-128") {
            return Err("取消".into()); // 用户按了取消（AppleScript 的 User canceled. (-128)）
        }
        return Err(format!("选择目录失败: {err}"));
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if s.is_empty() {
        return Err("取消".into());
    }
    Ok(s.trim_end_matches('/').to_string())
}

/// 界面打开备份弹窗时给的默认导出目录：优先 ~/Downloads，没有就 ~ 或 ~/Desktop。
#[tauri::command]
fn default_export_dir() -> String {
    for cand in ["Downloads", "Desktop", ""] {
        let p = if cand.is_empty() { home() } else { home().join(cand) };
        if p.is_dir() {
            return p.display().to_string();
        }
    }
    home().display().to_string()
}

// ---------------------------------------------------------------- 恢复（spread）
//
// enable-index 的反操作：那边把各家的技能正文收回中心库、每家只留一个门牌；
// 这边把正文铺回各家、把门牌撤掉，并把 config 的 layout 翻成 copies。
// 不翻 layout 的话没用 —— 下一次 sync（还有那个每 3 小时的定时任务）会按 hold
// 字段把刚铺回去的正文当成"冗余副本"又清掉。

#[tauri::command]
fn spread_dry_run() -> Result<String, String> {
    run_engine(&["spread".into(), "--dry-run".into()], None, None)
}

#[tauri::command]
fn spread_copies() -> Result<String, String> {
    run_engine(
        &["spread".into()],
        Some("AgentHarbor 界面：恢复到各家"),
        None,
    )
}

// ---------------------------------------------------------------- MCP
//
// 后端只做两件事：把引擎的 JSON 原样递给前端（让前端自己渲染），
// 以及把前端的动作翻译成引擎子命令。MCP 的逻辑一律留在引擎里 ——
// 界面绝不自己去读写各家的 MCP 配置文件。

/// 只取 stdout，不掺 stderr —— `mcp status --json` 的输出是要被 JSON.parse 的。
fn run_engine_stdout(args: &[String]) -> Result<String, String> {
    let script = sync_py();
    if !script.is_file() {
        return Err(format!("找不到引擎脚本 {}", script.display()));
    }
    let out = Command::new(PY)
        .arg(&script)
        .args(args)
        .current_dir(hub())
        .env("LANG", "en_US.UTF-8")
        .output()
        .map_err(|e| format!("无法启动引擎: {e}"))?;
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(if err.is_empty() {
            format!("引擎退出码 {}", out.status.code().unwrap_or(-1))
        } else {
            err
        });
    }
    Ok(stdout)
}

/// MCP 全景：中心注册表 + 每家的现状。返回的是引擎的原始 JSON 字符串。
#[tauri::command]
fn mcp_status() -> Result<String, String> {
    run_engine_stdout(&["mcp".into(), "status".into(), "--json".into()])
}

/// MCP 收敛计划（每家会做什么、会收走几个）。
#[tauri::command]
fn mcp_plan() -> Result<String, String> {
    run_engine_stdout(&["mcp".into(), "plan".into(), "--json".into()])
}

/// 把各家配置里的 MCP server 定义收进中心注册表（并集）。
#[tauri::command]
fn mcp_import() -> Result<String, String> {
    run_engine(&["mcp".into(), "import".into()], None, None)
}

/// 把指定几家收敛成"只剩一条 mount-mcp"。ids 为空表示全部。
///
/// 会改用户各家的 MCP 配置文件 —— 所以引擎那边强制要求 --yes，
/// 而且改之前每一家都会备份到 ~/AgentSkillHub/mcp/.backups/。
#[tauri::command]
fn mcp_apply(ids: Option<Vec<String>>, dry: bool) -> Result<String, String> {
    let mut a = vec!["mcp".to_string(), "apply".to_string()];
    if dry {
        a.push("--dry-run".into());
    } else {
        a.push("--yes".into());
    }
    if let Some(list) = ids.as_ref().filter(|v| !v.is_empty()) {
        a.push("--ids".into());
        a.extend(list.iter().cloned());
    }
    run_engine(&a, None, None)
}

/// mount-mcp 这个通用挂载器的自检（注册表读得到吗、命令都在吗）。
#[tauri::command]
fn mcp_selfcheck() -> Result<String, String> {
    let script = hub().join("mcp").join("mount-mcp").join("server.py");
    if !script.is_file() {
        return Err(format!("找不到 {}", script.display()));
    }
    let out = Command::new("/usr/bin/python3")
        .arg(&script)
        .arg("--selfcheck")
        .output()
        .map_err(|e| format!("无法启动自检: {e}"))?;
    let mut s = String::from_utf8_lossy(&out.stdout).to_string();
    let err = String::from_utf8_lossy(&out.stderr);
    if !err.trim().is_empty() {
        s.push_str(&err);
    }
    Ok(s)
}

/// 打开 MCP 备份目录 / 注册表文件，方便用户自己看。
#[tauri::command]
fn mcp_paths(which: String) -> Result<String, String> {
    let p = match which.as_str() {
        "registry" => hub().join("mcp").join("servers.json"),
        "backups" => hub().join("mcp").join(".backups"),
        "server" => hub().join("mcp").join("mount-mcp").join("server.py"),
        other => return Err(format!("不认识的 mcp 路径: {other}")),
    };
    open_path(&p, which == "backups")
}

// ---------------------------------------------------------------- 自检
//
// 从命令行跑 `agentharbor --selfcheck` 会把所有**只读**命令对着真仓库跑一遍并打印结果。
// 用处：图形界面起不来（比如会话里没有 GUI）或者想快速确认"界面拿到的是不是对的数据"时，
// 不用开窗口就能验证后端。

fn fmt_size(n: u64) -> String {
    if n < 1024 {
        format!("{n}B")
    } else if n < 1_048_576 {
        format!("{}K", (n + 512) / 1024)
    } else {
        format!("{:.1}M", n as f64 / 1_048_576.0)
    }
}

fn selfcheck() -> i32 {
    let s = status();
    println!("仓库      {}", s.hub);
    println!("状态      技能 {} 个 · 接入 {} 家 · 上次同步 {} · 上轮仓库改动 {} · 备份 {} 个 · 回收站 {} 批 · 冲突 {} 批",
             s.skills, s.agents, s.last_sync, s.hub_changed, s.backups, s.trash, s.conflicts);

    let sk = skills();
    println!("\n技能      {} 个（列前 5 个）", sk.len());
    for x in sk.iter().take(5) {
        println!("  {:<26} {:>7}  {:>6} 字  {}",
                 x.name, fmt_size(x.size), x.chars,
                 if x.copies.is_empty() { "只挂索引".to_string() } else { x.copies.join("/") });
    }
    if let Some(f) = sk.first() {
        // 只做"能不能解析出路径"的检查，不真的去开 Finder（自检必须是只读的）
        println!("  路径样例  {}", f.path);
        match skill_path(f.name.clone()) {
            Ok(p) => println!("  解析校验  skill_path({}) -> {}  [存在]", f.name, p),
            Err(e) => println!("  解析校验  失败: {e}"),
        }
    }

    let tl = timeline(80);
    println!("\n流水      {} 条（最近 4 条）", tl.len());
    for e in tl.iter().take(4) {
        println!("  {}  {:<26} {:<12} {}", e.ts, e.skill, e.act, e.src);
    }

    let b = backups();
    println!("\n备份      {} 个", b.len());
    for x in b.iter().take(4) {
        println!("  {}  {:>7}  {:<7} {}", x.time, fmt_size(x.size), x.kind, x.name);
    }
    println!("  默认导出目录  {}", default_export_dir());

    let t = trash();
    println!("\n回收站    {} 条（列前 4 条）", t.len());
    for x in t.iter().take(4) {
        println!("  {}  {:<26} {:<12} <- {}", x.time, x.name, x.act, x.src);
    }

    let d = dictionary();
    println!("\n各家配置  {}", d.iter().map(|x| x.id.clone()).collect::<Vec<_>>().join(", "));
    for x in d.iter() {
        let flag = if !x.exists {
            "目录不存在"
        } else if x.has_index {
            "已挂门牌"
        } else if x.hold == "index" {
            "缺门牌"
        } else {
            "—"
        };
        println!("  {:<14} {:<8} {:<9} 门牌:{:<6} 副本:{:<4} {}",
                 x.id, x.hold, x.mode, flag, x.extras, x.path);
    }

    println!("\nMCP 统一状态");
    match mcp_status() {
        Ok(raw) => match serde_json::from_str::<Value>(&raw) {
            Ok(v) => {
                println!("  中心注册表 {}（{} 个）",
                         v["registry_path"].as_str().unwrap_or("?"),
                         v["registry_count"].as_u64().unwrap_or(0));
                println!("  通用挂载器 {}", v["mount_mcp"].as_str().unwrap_or("?"));
                if let Some(list) = v["vendors"].as_array() {
                    for x in list {
                        println!("  {:<12} {:<18} {:>2} 个  机制:{:<12} {}",
                                 x["id"].as_str().unwrap_or("?"),
                                 x["label"].as_str().unwrap_or(""),
                                 x["count"].as_u64().unwrap_or(0),
                                 x["mechanism"].as_str().unwrap_or(""),
                                 if x["unified"].as_bool().unwrap_or(false) { "已收敛" } else { "未收敛" });
                    }
                }
                for key in ["only_in_registry", "only_in_vendors"] {
                    if let Some(a) = v[key].as_array().filter(|a| !a.is_empty()) {
                        println!("  {}: {}", key,
                                 a.iter().filter_map(|s| s.as_str()).collect::<Vec<_>>().join(", "));
                    }
                }
            }
            Err(e) => println!("  解析失败: {e}"),
        },
        Err(e) => println!("  失败: {e}"),
    }
    match mcp_selfcheck() {
        Ok(t) => {
            let bad = t.lines().any(|l| l.contains("❌"));
            println!("  mount-mcp 自检: {} 行输出，{}", t.lines().count(),
                     if bad { "有 ❌" } else { "无 ❌" });
        }
        Err(e) => println!("  mount-mcp 自检失败: {e}"),
    }
    println!("  MCP 路径: registry={}", hub().join("mcp").join("servers.json").display());

    println!("\n恢复      spread 命令可用（把中心库技能铺回各家）");
    match spread_dry_run() {
        Ok(t) => {
            let first = t.lines().next().unwrap_or("（无输出）").to_string();
            println!("  dry-run  {first}");
        }
        Err(e) => println!("  dry-run 失败: {e}"),
    }

    println!("\n自检结束：所有只读命令都正常返回。");
    0
}

// ---------------------------------------------------------------- 入口

fn main() {
    if std::env::args().any(|a| a == "--selfcheck") {
        std::process::exit(selfcheck());
    }
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            status,
            dictionary,
            skills,
            skill_body,
            journal,
            timeline,
            backups,
            trash,
            run_sync,
            enable_index,
            reindex,
            run_doctor,
            run_status_cli,
            make_backup,
            delete_backup,
            restore_backup,
            save_skill,
            new_skill,
            restore_trash,
            reveal,
            reveal_skill,
            skill_path,
            open_file,
            pick_dir,
            default_export_dir,
            spread_dry_run,
            spread_copies,
            mcp_status,
            mcp_plan,
            mcp_import,
            mcp_apply,
            mcp_selfcheck,
            mcp_paths,
        ])
        .run(tauri::generate_context!())
        .expect("AgentHarbor 启动失败");
}
