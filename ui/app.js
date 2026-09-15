const T = window.__TAURI__.core;

// ---------------------------------------------------------------- 基础
const $ = (s) => document.querySelector(s);
const esc = (s) =>
  String(s == null ? "" : s).replace(/[&<>"']/g, (c) =>
    ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c]));

function fmtSize(n) {
  if (n == null) return "—";
  if (n < 1024) return n + "B";
  if (n < 1048576) return Math.round(n / 1024) + "K";
  return (n / 1048576).toFixed(1) + "M";
}

let busyN = 0;
function busy(on) {
  busyN += on ? 1 : -1;
  if (busyN < 0) busyN = 0;
  const bar = $("#busybar");
  if (busyN > 0 && !bar) {
    const d = document.createElement("div");
    d.className = "busy";
    d.id = "busybar";
    $("#app").appendChild(d);
  } else if (busyN === 0 && bar) {
    bar.remove();
  }
}

function toast(msg, kind) {
  const d = document.createElement("div");
  d.className = "toast" + (kind ? " " + kind : "");
  d.textContent = msg;
  $("#toast").appendChild(d);
  setTimeout(() => d.remove(), kind === "err" ? 5200 : 2600);
}

async function inv(cmd, args) {
  busy(true);
  try {
    return await T.invoke(cmd, args || {});
  } finally {
    busy(false);
  }
}

async function run(cmd, args, label) {
  try {
    const out = await inv(cmd, args);
    consoleModal(label, out);
    await refresh();
    return out;
  } catch (e) {
    toast(String(e), "err");
    return null;
  }
}

// ---------------------------------------------------------------- 弹层
let sheetButtons = [];

function openSheet(opts) {
  $("#modalTitle").textContent = opts.title || "";
  $("#modalBody").innerHTML = opts.html || "";
  $("#sheet").classList.toggle("small", !!opts.small);
  const f = $("#modalFooter");
  f.innerHTML = "";
  sheetButtons = opts.buttons || [];
  sheetButtons.forEach((b, i) => {
    const el = document.createElement("button");
    el.textContent = b.label;
    if (b.kind) el.className = b.kind;
    el.onclick = () => b.onClick && b.onClick();
    f.appendChild(el);
  });
  $("#modal").classList.remove("hidden");
  if (opts.focus) setTimeout(() => $(opts.focus) && $(opts.focus).focus(), 30);
}

function closeSheet() {
  $("#modal").classList.add("hidden");
  sheetButtons = [];
}

function consoleModal(title, text) {
  openSheet({
    title,
    html: `<pre>${esc(text || "（没有输出）")}</pre>`,
    buttons: [{ label: "关闭", kind: "primary", onClick: closeSheet }],
  });
}

function confirmModal(title, text, onYes) {
  openSheet({
    title,
    small: true,
    html: `<p class="meta">${esc(text)}</p>`,
    buttons: [
      { label: "取消", onClick: closeSheet },
      {
        label: "确认",
        kind: "primary",
        onClick: () => {
          closeSheet();
          onYes();
        },
      },
    ],
  });
}

function promptModal(title, label, placeholder, onOk) {
  openSheet({
    title,
    small: true,
    html: `<p class="meta">${esc(label)}</p><input type="text" id="pmInput" placeholder="${esc(
      placeholder || ""
    )}" />`,
    focus: "#pmInput",
    buttons: [
      { label: "取消", onClick: closeSheet },
      {
        label: "确定",
        kind: "primary",
        onClick: () => {
          const v = $("#pmInput").value.trim();
          closeSheet();
          onOk(v);
        },
      },
    ],
  });
  setTimeout(() => {
    const el = $("#pmInput");
    if (!el) return;
    el.addEventListener("keydown", (e) => {
      if (e.key === "Enter") {
        const v = el.value.trim();
        closeSheet();
        onOk(v);
      }
    });
  }, 40);
}

// ---------------------------------------------------------------- 状态
let STATE = { status: null, skills: [], timeline: [], backups: [], trash: [], agents: [], sel: null };

