const mount = document.getElementById("agentflow-debugger");
const state = {
  runId: "",
  runs: [],
  run: null,
  events: [],
  selectedSeq: null,
  connection: "idle",
  error: null,
  source: null,
};

const formatTime = (value) => {
  if (!value) return "pending";
  return new Intl.DateTimeFormat(undefined, {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  }).format(new Date(value));
};

const runFromEnvelope = (value) => value.run ?? value;

const eventTone = (kind) => {
  const lower = kind.toLowerCase();
  if (lower.includes("fail") || lower.includes("error")) return "danger";
  if (lower.includes("tool")) return "tool";
  if (lower.includes("agent") || lower.includes("reflect") || lower.includes("plan")) return "agent";
  if (lower.includes("complete") || lower.includes("succeed")) return "success";
  return "neutral";
};

const escapeHtml = (value) =>
  String(value)
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;");

const eventNodeName = (event) => {
  const payload = event.payload && typeof event.payload === "object" ? event.payload : {};
  return String(payload.node_name ?? payload.node ?? payload.step ?? event.kind).trim() || event.kind;
};

const nodeSummaries = () => {
  const seen = new Map();
  for (const event of state.events) {
    const name = eventNodeName(event);
    seen.set(name, { name, status: event.kind, tone: eventTone(event.kind) });
  }
  return Array.from(seen.values()).slice(-8);
};

const selectedEvent = () =>
  state.events.find((event) => event.seq === state.selectedSeq) ?? state.events.at(-1) ?? null;

const findLatest = (items, predicate) => {
  for (let index = items.length - 1; index >= 0; index -= 1) {
    if (predicate(items[index])) return items[index];
  }
  return undefined;
};

const setState = (patch) => {
  Object.assign(state, patch);
  render();
};

const connect = async () => {
  if (!state.runId.trim()) return;
  state.source?.close();
  setState({ connection: "loading", error: null, events: [], selectedSeq: null });

  try {
    const response = await fetch(`/v1/runs/${encodeURIComponent(state.runId)}`);
    if (!response.ok) {
      throw new Error(`run lookup failed with HTTP ${response.status}`);
    }
    const payload = await response.json();
    setState({
      run: runFromEnvelope(payload),
      connection: "streaming",
      error: null,
    });
    window.history.replaceState(null, "", `/ui?run=${encodeURIComponent(state.runId)}`);

    const source = new EventSource(`/v1/runs/${encodeURIComponent(state.runId)}/events`);
    source.onmessage = (message) => {
      const event = JSON.parse(message.data);
      const exists = state.events.some((item) => item.seq === event.seq);
      if (!exists) {
        state.events = [...state.events, event].sort((left, right) => left.seq - right.seq);
      }
      state.selectedSeq ??= event.seq;
      render();
    };
    source.onerror = () => {
      source.close();
      if (state.connection === "streaming") {
        setState({ connection: "closed" });
      }
    };
    state.source = source;
  } catch (error) {
    setState({
      connection: "error",
      error: error instanceof Error ? error.message : String(error),
    });
  }
};

const loadRuns = async () => {
  try {
    const response = await fetch("/v1/runs?limit=20");
    if (!response.ok) return;
    const payload = await response.json();
    state.runs = payload.runs ?? [];
    const shouldConnect = !state.runId && state.runs[0];
    if (!state.runId && state.runs[0]) {
      state.runId = state.runs[0].id;
    }
    render();
    if (shouldConnect) {
      connect();
    }
  } catch {
    // Explicit run-id connection still works when listing is unavailable.
  }
};

