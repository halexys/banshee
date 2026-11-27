"use strict";
// a desktop app, not a page: no text selection or dragging
for (const ev of ["selectstart", "dragstart"]) document.addEventListener(ev, (e) => e.preventDefault());
const { invoke } = window.__TAURI__.core;

const C = 213.6; // ring circumference (r = 34)
const COLORS = { silent: "#3CC8B4", balanced: "#5AA9FF", extreme: "#FF8A3D", superbatt: "#8BD45A" };
const $ = (s) => document.querySelector(s);
const $$ = (s) => Array.from(document.querySelectorAll(s));

let view = null;
let busy = false;

function ring(name, frac) {
  const arc = document.querySelector(`[data-g="${name}"] .arc`);
  const f = Math.max(0, Math.min(1, frac || 0));
  arc.style.strokeDasharray = `${(f * C).toFixed(1)} ${C}`;
}

function gauge(name, num, sub, frac, sleeping) {
  const g = document.querySelector(`[data-g="${name}"]`);
  g.querySelector(".num").textContent = num ?? "--";
  g.querySelector(".sub").textContent = sub ?? "";
  g.classList.toggle("sleep", !!sleeping);
  ring(name, frac);
}

const w = (x) => (x == null ? "" : `${x.toFixed(x < 10 ? 1 : 0)} W`);

function render(v) {
  view = v;
  if (window.renderFans) window.renderFans(v);
  const s = v.status;
  const accent = COLORS[s.profile] || "#8B97A3";
  document.documentElement.style.setProperty("--accent", accent);

  $("#fw").textContent = s.fw ? `PRESTIGE 16 · ${s.fw.split(".")[0]}` : "PRESTIGE 16";

  // banner: module missing / read-only
  const banner = $("#banner");
  if (!s.msi) {
    banner.hidden = false; banner.classList.add("err");
    $("#bannerText").textContent = "msi-ec no está cargado";
    $("#setupBtn").hidden = true;
  } else if (!s.writable || !s.debug) {
    banner.hidden = false; banner.classList.remove("err");
    $("#bannerText").textContent = "Solo lectura";
    $("#setupBtn").hidden = false;
  } else {
    banner.hidden = true;
  }
  const ro = !s.writable;

  // power
  const b = s.battery;
  $("#power").classList.toggle("bat", !b.ac);
  $("#powerLabel").textContent = b.ac ? "AC" : `${b.pct ?? "--"} %`;

  // gauges
  gauge("cpu", s.cpu_temp, w(s.cpu_watts), (s.cpu_temp || 0) / 100);
  if (s.gpu.awake) gauge("gpu", s.gpu.temp, `RTX · ${w(s.gpu.watts)}`, (s.gpu.temp || 0) / 100);
  else gauge("gpu", "zZ", s.gfx && s.gfx.booted === "igpu" ? "desactivada" : "dormida", 0, true);
  const boostRpm = 6200;
  gauge("fcpu", s.rpm_cpu, s.cooler_boost ? "boost" : (s.duty_cpu != null ? `${s.duty_cpu} %` : ""), (s.rpm_cpu || 0) / boostRpm);
  gauge("fgpu", s.rpm_gpu, s.cooler_boost ? "boost" : (s.duty_gpu != null ? `${s.duty_gpu} %` : ""), (s.rpm_gpu || 0) / boostRpm);
  let bsub = "";
  if (v.travel_active) bsub = "viaje · 100 %";
  else if (b.limit) bsub = `límite ${b.limit} %`;
  if (b.status === "Charging" && b.watts) bsub = `+${w(b.watts)} · ${bsub}`;
  else if (b.status === "Discharging" && b.watts) bsub = `−${w(b.watts)}`;
  gauge("bat", b.pct, bsub, (b.pct || 0) / 100);

  // profiles
  $$(".profile").forEach((el) => {
    el.classList.toggle("on", el.dataset.p === s.profile);
    el.setAttribute("aria-pressed", el.dataset.p === s.profile);
    el.disabled = ro;
  });

  // boost
  const bo = $("#boost");
  bo.classList.toggle("on", s.cooler_boost);
  bo.setAttribute("aria-pressed", s.cooler_boost);
  bo.disabled = ro;
  $("#boostSub").textContent = s.cooler_boost ? `ON · ${s.rpm_cpu ?? "--"} rpm` : "OFF";

  seg("#kbd", s.kbd == null ? null : String(s.kbd), ro || !s.debug);
  seg("#charge", v.travel_active ? "travel" : String(v.charge_limit), ro);
  seg("#fn", s.fn_right ? "right" : "left", ro);

  // graphics: highlight the mode requested for next boot, dashed if a reboot is pending
  const gx = s.gfx;
  $$("#gfx button").forEach((el) => {
    el.disabled = !gx || busy;
    el.classList.toggle("on", !!gx && el.dataset.v === gx.next && gx.next === gx.booted);
    el.classList.toggle("pending", !!gx && el.dataset.v === gx.next && gx.next !== gx.booted);
  });
  $("#gfxTag").hidden = !gx || gx.next === gx.booted;
}

function seg(sel, val, disabled) {
  $$(`${sel} button`).forEach((el) => {
    el.classList.toggle("on", el.dataset.v === val);
    el.setAttribute("aria-pressed", el.dataset.v === val);
    el.disabled = disabled;
  });
}

let toastTimer = null;
function toast(msg) {
  const t = $("#toast");
  t.textContent = msg;
  t.hidden = false;
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => (t.hidden = true), 5000);
}

async function act(cmd, args) {
  try {
    render(await invoke(cmd, args));
  } catch (e) {
    const m = String(e);
    if (m === "CANCELLED") return;
    if (m === "SETUP") { toast("Falta activar el control"); return; }
    toast(m);
  }
}

async function poll() {
  try { render(await invoke("get_status")); } catch (e) { toast(String(e)); }
}

// wiring
$$(".rail button").forEach((el) => el.addEventListener("click", () => {
  $$(".rail button").forEach((b) => b.classList.toggle("on", b === el));
  $("#view-home").hidden = el.dataset.view !== "home";
  $("#view-fans").hidden = el.dataset.view !== "fans";
}));
$$(".profile").forEach((el) => el.addEventListener("click", () => act("set_profile", { id: el.dataset.p })));
$("#boost").addEventListener("click", () => act("set_boost", { on: !view?.status.cooler_boost }));
$$("#kbd button").forEach((el) => el.addEventListener("click", () => act("set_kbd", { level: Number(el.dataset.v) })));
$$("#charge button").forEach((el) => el.addEventListener("click", () => {
  if (el.dataset.v === "travel") act("set_travel", { on: !view?.travel_active });
  else act("set_charge", { pct: Number(el.dataset.v) });
}));
$$("#fn button").forEach((el) => el.addEventListener("click", () => act("set_fn", { right: el.dataset.v === "right" })));
$$("#gfx button").forEach((el) => el.addEventListener("click", async () => {
  busy = true; render(view);
  await act("set_gpu_mode", { igpu: el.dataset.v === "igpu" });
  busy = false; if (view) render(view);
}));
$("#setupBtn").addEventListener("click", async () => {
  $("#setupBtn").disabled = true;
  await act("run_setup", {});
  $("#setupBtn").disabled = false;
});

poll();
setInterval(poll, 1000);
