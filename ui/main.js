// Tauri desktop UI – task-first, no marketing copy.
// Uses window.__TAURI__.core.invoke in bundled context; browser fallback is read-only.

const isTauri = !!(window.__TAURI__ && window.__TAURI__.core && window.__TAURI__.core.invoke);

function invoke(cmd, args) {
  if (isTauri) return window.__TAURI__.core.invoke(cmd, args || {});
  return Promise.reject(new Error("Tauri not available – open the desktop app"));
}

const $ = (s) => document.querySelector(s);
const $$ = (s) => Array.from(document.querySelectorAll(s));

function showMessage(text, kind) {
  const el = $("#globalMessage");
  el.textContent = text;
  el.className = "message " + (kind || "info");
  el.classList.remove("hidden");
  setTimeout(() => el.classList.add("hidden"), 6000);
}
function setHint(el, text) {
  if (el) el.textContent = text || "";
}

let opsOffset = 0;
let auditOffset = 0;

// Tabs
$$(".tab").forEach((btn) => {
  btn.addEventListener("click", () => {
    const tab = btn.dataset.tab;
    $$(".tab").forEach((b) => {
      b.classList.toggle("active", b === btn);
      b.setAttribute("aria-selected", b === btn ? "true" : "false");
    });
    $$(".panel").forEach((p) => p.classList.remove("active"));
    const panel = document.getElementById("panel-" + tab);
    if (panel) panel.classList.add("active");
    if (tab === "history") {
      loadOps();
      loadAudit();
    }
  });
});

function setFallback() {
  if (!isTauri) {
    const b = $("#fallbackBanner");
    if (b) b.classList.remove("hidden");
    ["#btnSaveSettings","#btnSaveToken","#btnClearToken","#btnTestUrl"].forEach((sel) => {
      const el = $(sel);
      if (el) el.disabled = true;
    });
  }
}

function updateNetworkUI() {
  const mode = $("#networkMode")?.value || "loopback";
  const show = mode === "private-overlay";
  const warn = $("#privateWarning");
  const ackRow = $("#ackRow");
  if (warn) warn.classList.toggle("hidden", !show);
  if (ackRow) ackRow.classList.toggle("hidden", !show);
}

async function loadSettings() {
  try {
    const s = await invoke("get_settings");
    $("#stockUrl").value = s.stock_service_url || "";
    $("#bindAddr").value = s.bind_addr || "";
    $("#mcpPath").value = s.mcp_path || "";
    $("#bundleId").value = s.target_bundle_id || "";
    $("#processName").value = s.target_process_name || "";
    $("#maxDepth").value = s.max_depth;
    $("#maxNodes").value = s.max_nodes;
    if ($("#networkMode")) {
      $("#networkMode").value = s.network_mode || "loopback";
      updateNetworkUI();
    }
    // ack checkbox is not persisted; require re-ack each time private mode is chosen
    if ($("#privateAck")) $("#privateAck").checked = false;
    $("#instanceBadge").textContent = (s.instance_id || "").slice(0, 8) || "—";
  } catch (e) {
    showMessage("Failed to load settings: " + (e?.message || e), "err");
  }
}

