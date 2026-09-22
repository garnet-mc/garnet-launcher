// Garnet launcher UI. Plain JavaScript on top of the Tauri commands in
// src/main.rs; no build step.

// Outside Tauri (opening index.html in a browser) a mock backend answers
// with sample data, which makes the UI easy to design and preview.
const T = window.__TAURI__ || mockTauri();
const invoke = T.core.invoke;
const listen = T.event.listen;
const win = T.window.getCurrentWindow();

function mockTauri() {
  const sample = {
    account: { name: "Steve", uuid: "8667ba71b85a4004af54457a9734eed7" },
    accounts: [{ name: "Steve", uuid: "8667ba71b85a4004af54457a9734eed7" }],
    settings: { client_id: "demo", default_memory_mb: 4096, jvm_args: [] },
    instances: [
      { name: "Survival with friends", minecraft: "26.3", loader: "fabric", server: "play.example.com:25565", memory_mb: 4096, last_played: new Date(Date.now() - 3600e3).toISOString(), mods: 14, dir: "" },
      { name: "Creative builds", minecraft: "26.3", loader: "vanilla", server: null, memory_mb: 4096, last_played: new Date(Date.now() - 86400e3 * 3).toISOString(), mods: 0, dir: "" },
      { name: "Shader testing", minecraft: "26.2", loader: "fabric", server: null, memory_mb: 8192, last_played: null, mods: 6, dir: "" },
    ],
    running: null, home: "C:/Users/you/AppData/Roaming/garnet", version: "0.1.0",
  };
  const handlers = {
    get_state: () => sample,
    list_versions: () => [{ id: "26.3", kind: "release", date: "2026-09-15" }, { id: "26.2", kind: "release", date: "2026-06-16" }],
    ping_server: ({ address }) => ({ host: address.split(":")[0], port: 25565, description: "A Garnet server — come build with us", online: 37, max: 100, version: "Garnet 26.3",
      garnet: { minecraft: "26.3", voice: true, enforce: true, required: [{ id: "sodium", name: "Sodium", version: "", source: "modrinth" }, { id: "lithium", name: "Lithium", version: "", source: "modrinth" }], optional: [{ id: "iris", name: "Iris Shaders", version: "", source: "modrinth" }], shader_pack: { id: "garnet-shaders", name: "Garnet Shaders" } } }),
    search_mods: ({ query }) => [
      { slug: "sodium", title: "Sodium", description: "A high-performance rendering engine replacement.", downloads: 229123936, icon_url: "" },
      { slug: "lithium", title: "Lithium", description: "No-compromises game logic and server optimization mod.", downloads: 120000000, icon_url: "" },
      { slug: "iris", title: "Iris Shaders", description: "A modern shaders mod for Minecraft.", downloads: 90000000, icon_url: "" },
    ].filter(h => !query || h.title.toLowerCase().includes(query.toLowerCase())),
    list_mods: () => [{ id: "sodium", version: "mc26.3-0.9.2", file: "sodium-fabric-0.9.2+mc26.3.jar", source: "modrinth" }, { id: "iris", version: "1.9.0", file: "iris-1.9.0.jar", source: "modrinth" }],
    login_start: () => ({ user_code: "ABCD-EFGH", verification_uri: "https://www.microsoft.com/link" }),
  };
  return {
    core: { invoke: async (name, args) => { await new Promise(r => setTimeout(r, 150)); return (handlers[name] || (() => null))(args || {}); } },
    event: { listen: async () => () => {} },
    window: { getCurrentWindow: () => ({ minimize() {}, toggleMaximize() {}, close() {} }) },
    opener: { openUrl: url => window.open(url, "_blank") },
  };
}

const page = document.getElementById("page");
let state = { account: null, accounts: [], settings: {}, instances: [], running: null };
let selectedInstance = null;

// ---------- helpers ----------

