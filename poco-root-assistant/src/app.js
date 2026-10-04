"use strict";

const TAURI = window.__TAURI__;
const invoke = TAURI && TAURI.core ? TAURI.core.invoke : null;
const listen = TAURI && TAURI.event ? TAURI.event.listen : null;

const state = {
  toolsReady: false,
  serial: null,
  devices: [],
  info: null,
  fastboot: [],
  unlocked: "unknown",
  lastPatched: null,
  downloading: false,
};

const $ = (id) => document.getElementById(id);

function toast(msg, kind) {
  const t = $("toast");
  t.textContent = msg;
  t.className = "toast" + (kind ? " " + kind : "");
  clearTimeout(toast._t);
  toast._t = setTimeout(() => t.classList.add("hidden"), 4200);
}

function logLine(line) {
  const el = $("log");
  const time = new Date().toLocaleTimeString();
  el.textContent += `[${time}] ${line}\n`;
  el.scrollTop = el.scrollHeight;
}

async function call(cmd, args) {
  try {
    return await invoke(cmd, args);
  } catch (err) {
    const msg = typeof err === "string" ? err : JSON.stringify(err);
    toast(msg, "err");
    logLine("⚠ " + cmd + " → " + msg);
    throw err;
  }
}

// ---- rendering -----------------------------------------------------------

const BADGE = {
  auto: ["badge-auto", "Auto"],
  semi_auto: ["badge-semi", "Semi-auto"],
  manual: ["badge-manual", "À faire toi-même"],
};

function renderSteps(plan) {
  const ol = $("steps");
  ol.innerHTML = "";
  plan.forEach((s, i) => {
    const li = document.createElement("li");
    li.className = "step" + (s.automation === "auto" ? " step-auto" : "");
    const [badgeCls, badgeTxt] = BADGE[s.automation] || BADGE.manual;
    li.innerHTML = `
      <div class="step-num">${i + 1}</div>
      <div>
        <div class="step-head">
          <span class="step-title">${esc(s.title)}</span>
          <span class="badge ${badgeCls}">${badgeTxt}</span>
        </div>
        <div class="step-detail">${esc(s.detail)}</div>
        ${
          s.manual_reason
            ? `<div class="step-why"><b>Pourquoi pas 100% auto :</b> ${esc(
                s.manual_reason
              )}</div>`
            : ""
        }
      </div>`;
    ol.appendChild(li);
  });
}