function chips(s) {
  const out = [];
  out.push(`<span class="chip">技能 <b>${s.skills}</b></span>`);
  out.push(`<span class="chip">接入 <b>${s.agents}</b> 家</span>`);
  out.push(`<span class="chip${s.doctor === "OK" ? " ok" : " warn"}">中心仓库 ${esc(s.doctor)}</span>`);
  out.push(`<span class="chip">上次同步 <b>${esc(s.last_sync)}</b></span>`);
  out.push(
    `<span class="chip${s.hub_changed ? " warn" : ""}">上轮仓库改动 <b>${s.hub_changed}</b></span>`
  );
  out.push(`<span class="chip">备份 <b>${s.backups}</b></span>`);
  out.push(`<span class="chip">回收站 <b>${s.trash}</b> 批</span>`);
  if (s.conflicts) out.push(`<span class="chip warn">冲突归档 <b>${s.conflicts}</b></span>`);
  $("#chips").innerHTML = out.join("");
}

async function refresh() {
  try {
    STATE.status = await inv("status");
    chips(STATE.status);
    $("#hubPath").textContent = STATE.status.skills_dir;
    const page = $("nav button.on").dataset.page;
    if (page === "skills") await loadSkills();
    if (page === "timeline") await loadTimeline();
    if (page === "backups") await loadBackups();
    if (page === "trash") await loadTrash();
    if (page === "agents") await loadAgents();
  } catch (e) {
    toast("读取失败：" + e, "err");
  }
}

// ---------------------------------------------------------------- 页 1 技能
async function loadSkills() {
  STATE.skills = await inv("skills");
  renderSkillList();
  if (STATE.sel && STATE.skills.some((s) => s.name === STATE.sel)) showSkill(STATE.sel);
  else {
    STATE.sel = null;
    $("#skillDetail").innerHTML = '<p class="empty">左边选一个技能。</p>';
  }
}

function renderSkillList() {
  const q = $("#skillSearch").value.trim().toLowerCase();
  const rows = STATE.skills.filter(
    (s) => !q || s.name.toLowerCase().includes(q) || (s.desc || "").toLowerCase().includes(q)
  );
  $("#skillList").innerHTML = rows
    .map(
      (s) => `<li data-name="${esc(s.name)}" class="${s.name === STATE.sel ? "on" : ""}">
        ${s.copies.length ? '<span class="dot" title="' + esc(s.copies.join(", ")) + '还留着完整副本"></span>' : ""}
        <span class="nm">${esc(s.name)}</span>
        <span class="meta" style="margin:0">${fmtSize(s.size)}</span>
      </li>`
    )
    .join("");
  [...$("#skillList").children].forEach((li) => {
    li.onclick = () => showSkill(li.dataset.name);
  });
}

async function showSkill(name) {
  STATE.sel = name;
  renderSkillList();
  const s = STATE.skills.find((x) => x.name === name);
  if (!s) return;
  let body = "";
  try {
    body = await inv("skill_body", { name });
  } catch (e) {
    body = "（读不出来：" + e + "）";
  }
  const copies = s.copies.length
    ? s.copies.join("、") + " 还留着完整副本"
    : "只存在于中心仓库（各家只挂索引）";
  $("#skillDetail").innerHTML = `
    <h2 class="panel">${esc(s.name)}</h2>
    <p class="meta">${esc(s.desc)}</p>
    <p class="meta"><b>体积</b> ${fmtSize(s.size)} · ${s.files} 个文件 · 正文 ${s.chars} 字</p>
    <p class="meta"><b>改动</b> ${ts(s.mtime)}</p>
    <p class="meta"><b>副本</b> ${esc(copies)}</p>
    <p class="meta"><b>路径</b> <span class="mono">${esc(s.path)}</span></p>
    <div class="actions">
      <button id="skEdit" class="primary">编辑</button>
      <button id="skNew">新建技能</button>
      <button id="skFinder">在 Finder 里打开</button>
    </div>
    <pre>${esc(body)}</pre>`;
  $("#skEdit").onclick = () => editSkill(name, body);
  $("#skNew").onclick = newSkill;
  // 只传技能名，路径由后端算 —— 前端不碰路径拼接
  $("#skFinder").onclick = () =>
    inv("reveal_skill", { name }).catch((e) => toast(String(e), "err"));
}

function ts(sec) {
  if (!sec) return "—";
  const d = new Date(sec * 1000);
  const p = (n) => String(n).padStart(2, "0");
  return `${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}`;
}