async function loadStatus() {
  try {
    const st = await invoke("get_runtime_status");
    const tokenEl = $("#tokenConfigured");
    const sourceEl = $("#tokenSource");
    tokenEl.textContent = st.token_configured ? "configured" : "not configured";
    tokenEl.style.background = st.token_configured ? "#dcfce7" : "#fee2e2";
    tokenEl.style.borderColor = st.token_configured ? "#bbf7d0" : "#fecaca";
    sourceEl.textContent = "(" + st.token_source + ")";
    const dot = $("#serverDot");
    const label = $("#serverLabel");
    if (st.server_running) {
      dot.className = "dot ok";
      label.textContent = "server running";
    } else {
      dot.className = "dot err";
      label.textContent = st.server_error ? "server " + st.server_error : "server not running";
    }
    $("#stServer").textContent = st.server_running ? "running" : (st.server_error || "stopped");
    $("#stServer").className = "v " + (st.server_running ? "ok-text" : "err-text");
    $("#stBind").textContent = st.server_bind_addr || $("#bindAddr").value || "—";
    $("#stMcp").textContent = $("#mcpPath").value || "—";
    const ax = st.accessibility;
    $("#stAx").textContent = ax.process_trusted ? "trusted" : "not trusted";
    $("#stAx").className = "v " + (ax.process_trusted ? "ok-text" : "warn-text");
    $("#stTarget").textContent = ax.target_found ? (ax.target_pid ? "pid " + ax.target_pid : "found") : "not found";
    $("#stTarget").className = "v " + (ax.target_found ? "ok-text" : "warn-text");
    $("#stRestart").textContent = st.restart_required ? "required" : "no";
    $("#stRestart").className = "v " + (st.restart_required ? "warn-text" : "ok-text");
    $("#stDb").textContent = st.db_path || "—";
    $("#restartReasons").textContent = (st.restart_reasons || []).join("; ");
    $("#axNotes").textContent = (ax.notes || []).join("; ");
    $("#instanceBadge").textContent = (st.instance_id || "").slice(0, 8) || $("#instanceBadge").textContent;
  } catch (e) {
    showMessage("Status failed: " + (e?.message || e), "err");
  }
}

async function saveSettings() {
  const req = {
    stock_service_url: $("#stockUrl").value.trim() ? $("#stockUrl").value.trim() : null,
    bind_addr: $("#bindAddr").value.trim(),
    mcp_path: $("#mcpPath").value.trim(),
    target_bundle_id: $("#bundleId").value.trim(),
    target_process_name: $("#processName").value.trim(),
    max_depth: parseInt($("#maxDepth").value, 10),
    max_nodes: parseInt($("#maxNodes").value, 10),
    network_mode: $("#networkMode") ? $("#networkMode").value : "loopback",
    private_overlay_ack: $("#privateAck") ? $("#privateAck").checked : false,
  };
  try {
    const resp = await invoke("save_settings", { request: req });
    showMessage(resp.message || "Saved", resp.restart_required ? "info" : "ok");
    await loadStatus();
    await loadSettings();
  } catch (e) {
    showMessage("Save failed: " + (typeof e === "string" ? e : e?.message || JSON.stringify(e)), "err");
  }
}

async function testUrl() {
  const url = $("#stockUrl").value.trim();
  if (!url) {
    showMessage("Enter a URL to test", "err");
    return;
  }
  setHint($("#testResult"), "testing…");
  try {
    const r = await invoke("test_stock_service_url", { url });
    setHint($("#testResult"), r.message + (r.latency_ms ? " (" + r.latency_ms + " ms)" : ""));
    $("#testResult").style.color = r.ok ? "var(--ok)" : "var(--danger)";
  } catch (e) {
    setHint($("#testResult"), "failed: " + (e?.message || e));
  }
}

async function saveToken() {
  const token = $("#newToken").value;
  if (!token.trim()) {
    showMessage("Token is empty", "err");
    return;
  }
  try {
    await invoke("save_token", { token });
    $("#newToken").value = "";
    showMessage("Token saved to Keychain", "ok");
    await loadStatus();
  } catch (e) {
    showMessage("Save token failed: " + (typeof e === "string" ? e : e?.message || e), "err");
  }
}
async function clearToken() {
  try {
    await invoke("clear_token");
    showMessage("Token cleared", "ok");
    await loadStatus();
  } catch (e) {
    showMessage("Clear failed: " + (e?.message || e), "err");
  }
}

function fmtTime(iso) {
  if (!iso) return "—";
  try {
    const d = new Date(iso);
    return d.toLocaleString();
  } catch { return iso; }
}
function shortFp(fp) {
  if (!fp) return "—";
  return fp.slice(0, 10) + "…";
}
function payloadSummary(v) {
  if (!v) return "—";
  try {
    if (typeof v === "string") return v.slice(0, 80);
    const s = JSON.stringify(v);
    return s.length > 100 ? s.slice(0, 100) + "…" : s;
  } catch { return String(v).slice(0, 80); }
}

