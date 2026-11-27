"use strict";
// Fan-curve editor. The EC keeps two step tables (7 speeds each, MSI's temperatures); speeds are
// EC duty units, shown as % on MSI's own 0-150 scale (150 = the fans' real maximum).

(() => {
  const L = 46, R = 544, TOP = 24, BOT = 262, W = R - L, H = BOT - TOP;
  const X = (i) => L + (i * W) / 6;
  const Y = (pct) => BOT - (pct / 150) * H;
  const FAN_COLOR = { cpu: "#FF8A3D", gpu: "#5AA9FF" };
  const PROFILES = [
    { id: "silent", name: "Silencioso", c: "#3CC8B4" },
    { id: "balanced", name: "Equilibrado", c: "#5AA9FF" },
    { id: "extreme", name: "Extremo", c: "#FF8A3D" },
    { id: "superbatt", name: "Super Batería", c: "#8BD45A" },
  ];
  const INTENTS = [
    { k: "quiet", name: "Silencio" },
    { k: "factory", name: "Fábrica" },
    { k: "cool", name: "Fresco" },
    { k: "max", name: "Máximo" },
  ];

  let max = 150;
  let draft = null;     // {cpu:[7], gpu:[7]} in EC units, what the charts show
  let dirty = false;
  let drag = null;      // {fan, i}
  let last = null;      // last view from the backend

  const pct = (raw) => raw;
  const raw = (p) => Math.max(0, Math.min(max, Math.round(p)));
  const copy = (c) => ({ cpu: c.cpu.slice(), gpu: c.gpu.slice() });
  const same = (a, b) => a.cpu.every((v, i) => v === b.cpu[i]) && a.gpu.every((v, i) => v === b.gpu[i]);

  // ---------- shapes (derived from MSI's own table, like GhostDeck) ----------
  function shape(kind, fan, temps) {
    const f = last.fans.factory[fan];
    const clamp = (v) => Math.max(0, Math.min(max, Math.round(v)));
    if (kind === "quiet") return f.map((v) => clamp(v - 0.12 * max));
    if (kind === "cool") return f.map((v) => clamp(v + 0.10 * max));
    if (kind === "max") {
      // MSI's curve up to 55 °C, then straight to full speed by 70 °C
      return f.map((v, i) => {
        const t = i === 0 ? 0 : temps[i - 1];
        if (t < 55) return v;
        return clamp(v + (max - v) * Math.min(1, (t - 55) / 15));
      });
    }
    return f.slice();
  }
  const temps = (fan) => {
    const t = last.status.fan_temps;
    return t ? t[fan] : fan === "cpu" ? [50, 55, 61, 67, 80, 90] : [42, 47, 51, 58, 75, 80];
  };
  const shapeBoth = (kind) => ({ cpu: shape(kind, "cpu", temps("cpu")), gpu: shape(kind, "gpu", temps("gpu")) });

  // ---------- state helpers ----------
  const current = () => last.status.profile;
  const assigned = (id) => last.fans.profiles.includes(id);
  const editable = () => last.status.writable;

  function setDraft(d) {
    draft = d;
    dirty = !same(draft, last.fans.curve);
    paint();
  }

  // ---------- painting ----------
  function plot(fan) {
    const s = draft[fan].map(pct);
    const t = temps(fan);
    const color = FAN_COLOR[fan];
    const pts = s.map((v, i) => [X(i), Y(v)]);
    const P = (q) => `${q[0].toFixed(1)},${q[1].toFixed(1)}`;
    const zone = (lo, hi) => [Y(hi), Y(lo) - Y(hi)];
    const [qy, qh] = zone(0, 30), [my, mh] = zone(30, 60), [ly, lh] = zone(60, 150);
    let g = `
      <defs><linearGradient id="g${fan}" x1="0" y1="0" x2="0" y2="1">
        <stop offset="0" stop-color="${color}" stop-opacity="0.32"/><stop offset="1" stop-color="${color}" stop-opacity="0.03"/>
      </linearGradient></defs>
      <rect x="${L}" y="${ly}" width="${W}" height="${lh}" fill="#FF5C8A" opacity="0.07"/>
      <rect x="${L}" y="${my}" width="${W}" height="${mh}" fill="#FFC15D" opacity="0.06"/>
      <rect x="${L}" y="${qy}" width="${W}" height="${qh}" fill="#61E7A4" opacity="0.06"/>
      <text x="${R - 6}" y="${ly + 14}" fill="#FF5C8A" opacity=".75" font-size="11" text-anchor="end">ruidoso</text>
      <text x="${R - 6}" y="${my + 14}" fill="#FFC15D" opacity=".75" font-size="11" text-anchor="end">se oye</text>
      <text x="${R - 6}" y="${qy + 14}" fill="#61E7A4" opacity=".75" font-size="11" text-anchor="end">silencio</text>`;
    for (let p = 0; p <= 150; p += 25) {
      g += `<path d="M ${L} ${Y(p)} H ${R}" stroke="#1E252C"/>`;
      g += `<text x="38" y="${Y(p) + 4}" fill="#6F7B87" font-size="12" text-anchor="end">${p}%</text>`;
    }
    g += `<path d="M ${pts.map((q) => q.map((n) => n.toFixed(1)).join(" ")).join(" L ")} L ${R} ${BOT} L ${L} ${BOT} Z" fill="url(#g${fan})"/>`;
    g += `<path d="${pts.map((q) => `M ${P(q).replace(",", " ")} V ${BOT}`).join(" ")}" stroke="${color}" stroke-opacity=".3" stroke-dasharray="3 4"/>`;
    g += `<polyline points="${pts.map(P).join(" ")}" fill="none" stroke="${color}" stroke-width="3" stroke-linejoin="round"/>`;
    pts.forEach((q, i) => {
      const r = drag && drag.fan === fan && drag.i === i ? 9 : 7;
      const label = s[i] >= max ? "MAX" : s[i] + "%";
      g += `<text x="${q[0]}" y="${q[1] - 14}" fill="#E6EDF3" font-size="13" font-weight="700" text-anchor="middle">${label}</text>`;
      g += `<text x="${q[0]}" y="284" fill="#8B97A3" font-size="12" text-anchor="middle">${i === 0 ? "<" + t[0] : t[i - 1]}°</text>`;
      g += `<circle class="node" data-i="${i}" cx="${q[0]}" cy="${q[1]}" r="${r}" fill="${color}" stroke="#12171C" stroke-width="3"/>`;
    });

    // live point: temperature mapped onto the index-spaced axis, riding on the curve
    const st = last.status;
    const lt = fan === "cpu" ? st.cpu_temp : st.gpu.awake ? st.gpu.temp : null;
    const duty = fan === "cpu" ? st.duty_cpu : st.duty_gpu;
    if (lt != null) {
      const nodesT = [t[0] - 15].concat(t);
      let k = 0;
      while (k < 5 && lt >= nodesT[k + 1]) k++;
      const f = Math.max(0, Math.min(1, (lt - nodesT[k]) / (nodesT[k + 1] - nodesT[k])));
      const lx = X(k + f), lyv = Y(s[k] + (s[k + 1] - s[k]) * f);
      const accent = getComputedStyle(document.documentElement).getPropertyValue("--accent").trim() || "#5AA9FF";
      const text = `${lt}° · ${duty != null ? pct(duty) + "%" : "--"}`;
      const pw = 14 + text.length * 7.4;
      let px = lx + 16; if (px + pw > R) px = lx - pw - 16;
      let py = lyv - 40; if (py < TOP) py = lyv + 14;
      g += `<circle cx="${lx}" cy="${lyv}" r="12" fill="none" stroke="#E6EDF3" stroke-opacity=".45" stroke-width="2"/>
        <circle cx="${lx}" cy="${lyv}" r="6.5" fill="${accent}"/><circle cx="${lx}" cy="${lyv}" r="2.5" fill="#fff"/>
        <rect x="${px}" y="${py}" width="${pw}" height="24" rx="6" fill="#0B0E11" fill-opacity=".9" stroke="${accent}" stroke-opacity=".7"/>
        <text x="${px + pw / 2}" y="${py + 16}" fill="#E6EDF3" font-size="12" font-weight="700" text-anchor="middle">${text}</text>`;
    } else if (fan === "gpu") {
      g += `<text x="${L + 6}" y="${TOP + 12}" fill="#6F7B87" font-size="12">GPU dormida</text>`;
    }
    return g;
  }

  function paint() {
    if (!last || !draft) return;
    const root = document.getElementById("view-fans");
    const st = last.status;
    const cur = current();
    const on = !!cur && assigned(cur);
    root.classList.toggle("ro", !editable());

    const sw = document.getElementById("fSwitch");
    sw.setAttribute("aria-checked", on);
    sw.disabled = !editable() || !cur || cur === "silent";
    sw.title = cur === "silent" ? "Silencioso usa siempre la curva de fábrica" : "";
    const mode = document.getElementById("fMode");
    mode.textContent = st.cooler_boost ? "BOOST" : st.fan_curve ? "AVANZADO" : "AUTO";
    mode.classList.toggle("on", st.fan_curve || st.cooler_boost);

    const ap = document.getElementById("fApply");
    ap.disabled = !editable() || !dirty;
    ap.classList.toggle("done", !dirty);
    document.getElementById("fApplyL").textContent = dirty ? "Aplicar" : "Aplicada";
    document.getElementById("fReset").disabled = !editable();

    // intent tiles
    const box = document.getElementById("intents");
    if (!box.children.length) {
      box.innerHTML = INTENTS.map((it) =>
        `<button type="button" class="intent" data-k="${it.k}"><svg width="52" height="34" viewBox="0 0 52 34" aria-hidden="true"><polyline points=""/></svg><span>${it.name}</span></button>`).join("");
      box.querySelectorAll(".intent").forEach((b) => b.addEventListener("click", () => setDraft(shapeBoth(b.dataset.k))));
    }
    box.querySelectorAll(".intent").forEach((b) => {
      const sh = shapeBoth(b.dataset.k);
      b.querySelector("polyline").setAttribute("points", sh.cpu.map((v, i) => `${2 + i * 8},${31 - (pct(v) * 0.19).toFixed(1)}`).join(" "));
      const act = same(sh, draft);
      b.classList.toggle("on", act);
      b.setAttribute("aria-pressed", act);
      b.disabled = !editable();
    });

    // charts
    for (const fan of ["cpu", "gpu"]) {
      const card = root.querySelector(`.fcard[data-fan="${fan}"]`);
      const svg = card.querySelector(".plot");
      svg.classList.toggle("off", !on && !dirty);
      svg.querySelector("g").innerHTML = plot(fan);
      const lt = fan === "cpu" ? st.cpu_temp : st.gpu.awake ? st.gpu.temp : null;
      card.querySelector(".fnum").textContent = lt != null ? `${lt}°` : "zZ";
      const rpm = fan === "cpu" ? st.rpm_cpu : st.rpm_gpu;
      card.querySelector(".frpm").textContent = rpm != null ? `${rpm} rpm` : "";
    }

    // per-profile assignment
    const as = document.getElementById("assign");
    if (as.querySelectorAll(".apill").length === 0) {
      PROFILES.forEach((p) => {
        const b = document.createElement("button");
        b.type = "button"; b.className = "apill"; b.dataset.p = p.id; b.style.setProperty("--c", p.c);
        b.innerHTML = `<span class="dot"></span><span>${p.name}</span><span class="av"></span>`;
        b.addEventListener("click", () => act("set_curve_profile", { id: p.id, on: !assigned(p.id) }));
        as.appendChild(b);
      });
    }
    as.querySelectorAll(".apill").forEach((b) => {
      const a = b.dataset.p !== "silent" && assigned(b.dataset.p);
      b.classList.toggle("on", a);
      b.querySelector(".av").textContent = a ? "PROPIA" : "FÁBRICA";
      b.disabled = !editable() || b.dataset.p === "silent";
      b.setAttribute("aria-label", `${b.querySelector("span:nth-child(2)").textContent}: curva ${a ? "propia" : "de fábrica"}`);
    });
  }

  window.renderFans = (v) => {
    last = v;
    max = v.fans.max || 150;
    if (!draft || (!dirty && !drag)) draft = copy(v.fans.curve);
    dirty = !same(draft, v.fans.curve);
    if (!document.getElementById("view-fans").hidden) paint();
  };

  // ---------- interaction ----------
  document.querySelectorAll("#view-fans .fcard").forEach((card) => {
    const fan = card.dataset.fan;
    const svg = card.querySelector(".plot");
    svg.addEventListener("pointerdown", (e) => {
      const n = e.target.closest(".node");
      if (!n || !editable()) return;
      e.preventDefault();
      drag = { fan, i: Number(n.dataset.i) };
      svg.setPointerCapture(e.pointerId);
      paint();
    });
    svg.addEventListener("pointermove", (e) => {
      if (!drag || drag.fan !== fan) return;
      const r = svg.getBoundingClientRect();
      const vy = ((e.clientY - r.top) * 300) / r.height;
      const p = Math.max(0, Math.min(150, Math.round(((BOT - vy) / H) * 150)));
      const d = copy(draft), arr = d[fan], i = drag.i;
      const lo = i > 0 ? arr[i - 1] : 0, hi = i < 6 ? arr[i + 1] : max;
      arr[i] = Math.max(lo, Math.min(hi, raw(p)));
      setDraft(d);
    });
    const end = () => { if (drag) { drag = null; paint(); } };
    svg.addEventListener("pointerup", end);
    svg.addEventListener("pointercancel", end);
  });

  document.getElementById("fApply").addEventListener("click", async () => {
    await act("set_curve", { curve: draft });
    dirty = false;
    paint();
  });
  document.getElementById("fReset").addEventListener("click", () => setDraft(copy(last.fans.factory)));
  document.getElementById("fSwitch").addEventListener("click", async () => {
    const cur = current();
    if (!cur || cur === "silent") return;
    const turnOn = !assigned(cur);
    if (turnOn && dirty) await act("set_curve", { curve: draft });
    await act("set_curve_profile", { id: cur, on: turnOn });
  });
  document.querySelectorAll(".rail button").forEach((b) => b.addEventListener("click", () => setTimeout(paint, 0)));
})();