function editSkill(name, body) {
  openSheet({
    title: "编辑 " + name + "/SKILL.md",
    html: `<textarea id="edTa" spellcheck="false">${esc(body)}</textarea>`,
    focus: "#edTa",
    buttons: [
      { label: "取消", onClick: closeSheet },
      {
        label: "保存并同步",
        kind: "primary",
        onClick: async () => {
          const v = $("#edTa").value;
          closeSheet();
          await run("save_skill", { name, body: v }, "保存 " + name + " 并同步");
        },
      },
    ],
  });
}

function newSkill() {
  promptModal("新建技能", "名字（英文字母 / 数字 / - / _）", "my-new-skill", async (v) => {
    if (!v) return;
    const out = await run("new_skill", { name: v }, "新建 " + v);
    if (out) {
      try {
        const b = await inv("skill_body", { name: v });
        editSkill(v, b);
      } catch (e) {
        /* 忽略 */
      }
    }
  });
}

// ---------------------------------------------------------------- 页 2 流水
async function loadTimeline() {
  STATE.timeline = await inv("timeline", { limit: 400 });
  const body = $("#timelineBody");
  body.innerHTML = STATE.timeline
    .map((e) => {
      const cls = e.kind === "git" ? "git" : /删除/.test(e.act) ? "del" : /新增|收编/.test(e.act) ? "add" : "mod";
      const srcTxt = e.kind === "git" ? e.src : `${e.src} → ${e.dst}`;
      return `<tr>
        <td class="mono nw">${esc(e.ts)}</td>
        <td class="ell">${esc(e.skill)}</td>
        <td class="nw"><span class="tag ${cls}">${esc(e.act)}</span></td>
        <td class="ell">${esc(srcTxt)}</td>
      </tr>`;
    })
    .join("");
  $("#timelineEmpty").hidden = STATE.timeline.length > 0;
}

// ---------------------------------------------------------------- 页 3 备份
async function loadBackups() {
  STATE.backups = await inv("backups");
  const total = STATE.backups.reduce((a, b) => a + b.size, 0);
  const nz = STATE.backups.filter((b) => b.kind === "zip").length;
  $("#backupHint").textContent =
    `共 ${STATE.backups.length} 个（zip ${nz} · 旧 tar.gz ${STATE.backups.length - nz}），合计 ${fmtSize(total)}` +
    `　·　点「备份并归档」会打一个 zip，并让你选一个目录再存一份` +
    `　·　恢复是"整体回退到那个时间点"，回退前的现场会进回收站`;
  $("#backupBody").innerHTML = STATE.backups
    .map(
      (b) => `<tr>
      <td class="mono ell" title="${esc(b.name)}">${esc(b.name)}${b.note ? ` <span class="tag">${esc(b.note)}</span>` : ""}</td>
      <td class="nw"><span class="tag${b.kind === "zip" ? " mod" : ""}">${esc(b.kind)}</span></td>
      <td class="nw mono">${fmtSize(b.size)}</td>
      <td class="nw mono">${esc(b.time)}</td>
      <td class="nw"><span class="act">
        <button data-act="restore" data-name="${esc(b.name)}">恢复全部</button>
        <button data-act="restore1" data-name="${esc(b.name)}">只恢复一个…</button>
        <button data-act="finder" data-name="${esc(b.name)}">在 Finder 里看</button>
        <button data-act="del" data-name="${esc(b.name)}" class="danger">删除</button>
      </span></td>
    </tr>`
    )
    .join("");
  $("#backupEmpty").hidden = STATE.backups.length > 0;
  [...$("#backupBody").querySelectorAll("button")].forEach((btn) => {
    const name = btn.dataset.name;
    const rec = STATE.backups.find((x) => x.name === name);
    if (btn.dataset.act === "restore") {
      btn.onclick = () =>
        confirmModal("恢复整个中心仓库", `会把 skills/ index/ config.json 整体回退到「${name}」的状态。回退前的现场会进回收站，可再捞回来。`, () =>
          run("restore_backup", { name, skill: null }, "从 " + name + " 恢复")
        );
    } else if (btn.dataset.act === "restore1") {
      btn.onclick = () =>
        promptModal("只恢复一个技能", "要恢复哪个技能？（填技能目录名）", "docx", (v) => {
          if (!v) return;
          confirmModal("只恢复 " + v, `拿「${name}」里那个版本的 ${v} 覆盖中心仓库现在的 ${v}。`, () =>
            run("restore_backup", { name, skill: v }, `从 ${name} 恢复 ${v}`)
          );
        });
    } else if (btn.dataset.act === "finder") {
      // 路径由后端 records 给，前端不拼
      btn.onclick = () =>
        inv("reveal", { path: rec ? rec.path : "" }).catch((e) => toast(String(e), "err"));
    } else {
      btn.onclick = () =>
        confirmModal("删掉备份", `删除 ${name}（不可恢复）。`, () =>
          inv("delete_backup", { name })
            .then(async (m) => {
              toast(m, "ok");
              await refresh();
            })
            .catch((e) => toast(String(e), "err"))
        );
    }
  });
}