function clearBody(tbody) {
  while (tbody.firstChild) tbody.removeChild(tbody.firstChild);
}
function mkCell(text, title) {
  const td = document.createElement("td");
  td.textContent = text;
  if (title !== undefined && title !== null) td.setAttribute("title", title);
  return td;
}
function mkEmptyRow(colspan, text) {
  const tr = document.createElement("tr");
  const td = document.createElement("td");
  td.colSpan = colspan;
  td.className = "empty";
  td.textContent = text;
  tr.appendChild(td);
  return tr;
}

async function resolveStale(opId, currentState) {
  const confirmed = window.confirm(
    `Resolve stale operation ${opId.slice(0, 8)}… (${currentState})?\n\n` +
    `Only proceed if you have verified the broker confirmation dialog is CLOSED and the operation will not be submitted.\n\n` +
    `This will write an auditable 'stale_resolved' event, move the operation to 'aborted', clear its in-memory token, and unblock new prepares.`
  );
  if (!confirmed) return;
  try {
    const res = await invoke("resolve_stale_operation", { operation_id: opId, operationId: opId });
    showMessage(res.message || `Resolved ${opId.slice(0, 8)}…`, "ok");
    await loadOps();
    await loadAudit();
  } catch (e) {
    showMessage("Resolve failed: " + (typeof e === "string" ? e : e?.message || JSON.stringify(e)).slice(0, 300), "err");
  }
}

async function loadOps() {
  const kind = $("#fKind").value || null;
  const state = $("#fState").value || null;
  const limit = parseInt($("#fLimit").value, 10) || 20;
  const hint = $("#opsMeta");
  setHint(hint, "loading…");
  const body = $("#opsBody");
  try {
    const resp = await invoke("list_operations", { limit, offset: opsOffset, kind, stateFilter: state });
    const total = resp.total;
    const ops = resp.operations || [];
    setHint(hint, total + " total");
    $("#opsPage").textContent = "page " + (Math.floor(opsOffset / limit) + 1) + " · " + ops.length + " rows";
    clearBody(body);
    if (ops.length === 0) {
      body.appendChild(mkEmptyRow(7, "No operations"));
    } else {
      for (const o of ops) {
        const tr = document.createElement("tr");
        tr.appendChild(mkCell(fmtTime(o.created_at)));
        tr.appendChild(mkCell(o.kind || "—"));
        tr.appendChild(mkCell(o.state || "—"));
        tr.appendChild(mkCell(shortFp(o.fingerprint), o.fingerprint || ""));
        const payloadText = payloadSummary(o.payload_summary);
        const payloadTitle = (() => { try { return JSON.stringify(o.payload_summary); } catch { return payloadText; }})();
        tr.appendChild(mkCell(payloadText, payloadTitle));
        tr.appendChild(mkCell(o.actor_source || "—"));
        const actionTd = document.createElement("td");
        if (o.state === "unknown" || o.state === "expired") {
          const btn = document.createElement("button");
          btn.textContent = "Resolve stale";
          btn.className = "danger";
          btn.title = "Mark dialog closed and unblock new operations (audited)";
          btn.addEventListener("click", () => resolveStale(o.operation_id || o.id, o.state));
          actionTd.appendChild(btn);
        } else {
          actionTd.textContent = "—";
          actionTd.className = "hint";
        }
        tr.appendChild(actionTd);
        body.appendChild(tr);
      }
    }
    body.dataset.total = String(total);
  } catch (e) {
    setHint(hint, "failed");
    clearBody(body);
    body.appendChild(mkEmptyRow(7, "Failed: " + String(e?.message || e).slice(0, 200)));
  }
}

