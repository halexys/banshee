"use strict";
// a desktop app, not a page: no text selection or dragging
for (const ev of ["selectstart", "dragstart"]) document.addEventListener(ev, (e) => e.preventDefault());
const { invoke } = window.__TAURI__.core;
const COLORS = { silent: "#3CC8B4", balanced: "#5AA9FF", extreme: "#FF8A3D", superbatt: "#8BD45A" };
const NAMES = { silent: "Silencioso", balanced: "Equilibrado", extreme: "Extremo", superbatt: "Super Batería" };
const $ = (s) => document.querySelector(s);
let view = null;

function render(v) {
  view = v;
  const s = v.status, b = s.battery;
  document.documentElement.style.setProperty("--accent", COLORS[s.profile] || "#8B97A3");
  $("#pname").textContent = NAMES[s.profile] || "Personalizado";
  const lim = v.travel_active ? "✈ 100 %" : `${b.limit ?? "--"} %`;
  $("#psub").textContent = `${b.ac ? "AC" : "BAT"} · ${b.pct ?? "--"} % → ${lim}`;
  $("#tcpu").textContent = s.cpu_temp != null ? `${s.cpu_temp}°` : "--";
  $("#tgpu").textContent = s.gpu.awake && s.gpu.temp != null ? `${s.gpu.temp}°` : "zZ";
  $("#trpm").textContent = s.rpm_cpu ?? "--";
  document.querySelectorAll(".pp button").forEach((el) => {
    el.classList.toggle("on", el.dataset.p === s.profile);
    el.disabled = !s.writable;
  });
  const bo = $("#pboost");
  bo.classList.toggle("on", s.cooler_boost);
  bo.setAttribute("aria-pressed", s.cooler_boost);
  bo.disabled = !s.writable;
  $("#pboostv").textContent = s.cooler_boost ? "ON" : "OFF";
  $("#ppanic").disabled = !s.writable;
}

async function act(cmd, args) {
  try { render(await invoke(cmd, args)); } catch (e) { console.error(e); }
}

document.querySelectorAll(".pp button").forEach((el) =>
  el.addEventListener("click", () => act("set_profile", { id: el.dataset.p })));
$("#pboost").addEventListener("click", () => act("set_boost", { on: !view?.status.cooler_boost }));
$("#popen").addEventListener("click", () => invoke("open_main"));
$("#ppanic").addEventListener("click", () => act("panic_reset", {}));

async function poll() { try { render(await invoke("get_status")); } catch (e) {} }
poll();
setInterval(poll, 1000);