// ---------------------------------------------------------------- 页 4 回收站
async function loadTrash() {
  STATE.trash = await inv("trash");
  $("#trashHint").textContent = `共 ${STATE.trash.length} 条旧版本，都能捞回中心仓库`;
  renderTrash();
}

function renderTrash() {
  const q = $("#trashSearch").value.trim().toLowerCase();
  const rows = STATE.trash.filter(
    (t) => !q || t.name.toLowerCase().includes(q) || t.tag.toLowerCase().includes(q)
  );
  $("#trashBody").innerHTML = rows
    .slice(0, 600)
    .map(
      (t, i) => `<tr>
      <td class="mono nw">${esc(t.time)}</td>
      <td class="ell" title="${esc(t.name)}">${esc(t.name)}</td>
      <td class="nw">${esc(t.act)}</td>
      <td class="nw mono">${esc(t.src)}</td>
      <td class="nw mono">${t.is_skill ? fmtSize(t.size) : "—"}</td>
      <td class="nw"><span class="act">
        ${t.is_skill ? `<button data-i="${i}" data-act="back">恢复</button>` : ""}
        <button data-i="${i}" data-act="show">在 Finder 里看</button>
      </span></td>
    </tr>`
    )
    .join("");
  $("#trashEmpty").hidden = rows.length > 0;
  [...$("#trashBody").querySelectorAll("button")].forEach((btn) => {
    const t = rows[Number(btn.dataset.i)];
    if (btn.dataset.act === "back") {
      btn.onclick = () =>
        confirmModal(
          "恢复到中心仓库",
          `把 ${t.name}（${t.act}，来自 ${t.src}）的内容恢复回中心仓库。如果现有版本会被覆盖，旧版会先进回收站。`,
          () => run("restore_trash", { tag: t.tag, name: t.name }, "恢复 " + t.name)
        );
    } else {
      btn.onclick = () =>
        inv("reveal", { path: t.path }).catch((e) => toast(String(e), "err"));
    }
  });
}

// ---------------------------------------------------------------- 页 5 各家配置
async function loadAgents() {
  STATE.agents = await inv("dictionary");
  const idx = STATE.agents.filter((a) => a.hold === "index");
  const live = idx.filter((a) => a.exists);
  const missing = live.filter((a) => !a.has_index).length;
  const dirty = live.filter((a) => a.extras > 0).length;
  $("#agentHint").textContent =
    `索引模式接入方 ${live.length} 家　·　` +
    (missing ? `${missing} 家还没挂门牌` : "门牌都齐了") +
    "　·　" +
    (dirty ? `${dirty} 家还残留正文副本` : "没有残留副本");
  $("#agentBody").innerHTML = STATE.agents
    .map((a) => {
      const badge = !a.exists
        ? '<span class="tag">目录不存在</span>'
        : a.has_index
        ? '<span class="tag add">已挂</span>'
        : a.hold === "index"
        ? '<span class="tag del">缺门牌</span>'
        : '<span class="tag">不适用</span>';
      const extra = !a.exists
        ? "—"
        : a.extras === 0
        ? '<span class="tag add">0</span>'
        : a.hold === "copies"
        ? `<span class="tag">${a.extras}</span>`
        : `<span class="tag del">${a.extras}</span>`;
      return `<tr>
      <td class="nw mono">${esc(a.id)}</td>
      <td class="nw"><span class="tag">${esc(a.hold)}</span> <span class="tag">${esc(a.mode)}</span></td>
      <td class="nw">${badge}</td>
      <td class="nw">${extra}</td>
      <td class="mono ell" title="${esc(a.path)}">${esc(a.path)}</td>
    </tr>`;
    })
    .join("");
}