const render = () => {
  const nodes = nodeSummaries();
  const selected = selectedEvent();
  mount.innerHTML = `
    <main class="shell">
      <header class="topbar">
        <div>
          <p class="eyebrow">AgentFlow</p>
          <h1>Hybrid Run Debugger</h1>
        </div>
        <form class="run-form" data-run-form>
          <input aria-label="Run ID" value="${escapeHtml(state.runId)}" placeholder="Run ID" />
          <button type="submit">Connect</button>
        </form>
      </header>
      <section class="status-strip" aria-label="Run status">
        <div><span>State</span><strong>${escapeHtml(state.connection)}</strong></div>
        <div><span>Status</span><strong>${escapeHtml(state.run?.status ?? "none")}</strong></div>
        <div><span>Tenant</span><strong>${escapeHtml(state.run?.tenant_id ?? "default")}</strong></div>
        <div><span>Events</span><strong>${state.events.length}</strong></div>
      </section>
      ${state.error ? `<p class="error-line">${escapeHtml(state.error)}</p>` : ""}
      <section class="workspace">
        <aside class="run-pane">
          <div class="pane-heading"><span>Runs</span><strong>${escapeHtml(state.run ? formatTime(state.run.started_at) : "-")}</strong></div>
          <ol class="run-list">
            ${state.runs
              .map(
                (run) => `
                  <li>
                    <button class="${run.id === state.runId ? "selected" : ""}" type="button" data-run-id="${escapeHtml(run.id)}">
                      <span>${escapeHtml((run.workflow ?? "").split("\n")[0] || run.id)}</span>
                      <small>${escapeHtml(run.status)} · ${escapeHtml(formatTime(run.started_at))}</small>
                    </button>
                  </li>`,
              )
              .join("")}
          </ol>
          <pre class="workflow-preview">${escapeHtml(state.run?.workflow ?? "No run loaded.")}</pre>
        </aside>
        <section class="graph-pane" aria-label="DAG status">
          <div class="pane-heading"><span>DAG</span><strong>${nodes.length} nodes</strong></div>
          <div class="node-grid">
            ${
              nodes.length === 0
                ? '<div class="empty-node">Waiting for events</div>'
                : nodes
                    .map(
                      (node) => `
                        <button class="node node-${node.tone}" type="button" data-node="${escapeHtml(node.name)}">
                          <span>${escapeHtml(node.name)}</span>
                          <small>${escapeHtml(node.status)}</small>
                        </button>`,
                    )
                    .join("")
            }
          </div>
        </section>
        <aside class="timeline-pane" aria-label="Agent timeline">
          <div class="pane-heading"><span>Timeline</span><strong>${selected ? `#${selected.seq}` : "-"}</strong></div>
          <ol class="timeline">
            ${state.events
              .map(
                (event) => `
                  <li>
                    <button class="${state.selectedSeq === event.seq ? "selected" : ""}" type="button" data-seq="${event.seq}">
                      <span class="dot dot-${eventTone(event.kind)}"></span>
                      <span>${escapeHtml(event.kind)}</span>
                      <time>${escapeHtml(formatTime(event.ts))}</time>
                    </button>
                  </li>`,
              )
              .join("")}
          </ol>
        </aside>
      </section>
      <section class="details-pane" aria-label="Tool call details">
        <div class="pane-heading"><span>Details</span><strong>${escapeHtml(selected?.kind ?? "none")}</strong></div>
        <pre>${escapeHtml(selected ? JSON.stringify(selected.payload, null, 2) : "Select an event.")}</pre>
      </section>
    </main>`;

  const form = mount.querySelector("[data-run-form]");
  const input = form.querySelector("input");
  input.addEventListener("input", (event) => {
    state.runId = event.target.value;
  });
  form.addEventListener("submit", (event) => {
    event.preventDefault();
    connect();
  });
  for (const button of mount.querySelectorAll("[data-seq]")) {
    button.addEventListener("click", () => setState({ selectedSeq: Number(button.dataset.seq) }));
  }
  for (const button of mount.querySelectorAll("[data-run-id]")) {
    button.addEventListener("click", () => {
      state.runId = button.dataset.runId ?? "";
      connect();
    });
  }
  for (const button of mount.querySelectorAll("[data-node]")) {
    button.addEventListener("click", () => {
      const match = findLatest(state.events, (event) => eventNodeName(event) === button.dataset.node);
      setState({ selectedSeq: match?.seq ?? null });
    });
  }
};

const params = new URLSearchParams(window.location.search);
state.runId = params.get("run") ?? "";
render();
loadRuns();
if (state.runId) {
  connect();
}
