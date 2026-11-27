"use strict";
// Shortcuts screen: GNOME custom keybindings (accelerators like "<Control><Alt>p").

(() => {
  const ACTIONS = [
    { id: "cycle", name: "Siguiente perfil", c: "#E6EDF3", icon: '<path d="M4 12a8 8 0 0 1 14-5.3L20 9M20 4v5h-5M20 12a8 8 0 0 1-14 5.3L4 15M4 20v-5h5"/>' },
    { id: "silent", name: "Silencioso", c: "#3CC8B4", dot: true },
    { id: "balanced", name: "Equilibrado", c: "#5AA9FF", dot: true },
    { id: "extreme", name: "Extremo", c: "#FF8A3D", dot: true },
    { id: "superbatt", name: "Super Batería", c: "#8BD45A", dot: true },
    { id: "boost", name: "Fan Boost", c: "#FF8A3D", icon: '<circle cx="12" cy="12" r="2"/><path d="M12 10c0-4 1-7 4-7s3 4-2 7M14 12c4 0 7 1 7 4s-4 3-7-2M12 14c0 4-1 7-4 7s-3-4 2-7M10 12c-4 0-7-1-7-4s4-3 7 2"/>' },
    { id: "curve", name: "Curva propia sí/no", c: "#5AA9FF", icon: '<path d="M3 20h18M4 17l5-3 4 1 6-8"/><circle cx="9" cy="14" r="1.2"/><circle cx="13" cy="15" r="1.2"/><circle cx="19" cy="7" r="1.2"/>' },
    { id: "panel", name: "Panel rápido", c: "#5AA9FF", icon: '<path d="M5 20V10a7 7 0 0 1 14 0v10l-2.5-2-2.3 2-2.2-2-2.2 2-2.3-2z"/><circle cx="9.5" cy="10.5" r="1"/><circle cx="14.5" cy="10.5" r="1"/>' },
    { id: "open", name: "Abrir Banshee", c: "#8B97A3", icon: '<rect x="3" y="4" width="18" height="14" rx="2"/><path d="M3 9h18"/>' },
  ];
  const MODS = { Control: "Ctrl", Alt: "Alt", Shift: "Shift", Super: "Super", Primary: "Ctrl", Meta: "Super" };
  const NAMES = { space: "Espacio", Return: "Enter", Escape: "Esc", Up: "↑", Down: "↓", Left: "←", Right: "→", Page_Up: "RePág", Page_Down: "AvPág" };
  const KEYMAP = { " ": "space", Enter: "Return", ArrowUp: "Up", ArrowDown: "Down", ArrowLeft: "Left", ArrowRight: "Right", PageUp: "Page_Up", PageDown: "Page_Down", Backspace: "BackSpace", Delete: "Delete", Home: "Home", End: "End", Insert: "Insert" };

  let data = null;
  let capturing = null;

  // "<Control><Alt>p" -> ["Ctrl","Alt","P"]
  function chips(accel) {
    if (!accel) return [];
    const out = [];
    const re = /<([^>]+)>/g;
    let m, last = 0;
    while ((m = re.exec(accel))) { out.push(MODS[m[1]] || m[1]); last = re.lastIndex; }
    const k = accel.slice(last);
    out.push(NAMES[k] || (k.length === 1 ? k.toUpperCase() : k));
    return out;
  }

  // keydown -> accelerator, or null for a lone modifier
  function toAccel(e) {
    if (["Control", "Alt", "Shift", "Meta", "OS", "Super", "AltGraph"].includes(e.key)) return null;
    let key = KEYMAP[e.key] || e.key;
    if (/^F\d{1,2}$/.test(key)) { /* keep */ }
    else if (e.code && /^Key[A-Z]$/.test(e.code)) key = e.code.slice(3).toLowerCase();
    else if (e.code && /^Digit\d$/.test(e.code)) key = e.code.slice(5);
    else if (key.length === 1) key = key.toLowerCase();
    let a = "";
    if (e.ctrlKey) a += "<Control>";
    if (e.altKey) a += "<Alt>";
    if (e.shiftKey) a += "<Shift>";
    if (e.metaKey) a += "<Super>";
    // a bare letter would hijack normal typing everywhere
    if (!a && !/^F\d{1,2}$/.test(key)) return "";
    return a + key;
  }

  function capHtml(list) {
    return list.map((k) => `<span class="kcap">${k}</span>`).join("");
  }

  function render() {
    if (!data) return;
    const box = document.getElementById("krows");
    if (!box.children.length) {
      box.innerHTML = ACTIONS.map((a) => `
        <div class="krow" data-id="${a.id}" style="--c:${a.c}">
          <span class="ico"><svg width="28" height="28" viewBox="0 0 24 24" aria-hidden="true">${a.dot ? '<circle cx="12" cy="12" r="6" fill="currentColor" stroke="none"/>' : a.icon}</svg></span>
          <span class="kname">${a.name}</span>
          <button type="button" class="kbtn" aria-label="Cambiar atajo de ${a.name}"></button>
          <button type="button" class="kx" aria-label="Quitar atajo de ${a.name}"><svg width="16" height="16" viewBox="0 0 24 24" aria-hidden="true" style="stroke-width:2.2"><path d="M6 6l12 12M18 6L6 18"/></svg></button>
        </div>`).join("");
      box.querySelectorAll(".krow").forEach((row) => {
        const id = row.dataset.id;
        const btn = row.querySelector(".kbtn");
        btn.addEventListener("click", () => { capturing = id; window.capturingShortcut = true; render(); btn.focus(); });
        btn.addEventListener("blur", () => { if (capturing === id) { capturing = null; window.capturingShortcut = false; render(); } });
        btn.addEventListener("keydown", (e) => {
          if (capturing !== id) return;
          e.preventDefault(); e.stopPropagation();
          if (e.key === "Escape") { capturing = null; window.capturingShortcut = false; render(); return; }
          const accel = toAccel(e);
          if (accel === null) return;
          if (accel === "") { toast("Usa al menos Ctrl, Alt o Super (o una tecla F)"); return; }
          capturing = null; window.capturingShortcut = false;
          save(id, accel);
        });
        row.querySelector(".kx").addEventListener("click", () => save(id, ""));
      });
    }
    box.querySelectorAll(".krow").forEach((row) => {
      const id = row.dataset.id;
      const item = data.items.find((i) => i.id === id);
      const list = chips(item ? item.accel : "");
      const cap = capturing === id;
      row.classList.toggle("cap", cap);
      row.querySelector(".kbtn").innerHTML = cap ? '<span class="wait">PULSA…</span>' : list.length ? capHtml(list) : '<span class="none">—</span>';
      row.querySelector(".kx").disabled = !list.length;
    });
    document.getElementById("kScen").setAttribute("aria-checked", data.scenario_key);
  }

  async function load() {
    try { data = await invoke("get_shortcuts"); render(); } catch (e) { toast(String(e)); }
  }
  async function save(id, accel) {
    try { data = await invoke("set_shortcut", { id, accel }); } catch (e) { toast(String(e)); }
    render();
  }

  document.getElementById("kScen").addEventListener("click", async () => {
    data = await invoke("set_scenario_key", { on: !data.scenario_key });
    render();
  });
  document.getElementById("kGnome").addEventListener("click", () => invoke("open_gnome_keys"));
  window.addEventListener("view", (e) => { if (e.detail === "keys") load(); });
})();