// ---------------------------------------------------------------- 事件
document.querySelectorAll("nav button").forEach((b) => {
  b.onclick = async () => {
    document.querySelectorAll("nav button").forEach((x) => x.classList.remove("on"));
    b.classList.add("on");
    document.querySelectorAll(".page").forEach((p) => p.classList.remove("on"));
    $("#page-" + b.dataset.page).classList.add("on");
    const p = b.dataset.page;
    if (p === "skills") await loadSkills();
    if (p === "timeline") await loadTimeline();
    if (p === "backups") await loadBackups();
    if (p === "trash") await loadTrash();
    if (p === "agents") await loadAgents();
  };
});

$("#skillSearch").oninput = renderSkillList;
$("#trashSearch").oninput = renderTrash;
$("#btnRefresh").onclick = () => refresh();
$("#btnSync").onclick = () => run("run_sync", {}, "同步到各家");
$("#btnDoctor").onclick = () => run("run_doctor", {}, "体检");
$("#btnBackup").onclick = askBackup;
$("#btnBackup2").onclick = askBackup;
$("#btnEnableIndex").onclick = enableIndex;
$("#btnEnableIndex2").onclick = enableIndex;
$("#btnReindex").onclick = () => run("reindex", {}, "重建 skills-index 门牌清单");

function enableIndex() {
  confirmModal(
    "一键索引模式",
    "会把所有被检测到的接入方目录里多出来的技能收编进中心仓库、正文副本清进回收站（随时可捞回），" +
      "然后在每家只留一个 skills-index 门牌。门牌清单是按中心仓库现有技能当场重写的，不是写死的。" +
      "已经干净的目录不会有任何变化。",
    () => run("enable_index", {}, "一键索引模式")
  );
}

async function askBackup() {
  let def = "";
  try {
    def = await inv("default_export_dir");
  } catch (e) {
    /* 拿不到就让用户自己填，不拦着 */
  }
  openSheet({
    title: "备份并归档",
    small: true,
    html: `
      <p class="meta">先给中心仓库打一个 <b>.zip</b> 快照（留在仓库的 backups/ 里 —— 恢复功能只认那里），
      再把同一份包存一份到你选的目录。</p>
      <p class="meta" style="margin-top:12px"><b>备份说明</b>（可留空，会写进文件名）</p>
      <input type="text" id="bkNote" placeholder="改了什么之前的状态" />
      <p class="meta" style="margin-top:12px"><b>再归档一份到这个目录</b>（留空就只存在仓库里）</p>
      <div class="row">
        <input type="text" id="bkDest" class="mono" value="${esc(def)}" spellcheck="false" />
        <button id="bkBrowse">选择…</button>
        <button id="bkOpen">打开</button>
      </div>
      <p class="meta">回车 = 直接开始备份。</p>`,
    buttons: [
      { label: "取消", onClick: closeSheet },
      { label: "开始备份", kind: "primary", onClick: doBackup },
    ],
  });
  setTimeout(() => {
    const dest = $("#bkDest");
    const note = $("#bkNote");
    if (dest) dest.focus();
    const browse = $("#bkBrowse");
    if (browse)
      browse.onclick = async () => {
        try {
          const p = await inv("pick_dir", { prompt: "选一个目录来归档备份", initial: dest.value });
          dest.value = p;
        } catch (e) {
          if (String(e) !== "取消") toast(String(e), "err");
        }
      };
    const ox = $("#bkOpen");
    if (ox)
      ox.onclick = () =>
        inv("reveal", { path: dest.value }).catch((e) => toast(String(e), "err"));
    [dest, note].forEach((el2) => {
      if (!el2) return;
      el2.addEventListener("keydown", (e) => {
        if (e.key === "Enter") doBackup();
      });
    });
  }, 40);
}

function doBackup() {
  const val = (id) => ($(id) ? $(id).value : "");
  const note = val("#bkNote");
  const dest = val("#bkDest");
  closeSheet();
  return run("make_backup", { note, dest }, "备份并归档");
}

document.addEventListener("keydown", (e) => {
  if ((e.metaKey || e.ctrlKey) && e.key === "r") {
    e.preventDefault();
    refresh();
  }
  if ((e.metaKey || e.ctrlKey) && e.key === "n") {
    e.preventDefault();
    newSkill();
  }
  if (e.key === "Escape" && !$("#modal").classList.contains("hidden")) closeSheet();
});

refresh();