function esc(s) {
  return String(s ?? "").replace(/[&<>"']/g, c => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c]));
}

function toast(message, kind) {
  const el = document.createElement("div");
  el.className = "toast " + (kind || "");
  el.textContent = message;
  document.getElementById("toasts").appendChild(el);
  setTimeout(() => el.remove(), 4200);
}

async function call(name, args) {
  try {
    return await invoke(name, args || {});
  } catch (e) {
    toast(String(e), "err");
    throw e;
  }
}

// Deep jewel-tone covers, picked per instance name.
const COVERS = [
  ["#c8153f", "#3a0f2b"], ["#7a1c5c", "#1c0b26"], ["#b3402a", "#3b0f14"], ["#1f6f8b", "#0d1b2e"],
  ["#8a5a15", "#2b1607"], ["#2f7d5a", "#0b1f1a"], ["#5c3fbf", "#170c3a"], ["#a11a5a", "#2a0a1e"],
];
function coverColors(name) {
  let h = 0;
  for (const c of name) h = (h * 31 + c.charCodeAt(0)) >>> 0;
  return COVERS[h % COVERS.length];
}

function ago(iso) {
  if (!iso) return "never played";
  const s = (Date.now() - new Date(iso).getTime()) / 1000;
  if (s < 3600) return `${Math.max(1, Math.floor(s / 60))} min ago`;
  if (s < 86400) return `${Math.floor(s / 3600)} h ago`;
  return `${Math.floor(s / 86400)} d ago`;
}

function modal(html) {
  const root = document.getElementById("modal-root");
  root.innerHTML = `<div class="modal-bg"><div class="modal">${html}</div></div>`;
  root.querySelector(".modal-bg").addEventListener("mousedown", e => { if (e.target.classList.contains("modal-bg")) closeModal(); });
  return root;
}

function closeModal() {
  document.getElementById("modal-root").innerHTML = "";
}

async function refresh() {
  state = await invoke("get_state");
  renderAccount();
  document.getElementById("tb-status").textContent = state.running ? `playing ${state.running}` : "";
}

// ---------- title bar ----------

document.getElementById("tb-min").onclick = () => win.minimize();
document.getElementById("tb-max").onclick = () => win.toggleMaximize();
document.getElementById("tb-close").onclick = () => win.close();

// ---------- account ----------

function renderAccount() {
  const head = document.getElementById("account-head");
  const fallback = document.getElementById("account-fallback");
  if (state.account) {
    head.src = `https://mc-heads.net/avatar/${state.account.uuid}/68`;
    head.hidden = false;
    fallback.hidden = true;
    document.getElementById("account-name").textContent = state.account.name;
    document.getElementById("account-sub").textContent = "Microsoft account";
  } else {
    head.hidden = true;
    fallback.hidden = false;
    document.getElementById("account-name").textContent = "Not signed in";
    document.getElementById("account-sub").textContent = "Click to sign in";
  }
}

document.getElementById("rail-account").onclick = () => {
  if (!state.account) return loginModal();
  const others = state.accounts.filter(a => a.uuid !== state.account.uuid);
  modal(`
    <h2>${esc(state.account.name)}</h2>
    <p class="lead">Signed in with a Microsoft account.</p>
    ${others.length ? `<label class="field">Switch account</label>${others.map(a => `<button class="btn" data-switch="${a.uuid}" style="margin:0 6px 6px 0">${esc(a.name)}</button>`).join("")}` : ""}
    <div class="modal-actions">
      <button class="btn" id="m-add">Add another account</button>
      <button class="btn danger" id="m-logout">Sign out</button>
      <button class="btn primary" id="m-close">Done</button>
    </div>`);
  document.getElementById("m-close").onclick = closeModal;
  document.getElementById("m-add").onclick = loginModal;
  document.getElementById("m-logout").onclick = async () => { await call("logout"); closeModal(); await refresh(); render(); };
  document.querySelectorAll("[data-switch]").forEach(b => b.onclick = async () => { await call("switch_account", { uuid: b.dataset.switch }); closeModal(); await refresh(); render(); });
};

async function loginModal() {
  if (!state.settings.client_id) {
    modal(`
      <h2>Sign-in needs a client id</h2>
      <p class="lead">Minecraft sign-in goes through Microsoft, and every launcher needs its own application id that Mojang has approved. Paste yours below (the README explains how to get one).</p>
      <input type="text" id="m-cid" placeholder="Azure application (client) id">
      <div class="modal-actions"><button class="btn" id="m-cancel">Cancel</button><button class="btn primary" id="m-save">Save and continue</button></div>`);
    document.getElementById("m-cancel").onclick = closeModal;
    document.getElementById("m-save").onclick = async () => {
      const client_id = document.getElementById("m-cid").value.trim();
      if (!client_id) return;
      await call("save_settings", { settings: { ...state.settings, client_id } });
      await refresh();
      loginModal();
    };
    return;
  }
  const root = modal(`<h2>Sign in</h2><p class="lead">Getting a code from Microsoft…</p>`);
  let start;
  try {
    start = await call("login_start");
  } catch (e) {
    closeModal();
    return;
  }
  root.querySelector(".modal").innerHTML = `
    <h2>Sign in with Microsoft</h2>
    <p class="lead">Open the Microsoft page and enter this code. This window updates by itself once you are done.</p>
    <div class="code-box" id="m-code">${esc(start.user_code)}</div>
    <div class="modal-actions">
      <button class="btn" id="m-copy">Copy code</button>
      <button class="btn primary" id="m-open">Open Microsoft sign-in</button>
    </div>
    <p class="muted" style="margin-top:12px;font-size:12.5px">${esc(start.verification_uri)}</p>`;
  document.getElementById("m-copy").onclick = () => { navigator.clipboard.writeText(start.user_code); toast("Code copied"); };
  document.getElementById("m-open").onclick = () => T.opener.openUrl(start.verification_uri);
}

listen("login", e => {
  if (e.payload.ok) {
    closeModal();
    toast(`Signed in as ${e.payload.name}`, "ok");
    refresh().then(render);
  } else {
    toast(e.payload.error, "err");
  }
});

// ---------- progress + console ----------

const taskbar = document.getElementById("taskbar");
listen("progress", e => {
  const p = e.payload;
  taskbar.hidden = false;
  document.getElementById("task-phase").textContent = p.phase || "Working";
  if (p.message) document.getElementById("task-detail").textContent = p.message;
  const pct = p.total ? Math.round(p.done / p.total * 100) : 0;
  if (p.total) document.getElementById("task-detail").textContent = `${p.done} / ${p.total}`;
  document.getElementById("task-pct").textContent = p.total ? pct + "%" : "";
  document.getElementById("task-fill").style.strokeDashoffset = 97.4 - 97.4 * (p.total ? p.done / p.total : 0);
  updateTimeline(p);
});

const consoleEl = document.getElementById("console");
const consoleLog = document.getElementById("console-log");
listen("game-log", e => {
  const stick = consoleLog.scrollTop + consoleLog.clientHeight >= consoleLog.scrollHeight - 30;
  consoleLog.textContent += e.payload + "\n";
  if (consoleLog.textContent.length > 400000) consoleLog.textContent = consoleLog.textContent.slice(-300000);
  if (stick) consoleLog.scrollTop = consoleLog.scrollHeight;
});
listen("game-exit", e => {
  taskbar.hidden = true;
  toast(e.payload.code === 0 ? "Game closed" : `Game exited with code ${e.payload.code} (see the console)`, e.payload.code === 0 ? "ok" : "err");
  refresh().then(render);
});
document.getElementById("task-console").onclick = () => { consoleEl.hidden = false; };
document.getElementById("console-close").onclick = () => { consoleEl.hidden = true; };
document.getElementById("console-clear").onclick = () => { consoleLog.textContent = ""; };

// ---------- pages ----------

const pages = {};

pages.play = () => {
  const last = [...state.instances].sort((a, b) => (b.last_played || "").localeCompare(a.last_played || ""))[0];
  page.innerHTML = `
    <div class="page">
      <section class="hero">
        <img class="hero-gem" src="stone-large.png" alt="">
        <div class="hero-body">
          <div class="hero-kicker">${last ? "Continue playing" : "Welcome to Garnet"}</div>
          <h1>${last ? esc(last.name) : "Your Minecraft, your way"}</h1>
          <p class="lead">${last ? `Minecraft ${esc(last.minecraft)} · ${esc(last.loader)} · ${last.mods} mods · ${ago(last.last_played)}` : "Create an instance, or join a Garnet server and let it set everything up for you."}</p>
          <div class="row">
            ${last ? `<button class="btn primary big" id="hero-play">▶ &nbsp;Play</button>` : `<button class="btn primary big" id="hero-new">Create an instance</button>`}
            <button class="btn big ghost" id="hero-join">Join a server</button>
          </div>
        </div>
      </section>
      <h2>Instances</h2>
      <div class="grid" id="inst-grid">
        ${state.instances.map(i => {
          const [c1, c2] = coverColors(i.name);
          return `<div class="inst glass-hover ${state.running === i.name ? "running" : ""}" data-name="${esc(i.name)}">
            <div class="inst-cover" style="--c1:${c1};--c2:${c2}"></div>
            <div class="inst-body">
              <div class="inst-name">${esc(i.name)}</div>
              <div class="inst-meta"><span class="chip">${esc(i.minecraft)}</span><span class="chip ${i.loader === "fabric" ? "red" : ""}">${esc(i.loader)}</span>${i.server ? `<span class="chip green">server</span>` : ""}<span class="chip">${i.mods} mods</span></div>
              <div class="inst-actions">
                <button class="btn primary small" data-play="${esc(i.name)}" ${state.running ? "disabled" : ""}>▶ Play</button>
                <button class="btn small" data-mods="${esc(i.name)}">Mods</button>
                <button class="btn small" data-folder="${esc(i.name)}">Folder</button>
                <span class="spacer"></span>
                <button class="btn small danger" data-del="${esc(i.name)}">✕</button>
              </div>
            </div></div>`;
        }).join("")}
        <div class="inst-new" id="inst-new"><div><div class="plus">+</div>New instance</div></div>
      </div>
    </div>`;
  const heroPlay = document.getElementById("hero-play");
  if (heroPlay) heroPlay.onclick = () => launch(last.name, last.server);
  const heroNew = document.getElementById("hero-new");
  if (heroNew) heroNew.onclick = newInstanceModal;
  document.getElementById("hero-join").onclick = () => { location.hash = "#servers"; };
  document.getElementById("inst-new").onclick = newInstanceModal;
  page.querySelectorAll("[data-play]").forEach(b => b.onclick = () => launch(b.dataset.play, state.instances.find(i => i.name === b.dataset.play)?.server));
  page.querySelectorAll("[data-mods]").forEach(b => b.onclick = () => { selectedInstance = b.dataset.mods; location.hash = "#mods"; });
  page.querySelectorAll("[data-folder]").forEach(b => b.onclick = () => call("open_instance_folder", { name: b.dataset.folder }));
  page.querySelectorAll("[data-del]").forEach(b => b.onclick = async () => {
    if (!confirm(`Delete "${b.dataset.del}" and everything in it?`)) return;
    await call("delete_instance", { name: b.dataset.del });
    await refresh(); render();
  });
};

async function launch(name, server) {
  if (!state.account) return loginModal();
  taskbar.hidden = false;
  document.getElementById("task-phase").textContent = "Preparing " + name;
  try {
    await call("launch_instance", { name, server: server || null });
    toast(`Started ${name}`, "ok");
    await refresh(); render();
  } catch (_) {
    taskbar.hidden = true;
  }
}

async function newInstanceModal() {
  const versions = await call("list_versions");
  modal(`
    <h2>New instance</h2>
    <p class="lead">Instances keep their own mods, saves and settings.</p>
    <label class="field">Name</label><input type="text" id="n-name" placeholder="My world">
    <label class="field">Minecraft version</label>
    <select id="n-version">${versions.map(v => `<option value="${esc(v.id)}">${esc(v.id)} &nbsp; (${esc(v.date)})</option>`).join("")}</select>
    <label class="field">Mod loader</label>
    <select id="n-loader"><option value="fabric">Garnet Loader (Fabric) — mods, voice chat, shaders</option><option value="vanilla">Vanilla — no mods</option></select>
    <label class="field">Memory: <span id="n-mem-label">${state.settings.default_memory_mb || 4096} MB</span></label>
    <input type="range" class="range" id="n-mem" min="1024" max="16384" step="512" value="${state.settings.default_memory_mb || 4096}">
    <div class="modal-actions"><button class="btn" id="n-cancel">Cancel</button><button class="btn primary" id="n-create">Create</button></div>`);
  const mem = document.getElementById("n-mem");
  mem.oninput = () => { document.getElementById("n-mem-label").textContent = mem.value + " MB"; };
  document.getElementById("n-cancel").onclick = closeModal;
  document.getElementById("n-create").onclick = async () => {
    const name = document.getElementById("n-name").value.trim();
    if (!name) return toast("Give the instance a name", "err");
    const btn = document.getElementById("n-create");
    btn.disabled = true; btn.textContent = "Creating…";
    try {
      await call("create_instance", { new: { name, minecraft: document.getElementById("n-version").value, fabric: document.getElementById("n-loader").value === "fabric", memory_mb: Number(mem.value) } });
      closeModal(); toast(`Created ${name}`, "ok");
      await refresh(); render();
    } catch (_) { btn.disabled = false; btn.textContent = "Create"; }
  };
}

// ---------- servers ----------

const STEPS = ["Ping", "Instance", "Mods", "Downloads", "Launch"];
function phaseToStep(phase) {
  const p = (phase || "").toLowerCase();
  if (p.startsWith("ping")) return 0;
  if (p.includes("instance")) return 1;
  if (p.includes("mods") || p.includes("checking ")) return 2;
  if (p.includes("download") || p.includes("resolv") || p.includes("java") || p.includes("librar") || p.includes("assets")) return 3;
  if (p.includes("start")) return 4;
  return -1;
}
function updateTimeline(p) {
  const tl = document.getElementById("timeline");
  if (!tl) return;
  const idx = phaseToStep(p.phase);
  if (idx < 0) return;
  tl.querySelectorAll(".step").forEach((el, i) => {
    el.classList.toggle("done", i < idx);
    el.classList.toggle("active", i === idx);
    if (i === idx) el.querySelector(".step-detail").textContent = p.message || (p.total ? `${p.done} / ${p.total}` : p.phase);
  });
  const bar = document.getElementById("join-bar");
  if (bar) bar.style.width = (p.total ? p.done / p.total * 100 : 0) + "%";
}

pages.servers = () => {
  page.innerHTML = `
    <div class="page">
      <h1>Join a server</h1>
      <p class="lead">Garnet servers tell the launcher which mods they need. Enter an address and Garnet sets up an instance, installs the mods and drops you straight in. No modpacks.</p>
      <div class="row">
        <input type="text" class="big" id="srv-addr" placeholder="play.example.com or host:port" style="flex:1;max-width:520px">
        <button class="btn big" id="srv-check">Check</button>
      </div>
      <div id="srv-result" style="margin-top:18px"></div>
      <h2>Servers you have joined</h2>
      <div class="grid">${state.instances.filter(i => i.server).map(i => `
        <div class="card glass-hover">
          <div class="row"><div class="server-icon" style="width:44px;height:44px;font-size:16px">${esc(i.server[0].toUpperCase())}</div><div><div style="font-weight:700">${esc(i.server)}</div><div class="muted" style="font-size:12.5px">Minecraft ${esc(i.minecraft)} · ${i.mods} mods · ${ago(i.last_played)}</div></div></div>
          <div class="row" style="margin-top:12px"><button class="btn primary small" data-join="${esc(i.server)}">▶ Join</button><button class="btn small" data-mods="${esc(i.name)}">Mods</button></div>
        </div>`).join("") || `<div class="empty">Nothing yet. Check a server above.</div>`}</div>
    </div>`;
  const addr = document.getElementById("srv-addr");
  const check = async () => {
    const address = addr.value.trim();
    if (!address) return;
    const out = document.getElementById("srv-result");
    out.innerHTML = `<div class="card"><span class="muted">Pinging ${esc(address)}…</span></div>`;
    let s;
    try { s = await call("ping_server", { address }); } catch (_) { out.innerHTML = ""; return; }
    const g = s.garnet;
    out.innerHTML = `
      <div class="card server-card">
        <div class="server-icon">${esc(s.host[0].toUpperCase())}</div>
        <div style="flex:1;min-width:0">
          <div class="row"><b style="font-size:16px">${esc(s.host)}:${s.port}</b><span class="chip">${esc(s.version)}</span>${g ? `<span class="chip red">Garnet server</span>` : `<span class="chip">standard server</span>`}${g && g.voice ? `<span class="chip green">voice chat</span>` : ""}</div>
          <div class="muted" style="margin-top:4px">${esc(s.description)}</div>
          <div class="players-bar"><div style="width:${s.max ? Math.min(100, s.online / s.max * 100) : 0}%"></div></div>
          <div class="muted" style="font-size:12.5px">${s.online} / ${s.max} players online</div>
          ${g ? `
            <div class="modlist">${g.required.map(m => `<span class="chip red" title="required">${esc(m.name || m.id)}${m.version ? " " + esc(m.version) : ""}</span>`).join("")}
            ${g.optional.map(m => `<span class="chip" title="optional">${esc(m.name || m.id)}</span>`).join("")}
            ${g.shader_pack ? `<span class="chip yellow">✦ ${esc(g.shader_pack.name)}</span>` : ""}</div>
            <p class="muted" style="font-size:12.5px;margin:8px 0 0">${g.required.length ? `${g.required.length} required mod${g.required.length > 1 ? "s" : ""} will be installed automatically.` : "No extra mods needed."}${g.optional.length || g.shader_pack ? " Optional extras are installed when the box below is ticked." : ""}</p>` : `<p class="muted" style="font-size:12.5px;margin:8px 0 0">Not a Garnet server, but you can still join with a vanilla instance.</p>`}
          <div class="row" style="margin-top:14px">
            <button class="btn primary big" id="srv-join">▶ &nbsp;Join ${esc(s.host)}</button>
            ${g && (g.optional.length || g.shader_pack) ? `<label class="muted" style="display:flex;gap:6px;align-items:center"><input type="checkbox" id="srv-optional" checked> Include optional mods and shaders</label>` : ""}
          </div>
          <div class="timeline" id="timeline" hidden>
            ${STEPS.map((s, i) => `<div class="step"><div class="dot">${i + 1}</div><div><div style="font-weight:700">${s}</div><div class="step-detail muted" style="font-size:12.5px"></div></div></div>`).join("")}
            <div class="progress" style="margin-top:10px"><div id="join-bar"></div></div>
          </div>
        </div>
      </div>`;
    document.getElementById("srv-join").onclick = async () => {
      if (!state.account) return loginModal();
      const btn = document.getElementById("srv-join");
      btn.disabled = true; btn.textContent = "Joining…";
      document.getElementById("timeline").hidden = false;
      const optional = document.getElementById("srv-optional")?.checked ?? false;
      try {
        const name = await call("join_server", { address: `${s.host}:${s.port}`, optional });
        toast(`Started ${name}`, "ok");
        await refresh();
        btn.textContent = "Running";
      } catch (_) {
        btn.disabled = false; btn.textContent = `▶  Join ${s.host}`;
      }
    };
  };
  document.getElementById("srv-check").onclick = check;
  addr.onkeydown = e => { if (e.key === "Enter") check(); };
  page.querySelectorAll("[data-join]").forEach(b => b.onclick = () => { addr.value = b.dataset.join; check(); });
  page.querySelectorAll("[data-mods]").forEach(b => b.onclick = () => { selectedInstance = b.dataset.mods; location.hash = "#mods"; });
  addr.focus();
};

// ---------- mods ----------

pages.mods = async (kind = "mod") => {
  const fabricInstances = state.instances.filter(i => i.loader === "fabric" || kind !== "mod");
  if (!selectedInstance || !state.instances.some(i => i.name === selectedInstance)) selectedInstance = fabricInstances[0]?.name || state.instances[0]?.name || null;
  const title = kind === "shader" ? "Shaders" : "Mods";
  page.innerHTML = `
    <div class="page">
      <div class="row"><h1 style="margin:0">${title}</h1><span class="spacer"></span>
        <label class="muted">Instance</label>
        <select id="mods-inst">${state.instances.map(i => `<option ${i.name === selectedInstance ? "selected" : ""}>${esc(i.name)}</option>`).join("")}</select></div>
      <p class="lead" style="margin-top:8px">${kind === "shader" ? "Shader packs from Modrinth, installed into the instance's shaderpacks folder. Iris is needed to load them (it is a mod; install it from the Mods page)." : "Search Modrinth and install with one click. Dependencies come along automatically."}</p>
      ${state.instances.length ? "" : `<div class="empty">Create an instance first.</div>`}
      <div class="split">
        <div class="card">
          <div class="row"><input type="text" id="mods-q" placeholder="Search ${title.toLowerCase()}…" style="flex:1"><button class="btn" id="mods-go">Search</button></div>
          <div class="list" id="mods-results" style="margin-top:10px"><div class="empty">Type something to search.</div></div>
        </div>
        <div class="card">
          <div class="row"><b>Installed</b><span class="spacer"></span><button class="btn small" id="mods-folder">Open folder</button></div>
          <div class="list" id="mods-installed" style="margin-top:10px"></div>
        </div>
      </div>
    </div>`;
  const instSel = document.getElementById("mods-inst");
  instSel.onchange = () => { selectedInstance = instSel.value; pages.mods(kind); };
  document.getElementById("mods-folder").onclick = () => selectedInstance && call("open_instance_folder", { name: selectedInstance });
  const renderInstalled = async () => {
    const list = document.getElementById("mods-installed");
    if (!selectedInstance) { list.innerHTML = `<div class="empty">No instance.</div>`; return; }
    const mods = await call("list_mods", { instance: selectedInstance });
    list.innerHTML = mods.map(m => `<div class="modrow"><div class="noicon">${esc((m.id || "?")[0].toUpperCase())}</div><div class="mr-body"><div class="mr-title">${esc(m.id)}</div><div class="mr-desc">${esc(m.version)} · ${esc(m.source)} · ${esc(m.file)}</div></div><button class="btn small danger" data-rm="${esc(m.file)}">Remove</button></div>`).join("") || `<div class="empty">Nothing installed yet.</div>`;
    list.querySelectorAll("[data-rm]").forEach(b => b.onclick = async () => { await call("remove_mod", { instance: selectedInstance, file: b.dataset.rm }); renderInstalled(); });
  };
  renderInstalled();
  const search = async () => {
    const q = document.getElementById("mods-q").value.trim();
    const results = document.getElementById("mods-results");
    if (!selectedInstance) return;
    results.innerHTML = `<div class="empty">Searching…</div>`;
    let hits;
    try { hits = await call("search_mods", { instance: selectedInstance, query: q, kind }); } catch (_) { results.innerHTML = ""; return; }
    results.innerHTML = hits.map(h => `<div class="modrow">
      ${h.icon_url ? `<img src="${esc(h.icon_url)}" alt="">` : `<div class="noicon">${esc(h.title[0])}</div>`}
      <div class="mr-body"><div class="mr-title">${esc(h.title)} <span class="mr-dl">· ${Intl.NumberFormat().format(h.downloads)} downloads</span></div><div class="mr-desc">${esc(h.description)}</div></div>
      <button class="btn primary small" data-install="${esc(h.slug)}">Install</button></div>`).join("") || `<div class="empty">No results for ${esc(q)}.</div>`;
    results.querySelectorAll("[data-install]").forEach(b => b.onclick = async () => {
      b.disabled = true; b.textContent = "Installing…";
      try { const f = await call("install_mod", { instance: selectedInstance, project: b.dataset.install, kind }); toast(`Installed ${f}`, "ok"); b.textContent = "Installed"; renderInstalled(); }
      catch (_) { b.disabled = false; b.textContent = "Install"; }
    });
  };
  document.getElementById("mods-go").onclick = search;
  document.getElementById("mods-q").onkeydown = e => { if (e.key === "Enter") search(); };
  // An empty search shows the most popular projects, a good starting point.
  search();
};

pages.shaders = async () => {
  await pages.mods("shader");
  const p = page.querySelector(".page");
  const hero = document.createElement("section");
  hero.className = "shader-hero";
  hero.style.marginBottom = "18px";
  hero.innerHTML = `
    <div class="shader-preview"></div>
    <div class="card">
      <div class="hero-kicker">Garnet Shaders</div>
      <h1 style="font-size:24px">Realistic lighting, built for Garnet</h1>
      <p class="lead" style="margin-bottom:8px">Our own shader pack for Iris: soft shadows, screen-space reflections, volumetric light, bloom and an experimental ray-traced global illumination mode.</p>
      <ul class="feature-list"><li>Cascaded soft shadows</li><li>Water reflections and refraction</li><li>Volumetric sun rays and fog</li><li>Ray-traced GI (experimental)</li></ul>
      <div class="row" style="margin-top:14px"><button class="btn primary" id="garnet-shaders">Install Garnet Shaders</button><span class="muted" style="font-size:12.5px">Installs Iris as well if needed.</span></div>
    </div>`;
  p.insertBefore(hero, p.children[2]);
  document.getElementById("garnet-shaders").onclick = async () => {
    if (!selectedInstance) return toast("Pick an instance first", "err");
    const btn = document.getElementById("garnet-shaders");
    btn.disabled = true; btn.textContent = "Installing…";
    try {
      await call("install_mod", { instance: selectedInstance, project: "iris", kind: "mod" });
      await call("install_mod", { instance: selectedInstance, project: "garnet-shaders", kind: "shader" });
      toast("Garnet Shaders installed", "ok");
    } catch (_) {
      toast("Garnet Shaders is not on Modrinth yet; Iris was installed", "err");
    }
    btn.disabled = false; btn.textContent = "Install Garnet Shaders";
  };
};

// ---------- settings ----------

pages.settings = () => {
  const s = state.settings;
  page.innerHTML = `
    <div class="page">
      <h1>Settings</h1>
      <p class="lead">Garnet keeps everything in <span class="mono">${esc(state.home)}</span>.</p>
      <div class="card" style="max-width:640px">
        <label class="field">Microsoft application (client) id</label>
        <input type="text" id="s-cid" value="${esc(s.client_id || "")}" placeholder="Needed for sign-in; see the README">
        <label class="field">Default memory for new instances: <span id="s-mem-label">${s.default_memory_mb} MB</span></label>
        <input type="range" class="range" id="s-mem" min="1024" max="16384" step="512" value="${s.default_memory_mb}">
        <label class="field">Extra JVM arguments (one per line)</label>
        <textarea id="s-jvm" rows="3" style="width:100%">${esc((s.jvm_args || []).join("\n"))}</textarea>
        <div class="modal-actions" style="justify-content:flex-start"><button class="btn primary" id="s-save">Save</button></div>
      </div>
      <h2>About</h2>
      <div class="card" style="max-width:640px">
        <div class="row"><img src="stone.png" style="width:48px;height:48px;object-fit:contain" alt=""><div><b>Garnet launcher ${esc(state.version)}</b><div class="muted">Open source, MIT. Downloads the game from Mojang, signs in with Microsoft, and never modifies the game jar.</div></div></div>
      </div>
    </div>`;
  const mem = document.getElementById("s-mem");
  mem.oninput = () => { document.getElementById("s-mem-label").textContent = mem.value + " MB"; };
  document.getElementById("s-save").onclick = async () => {
    await call("save_settings", { settings: { client_id: document.getElementById("s-cid").value.trim(), default_memory_mb: Number(mem.value), jvm_args: document.getElementById("s-jvm").value.split("\n").map(x => x.trim()).filter(Boolean) } });
    toast("Settings saved", "ok");
    await refresh();
  };
};

// ---------- routing ----------

async function render() {
  const name = (location.hash || "#play").slice(1);
  document.querySelectorAll(".rail-item").forEach(a => a.classList.toggle("active", a.dataset.page === name));
  await (pages[name] || pages.play)();
}

window.addEventListener("hashchange", render);
refresh().then(render);