async function loadAudit() {
  const limit = parseInt($("#fAuditLimit").value, 10) || 20;
  const operationId = $("#fOpId").value.trim() || null;
  const hint = $("#auditMeta");
  setHint(hint, "loading…");
  const body = $("#auditBody");
  try {
    const resp = await invoke("list_audit_events", { limit, offset: auditOffset, operationId });
    const total = resp.total;
    const events = resp.events || [];
    setHint(hint, total + " total");
    $("#auditPage").textContent = "page " + (Math.floor(auditOffset / limit) + 1) + " · " + events.length + " rows";
    clearBody(body);
    if (events.length === 0) {
      body.appendChild(mkEmptyRow(6, "No events"));
    } else {
      for (const ev of events) {
        const tr = document.createElement("tr");
        tr.appendChild(mkCell(fmtTime(ev.created_at)));
        const opShort = ev.operation_id ? ev.operation_id.slice(0, 8) + "…" : "—";
        tr.appendChild(mkCell(opShort, ev.operation_id || ""));
        tr.appendChild(mkCell(ev.event_type || "—"));
        tr.appendChild(mkCell((ev.from_state || "—") + " → " + (ev.to_state || "—")));
        tr.appendChild(mkCell(ev.actor_source || "—"));
        tr.appendChild(mkCell(payloadSummary(ev.detail)));
        body.appendChild(tr);
      }
    }
    body.dataset.total = String(total);
  } catch (e) {
    setHint(hint, "failed");
    clearBody(body);
    body.appendChild(mkEmptyRow(6, "Failed: " + String(e?.message || e).slice(0, 200)));
  }
}

function attachPager() {
  $("#opsPrev").addEventListener("click", () => {
    opsOffset = Math.max(0, opsOffset - (parseInt($("#fLimit").value, 10) || 20));
    loadOps();
  });
  $("#opsNext").addEventListener("click", () => {
    const limit = parseInt($("#fLimit").value, 10) || 20;
    const total = parseInt($("#opsBody").dataset.total || "0", 10);
    if (opsOffset + limit < total) {
      opsOffset += limit;
      loadOps();
    }
  });
  $("#auditPrev").addEventListener("click", () => {
    auditOffset = Math.max(0, auditOffset - (parseInt($("#fAuditLimit").value, 10) || 20));
    loadAudit();
  });
  $("#auditNext").addEventListener("click", () => {
    const limit = parseInt($("#fAuditLimit").value, 10) || 20;
    const total = parseInt($("#auditBody").dataset.total || "0", 10);
    if (auditOffset + limit < total) {
      auditOffset += limit;
      loadAudit();
    }
  });
  ["#fKind","#fState","#fLimit"].forEach((sel) => {
    const el = $(sel);
    if (el) el.addEventListener("change", () => { opsOffset = 0; loadOps(); });
  });
  const fOp = $("#fOpId");
  if (fOp) fOp.addEventListener("change", () => { auditOffset = 0; });
}

document.addEventListener("DOMContentLoaded", async () => {
  setFallback();
  attachPager();
  $("#btnSaveSettings")?.addEventListener("click", saveSettings);
  $("#btnTestUrl")?.addEventListener("click", testUrl);
  $("#btnSaveToken")?.addEventListener("click", saveToken);
  $("#btnClearToken")?.addEventListener("click", clearToken);
  $("#btnRefreshStatus")?.addEventListener("click", loadStatus);
  $("#btnLoadOps")?.addEventListener("click", () => { opsOffset = 0; loadOps(); });
  $("#btnLoadAudit")?.addEventListener("click", () => { auditOffset = 0; loadAudit(); });
  $("#networkMode")?.addEventListener("change", updateNetworkUI);

  if (isTauri) {
    await loadSettings();
    await loadStatus();
    loadOps();
    loadAudit();
    try {
      const tauriEvent = window.__TAURI__?.event;
      if (tauriEvent && tauriEvent.listen) {
        await tauriEvent.listen("single-instance", () => {
          loadStatus();
          const main = document.querySelector(".tab[data-tab='connection']");
          if (main) main.click();
        });
      }
    } catch {}
  } else {
    showMessage("Browser preview – open the Tauri desktop app for full functionality", "info");
  }
});