function esc(s) {
  return String(s).replace(/[&<>"]/g, (c) =>
    ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" }[c])
  );
}

function renderDevice() {
  const card = $("deviceCard");
  const info = state.info;
  if (!info) {
    card.innerHTML = `<p class="muted">Branche ton POCO X6 5G en USB, débogage activé.</p>`;
    return;
  }
  const row = (k, v) => (v ? `<dt>${k}</dt><dd>${esc(v)}</dd>` : "");
  const chip =
    info.support === "target"
      ? `<span class="chip-target">✓ POCO X6 5G reconnu (garnet)</span>`
      : info.support === "generic"
      ? `<span class="chip-generic">⚠ Appareil non ciblé — procédure générique, à tes risques</span>`
      : "";
  card.innerHTML = `
    <dl class="kv">
      ${row("Modèle", info.model)}
      ${row("Nom de code", info.codename)}
      ${row("Marque", info.brand)}
      ${row("Android", info.android_version)}
      ${row("Version OS", info.os_version)}
      ${row("Build exact", info.build_incremental)}
      ${row("Patch sécurité", info.security_patch)}
      ${row("Série", info.serial)}
    </dl>
    ${chip}
    ${
      info.build_incremental
        ? `<p class="muted" style="margin-top:10px">Pour le boot.img, cherche <span class="mono">${esc(
            info.build_incremental
          )}</span> (doit correspondre EXACTEMENT).</p>`
        : ""
    }`;
}

function showProgress(p) {
  const wrap = $("progressWrap");
  wrap.classList.remove("hidden");
  $("progressBar").style.width = (p.pct || 0) + "%";
  $("progressMsg").textContent = p.message || "";
  if (p.stage === "done") {
    setTimeout(() => wrap.classList.add("hidden"), 1800);
  }
}

// ---- status + button state ----------------------------------------------

function setPill(cls, text) {
  $("statusPill").className = "pill " + cls;
  $("statusText").textContent = text;
}

function hasUnauthorized() {
  return state.devices.some((d) => d.state === "unauthorized");
}

function unlockLabel() {
  if (state.unlocked === "unlocked") return " · déverrouillé ✅";
  if (state.unlocked === "locked") return " · verrouillé 🔒";
  return "";
}

function updateUi() {
  const inFastboot = state.fastboot.length > 0;
  if (inFastboot) setPill("pill-fastboot", "Mode fastboot" + unlockLabel());
  else if (state.serial)
    setPill("pill-ok", state.info && state.info.model ? state.info.model : "Téléphone détecté");
  else if (hasUnauthorized()) setPill("pill-warn", "Autorise le débogage USB sur le téléphone");
  else if (!state.toolsReady) setPill("pill-idle", "Outils non prêts");
  else setPill("pill-idle", "Aucun téléphone");

  const dev = !!state.serial;
  enable("btnFastboot", dev);
  enable("btnCheckUnlock", state.toolsReady);
  enable("btnMagisk", dev);
  enable("btnPushBoot", dev);
  enable("btnPull", dev);
  enable("btnFlash", inFastboot && state.unlocked !== "locked");
  enable("btnRebootSys", inFastboot);
  enable("btnVerify", dev);
  enable("btnTools", !state.downloading);
  $("btnTools").textContent = state.toolsReady
    ? "Mettre à jour les outils"
    : "Préparer les outils (adb, fastboot, Magisk)";
}

function enable(id, on) {
  $(id).disabled = !on;
}

// ---- actions -------------------------------------------------------------

function bindButtons() {
  $("btnTools").onclick = async () => {
    state.downloading = true;
    updateUi();
    try {
      await call("ensure_tools");
      toast("Outils prêts ✅", "ok");
    } finally {
      state.downloading = false;
      updateUi();
    }
  };

  $("btnOpenData").onclick = () => call("open_data_folder");
  $("btnClearLog").onclick = () => ($("log").textContent = "");

  $("btnRefresh").onclick = async () => {
    const devs = await call("list_devices");
    state.devices = devs;
    pickSerial();
    if (state.serial) {
      state.info = await call("device_info", { serial: state.serial });
      renderDevice();
    }
    updateUi();
  };

  $("btnFastboot").onclick = async () => {
    await call("reboot_bootloader", { serial: state.serial });
    toast("Redémarrage en fastboot… patiente quelques secondes.", "ok");
  };

  $("btnCheckUnlock").onclick = async () => {
    const r = await call("fastboot_state");
    state.fastboot = r.serials || [];
    state.unlocked = r.unlocked || "unknown";
    updateUi();
    if (!r.serials.length) {
      toast("Aucun appareil en fastboot. Passe d'abord en mode fastboot.", "err");
      return;
    }
    const map = { unlocked: "DÉVERROUILLÉ ✅", locked: "verrouillé ❌", unknown: "inconnu" };
    toast("Bootloader : " + (map[r.unlocked] || r.unlocked), r.unlocked === "unlocked" ? "ok" : "err");
    logLine("fastboot unlocked = " + r.unlocked);
  };

  $("btnMagisk").onclick = async () => {
    const r = await call("install_magisk", { serial: state.serial });
    toast("Magisk installé. Ouvre-le et patche le boot.img.", "ok");
    logLine(r);
  };

  $("btnPushBoot").onclick = async () => {
    const r = await call("pick_and_push_boot", { serial: state.serial });
    toast(r, "ok");
  };

  $("btnPull").onclick = async () => {
    const path = await call("pull_patched_boot", { serial: state.serial });
    state.lastPatched = path;
    toast("Boot patché récupéré. Passe en fastboot puis flashe.", "ok");
    logLine("Boot patché : " + path);
    updateUi();
  };

  $("btnFlash").onclick = async () => {
    if (!state.lastPatched) {
      toast("Récupère d'abord le boot patché (bouton précédent).", "err");
      return;
    }
    const r = await call("flash_patched_boot", { imgPath: state.lastPatched, serial: null });
    toast("Flash terminé, redémarrage. 🎉", "ok");
    logLine(r);
  };

  $("btnRebootSys").onclick = async () => {
    await call("reboot_from_fastboot");
    toast("Redémarrage du téléphone…", "ok");
  };

  $("btnVerify").onclick = async () => {
    const r = await call("verify_root", { serial: state.serial });
    toast(r, "ok");
    logLine(r);
  };

  document.querySelectorAll("[data-url]").forEach((a) => {
    a.addEventListener("click", (e) => {
      e.preventDefault();
      call("open_url", { url: a.getAttribute("data-url") });
    });
  });
}

function pickSerial() {
  const d = state.devices.find((x) => x.state === "device");
  state.serial = d ? d.serial : null;
  if (!d) state.info = null;
}

// ---- init ----------------------------------------------------------------

async function init() {
  if (!invoke || !listen) {
    document.body.innerHTML =
      '<div style="padding:40px;font-family:sans-serif;color:#e7e9ef">' +
      "<h2>Lance cette page depuis l'application Poco Root Assistant.</h2>" +
      "<p>Elle a besoin du moteur Tauri (elle ne fonctionne pas dans un navigateur classique).</p></div>";
    return;
  }

  try {
    $("honest").textContent = await invoke("honest_summary");
    renderSteps(await invoke("get_plan"));
    state.toolsReady = await invoke("tools_status");
  } catch (e) {
    logLine("Init: " + e);
  }

  await listen("tools", (e) => {
    state.toolsReady = !!(e.payload && e.payload.ready);
    updateUi();
  });
  await listen("tool-progress", (e) => showProgress(e.payload || {}));
  await listen("devices", (e) => {
    state.devices = e.payload || [];
    pickSerial();
    updateUi();
  });
  await listen("device-info", (e) => {
    state.info = e.payload || null;
    if (state.info) state.serial = state.info.serial;
    renderDevice();
    updateUi();
  });
  await listen("fastboot", (e) => {
    state.fastboot = (e.payload && e.payload.serials) || [];
    state.unlocked = (e.payload && e.payload.unlocked) || "unknown";
    updateUi();
  });
  await listen("log", (e) => logLine((e.payload && e.payload.line) || ""));

  bindButtons();
  renderDevice();
  updateUi();
  logLine("Prêt. Branche ton POCO X6 5G en USB (débogage activé).");
}

window.addEventListener("DOMContentLoaded", init);
