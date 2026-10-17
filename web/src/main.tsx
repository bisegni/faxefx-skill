import React, { useCallback, useEffect, useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import {
  ReactFlow,
  Background,
  Controls,
  MiniMap,
  Handle,
  Position,
  MarkerType,
  type Connection,
  type NodeProps,
} from "@xyflow/react";
import "@xyflow/react/dist/style.css";
import "./style.css";
import { Audition } from "./Audition";
import { Performance } from "./Performance";
import {
  canConnect,
  logicalPositions,
  removeNode,
  type Design,
  type DesignNode,
  type Setting,
} from "./graph";

// Read the current fragment on each request so a replacement session URL also
// works when the browser performs a fragment-only navigation after a restart.
const sessionToken = () => location.hash.slice(1);
async function api(path: string, body?: unknown): Promise<any> {
  const response = await fetch("/api/" + path, {
    method: body === undefined ? "GET" : "POST",
    headers: {
      Authorization: "Bearer " + sessionToken(),
      ...(body === undefined ? {} : { "Content-Type": "application/json" }),
    },
    ...(body === undefined ? {} : { body: JSON.stringify(body) }),
  });
  const data = await response.json();
  if (!response.ok)
    throw new Error(
      `${data.error?.code ?? response.status}: ${data.error?.message ?? "Request failed"}`,
    );
  return data;
}
async function blob(path: string) {
  const r = await fetch("/api/" + path, {
    headers: { Authorization: "Bearer " + sessionToken() },
  });
  if (!r.ok) {
    const e = await r.json();
    throw new Error(e.error.message);
  }
  return r.blob();
}
function download(data: Blob, name: string) {
  const url = URL.createObjectURL(data);
  const a = document.createElement("a");
  a.href = url;
  a.download = name;
  a.click();
  setTimeout(() => URL.revokeObjectURL(url), 2000);
}
function Block({ data, selected }: NodeProps) {
  return (
    <div
      className={
        "tone-block family-" +
        String(data.family) +
        (selected ? " selected" : "") +
        (data.bypass ? " bypassed" : "")
      }
    >
      <Handle type="target" position={Position.Left} />
      <span className="block-type">{String(data.family)}</span>
      <strong>{String(data.label)}</strong>
      <small>{String(data.detail ?? "Settings not specified")}</small>
      <span className="block-status">
        {data.bypass ? "Bypassed" : "Enabled"} · {String(data.channel ?? "A")}
      </span>
      <Handle type="source" position={Position.Right} />
    </div>
  );
}
const nodeTypes = { tone: Block };
function App() {
  const [context, setContext] = useState<any>();
  const [design, setDesign] = useState<Design>();
  const [revision, setRevision] = useState("");
  const [dirty, setDirty] = useState(false);
  const [remote, setRemote] = useState(false);
  const [selected, setSelected] = useState("");
  const [tab, setTab] = useState("logical");
  const [compiled, setCompiled] = useState<any>();
  const [message, setMessage] = useState("");
  const [busy, setBusy] = useState(false);
  const [palette, setPalette] = useState("amp");
  const [advanced, setAdvanced] = useState("");
  const [audioId, setAudioId] = useState("");
  const [audioUrl, setAudioUrl] = useState("");
  const [excerpt, setExcerpt] = useState({
    start: 0,
    duration: 30,
    separate: false,
  });
  const [job, setJob] = useState<any>();
  const [analysis, setAnalysis] = useState<any>();
  const [reference, setReference] = useState<any>();
  const [comparison, setComparison] = useState<any>();
  const [history, setHistory] = useState<Design[]>([]);
  const [future, setFuture] = useState<Design[]>([]);
  const [scene, setScene] = useState(1);
  const [channel, setChannel] = useState("A");
  const [paramName, setParamName] = useState("");
  const [paramValue, setParamValue] = useState("");
  const [modal, setModal] = useState("");
  const current = useRef({ dirty, revision });
  current.current = { dirty, revision };
  const dragStart = useRef<Design | null>(null);
  const accept = useCallback((state: any) => {
    setContext(state);
    setDesign(state.design);
    setRevision(state.revision);
    setDirty(false);
    setRemote(false);
    setCompiled(undefined);
    setHistory([]);
    setFuture([]);
    setMessage("");
  }, []);
  useEffect(() => {
    api("state")
      .then(accept)
      .catch((e) => setMessage(e.message));
  }, [accept]);
  useEffect(() => {
    const interval = setInterval(
      () =>
        api("state")
          .then((state) => {
            if (state.revision !== current.current.revision) {
              if (current.current.dirty) setRemote(true);
              else accept(state);
            }
          })
          .catch(() => {}),
      4000,
    );
    return () => clearInterval(interval);
  }, [accept]);
  useEffect(() => {
    if (!job || job.status !== "running") return;
    const timer = setInterval(
      () =>
        api("jobs/" + job.id)
          .then((j) => {
            setJob(j);
            if (j.status === "completed") {
              setAnalysis(j.result.data);
              setMessage(
                "Analysis ready. Listen to the target and review the measurements.",
              );
            } else if (j.status === "failed")
              setMessage(j.result.error?.message ?? "Analysis failed");
          })
          .catch((e) => setMessage(e.message)),
      750,
    );
    return () => clearInterval(timer);
  }, [job]);
  useEffect(
    () => () => {
      if (audioUrl) URL.revokeObjectURL(audioUrl);
    },
    [audioUrl],
  );
  useEffect(() => {
    if (design && selected) {
      const n = design.nodes.find((n) => n.id === selected);
      if (n)
        setChannel(
          design.scenes?.find((s) => s.number === scene)?.channels?.[
            selected
          ] ??
            n.active_channel ??
            "A",
        );
    }
  }, [scene, selected]);
  function change(next: Design) {
    if (!design) return;
    setHistory((h) => [...h.slice(-49), structuredClone(design)]);
    setFuture([]);
    setDesign(next);
    setDirty(true);
    setCompiled(undefined);
  }
  function editNode(patch: Partial<DesignNode>) {
    if (design)
      change({
        ...design,
        nodes: design.nodes.map((n) =>
          n.id === selected ? { ...n, ...patch } : n,
        ),
      });
  }
  function undo() {
    if (!design || !history.length) return;
    setFuture((f) => [design, ...f]);
    setDesign(history.at(-1));
    setHistory((h) => h.slice(0, -1));
    setDirty(true);
    setCompiled(undefined);
  }
  function redo() {
    if (!design || !future.length) return;
    setHistory((h) => [...h, design]);
    setDesign(future[0]);
    setFuture((f) => f.slice(1));
    setDirty(true);
    setCompiled(undefined);
  }
  async function save() {
    if (!design) return;
    const result = await api("revision", { base: revision, design });
    setRevision(result.revision);
    setDirty(false);
    setRemote(false);
    return result.revision;
  }
  async function run(action: () => Promise<unknown>) {
    setBusy(true);
    setMessage("");
    try {
      await action();
    } catch (e) {
      setMessage((e as Error).message);
    } finally {
      setBusy(false);
    }
  }
  async function compile() {
    const rev = dirty ? await save() : revision;
    const result = await api("compile", { revision: rev });
    setCompiled(result);
    setMessage(
      result.buildable
        ? "Simulation preview compiled. Hardware has not been changed."
        : "Draft retained. Resolve the listed hardware or routing blockers.",
    );
  }
  function addBlock() {
    if (!design) return;
    let i = 1;
    while (design.nodes.some((n) => n.id === palette.replaceAll("-", "") + i))
      i++;
    const id = palette.replaceAll("-", "") + i;
    change({
      ...design,
      nodes: [
        ...design.nodes,
        {
          id,
          type: palette,
          label:
            context.catalog.types.find((t: any) => t.alias === palette)?.name ??
            palette,
          parameters: {},
          channels: {},
          bypass: false,
        },
      ],
    });
    setSelected(id);
  }
  function connect(c: Connection) {
    if (design && c.source && c.target) {
      if (canConnect(design, c.source, c.target))
        change({
          ...design,
          edges: [...design.edges, { from: c.source, to: c.target }],
        });
      else
        setMessage(
          "That cable duplicates a route or creates a feedback cycle. Use explicit feedback blocks.",
        );
    }
  }
  async function exportImage(format: string) {
    if (dirty) await save();
    const source = await blob("render?view=" + tab + "&scene=" + scene);
    if (format === "svg") {
      download(source, "preset.svg");
      return;
    }
    const url = URL.createObjectURL(source);
    try {
      const img = new Image();
      img.src = url;
      await img.decode();
      const canvas = document.createElement("canvas");
      canvas.width = img.width * 2;
      canvas.height = img.height * 2;
      const ctx = canvas.getContext("2d")!;
      ctx.scale(2, 2);
      ctx.drawImage(img, 0, 0);
      canvas.toBlob((b) => b && download(b, "preset.png"));
    } finally {
      URL.revokeObjectURL(url);
    }
  }
  async function importFile(file: File) {
    const response = await fetch("/api/audio/import", {
      method: "POST",
      headers: { Authorization: "Bearer " + sessionToken(), "X-Filename": encodeURIComponent(file.name) },
      body: file,
    });
    const data = await response.json();
    if (!response.ok) throw new Error(data.error.message);
    setAudioId(data.audio_id);
    setAnalysis(undefined);
    setAudioUrl(URL.createObjectURL(await blob("audio/" + data.audio_id)));
    setMessage(
      "Audio stays on this computer. Choose an excerpt and target guitar part.",
    );
  }
  async function showStem() {
    const id = analysis?.processing?.separation?.guitar_audio_id;
    if (id) setAudioUrl(URL.createObjectURL(await blob("audio/" + id)));
  }
  if (!design || !context)
    return (
      <main className="loading">
        <h1>Tone workbench</h1>
        <p>{message || "Opening your local design…"}</p>
        <p>
          Use the complete session URL printed by <code>design open</code>.
        </p>
      </main>
    );
  const node = design.nodes.find((n) => n.id === selected);
  const positions = logicalPositions(design);
  const physical =
    tab === "captured" ? design.captured?.current : compiled?.physical;
  const selectedScene = design.scenes?.find((s) => s.number === scene);
  const graphNodes =
    tab === "logical"
      ? design.nodes
      : (physical?.blocks ?? [])
          .filter((b: any) => b.position)
          .map((b: any) => ({ ...b, id: b.ref, label: b.ref, parameters: {} }));
  const nodes = graphNodes.map((n: any) => ({
    id: n.id,
    type: "tone",
    position:
      tab === "logical"
        ? positions[n.id]
        : { x: (n.position.column - 1) * 200, y: (n.position.row - 1) * 125 },
    selected: n.id === selected,
    data: {
      label: n.label ?? n.id,
      family: n.type,
      detail: n.model ?? n.rationale ?? "",
      channel: selectedScene?.channels?.[n.id] ?? n.active_channel ?? "A",
      bypass: selectedScene?.bypass?.[n.id] ?? n.bypass,
    },
    draggable: tab === "logical",
  }));
  const graphEdges =
    tab === "logical" ? design.edges : (physical?.connections ?? []);
  const edges = graphEdges.map((e: any) => ({
    id: e.from + "->" + e.to,
    source: e.from,
    target: e.to,
    type: "smoothstep",
    markerEnd: { type: MarkerType.ArrowClosed, color: "#8194a8" },
    style: { stroke: "#8194a8", strokeWidth: 2 },
  }));
  const values = {
    ...(node?.channels?.[channel] ?? {}),
    ...(channel === (node?.active_channel ?? "A")
      ? (node?.parameters ?? {})
      : {}),
  };
  const typeDef = context.catalog.types.find(
    (t: any) => t.alias === node?.type,
  );
  return (
    <div className="app-shell">
      <header className="header">
        <div className="brand">
          <span className="brand-symbol">⌁</span>
          <div>
            <strong>Tone workbench</strong>
            <small>Axe-Fx III · local design studio</small>
          </div>
        </div>
        <div className="session-state">
          <span
            className={
              "dot " + (context.backend === "mock" ? "simulation" : "blocked")
            }
          />
          {context.backend === "mock"
            ? "Simulation target"
            : "USB target · authoring gates pending"}
        </div>
        <button onClick={() => setModal("protocol")}>Protocol coverage</button>
      </header>
      <section className="titlebar">
        <div>
          <span className="eyeline">
            {design.mode === "refine"
              ? "Refining a capture"
              : "Designing a preset"}
          </span>
          <input
            aria-label="Preset name"
            className="preset-name"
            value={design.name}
            onChange={(e) => change({ ...design, name: e.target.value })}
          />
          <small className="revision">
            Revision {revision.slice(0, 10)} ·{" "}
            {dirty ? "Unsaved draft" : "Saved locally"}
          </small>
        </div>
        <div className="title-actions">
          <button disabled={!history.length} onClick={undo} title="Undo">
            ↶ Undo
          </button>
          <button disabled={!future.length} onClick={redo}>
            ↷ Redo
          </button>
          <button disabled={!dirty || busy} onClick={() => run(save)}>
            Save draft
          </button>
          <button
            className="primary"
            disabled={busy}
            onClick={() => run(compile)}
          >
            Compile preview
          </button>
        </div>
      </section>
      {remote && (
        <div className="notice warning">
          Chat or another editor saved a newer revision. Your draft is retained.{" "}
          <button
            onClick={() =>
              download(
                new Blob([JSON.stringify(design, null, 2)], {
                  type: "application/json",
                }),
                "draft-backup.json",
              )
            }
          >
            Download your draft
          </button>
          <button
            onClick={() => {
              if (
                confirm(
                  "Replace your unsaved draft with the latest saved revision?",
                )
              )
                run(async () => accept(await api("state")));
            }}
          >
            Load latest
          </button>
        </div>
      )}
      {message && (
        <div className="notice" role="status">
          {message}
          <button aria-label="Dismiss message" onClick={() => setMessage("")}>
            ×
          </button>
        </div>
      )}
      <div className="workspace">
        <aside className="palette">
          <h2>Signal building blocks</h2>
          <p>Choose a role, then connect its ports.</p>
          <label>
            Effect family
            <select
              value={palette}
              onChange={(e) => setPalette(e.target.value)}
            >
              {context.catalog.types.map((t: any) => (
                <option key={t.alias} value={t.alias}>
                  {t.name}
                </option>
              ))}
            </select>
          </label>
          <button className="wide" onClick={addBlock}>
            + Add block
          </button>
          <p className="caption">
            Design palette only. Availability and settings must be resolved for
            the target device.
          </p>
          <hr />
          <h2>Tone brief</h2>
          <textarea
            aria-label="Tone brief"
            value={design.intent ?? ""}
            onChange={(e) => change({ ...design, intent: e.target.value })}
            placeholder="Describe the response, space and character you want…"
          />
          <h2>Scenes</h2>
          <div className="scenes">
            {Array.from({ length: 8 }, (_, i) => (
              <button
                key={i}
                className={scene === i + 1 ? "active" : ""}
                onClick={() => setScene(i + 1)}
              >
                {i + 1}
              </button>
            ))}
          </div>
          <small>{selectedScene?.name || "No explicit scene settings"}</small>
          <button
            className="wide"
            onClick={() => {
              setAdvanced(
                JSON.stringify(
                  {
                    scenes: design.scenes ?? [],
                    active_scene: design.active_scene ?? 1,
                    modifiers: design.modifiers ?? [],
                    controllers: design.controllers ?? {},
                  },
                  null,
                  2,
                ),
              );
              setModal("performance");
            }}
          >
            Edit scene & controller data
          </button>
          <hr />
          <h2>Project</h2>
          <button
            className="wide"
            onClick={() =>
              download(
                new Blob([JSON.stringify(design, null, 2)], {
                  type: "application/json",
                }),
                "preset-design.json",
              )
            }
          >
            Download design JSON
          </button>
          <label className="file-label">
            Import design JSON
            <input
              type="file"
              accept=".json"
              onChange={(e) => {
                const f = e.target.files?.[0];
                if (f)
                  run(async () => {
                    const next = JSON.parse(await f.text());
                    const valid = await api("validate", { design: next });
                    change(valid.design);
                  });
              }}
            />
          </label>
        </aside>
        <main className="canvas-panel">
          <div className="canvas-toolbar">
            <nav aria-label="Graph view">
              {[
                ["logical", "Logical chain"],
                ["physical", "Compiled Axe grid"],
                ["captured", "Captured grid"],
              ]
                .filter(([key]) => key !== "captured" || design.captured)
                .map(([key, label]) => (
                  <button
                    key={key}
                    className={tab === key ? "active" : ""}
                    onClick={() => setTab(key)}
                  >
                    {label}
                  </button>
                ))}
            </nav>
            <div>
              <button onClick={() => run(() => exportImage("svg"))}>SVG</button>
              <button onClick={() => run(() => exportImage("png"))}>PNG</button>
            </div>
          </div>
          <div className="canvas">
            {tab !== "logical" && !physical ? (
              <div className="empty">
                <h2>No physical preview yet</h2>
                <p>
                  Compile the draft to resolve physical placement. Unsupported
                  device operations remain blockers.
                </p>
              </div>
            ) : (
              <ReactFlow
                key={tab}
                nodes={nodes}
                edges={edges}
                nodeTypes={nodeTypes}
                fitView
                fitViewOptions={{ padding: 0.2 }}
                minZoom={0.15}
                maxZoom={2}
                nodesConnectable={tab === "logical"}
                nodesDraggable={tab === "logical"}
                deleteKeyCode={null}
                onNodeClick={(_, n) => {
                  setSelected(
                    compiled?.mapping && tab === "physical"
                      ? (Object.entries(compiled.mapping).find(
                          ([, ref]) => ref === n.id,
                        )?.[0] ?? n.id)
                      : n.id,
                  );
                }}
                onPaneClick={() => setSelected("")}
                onConnect={connect}
                onNodeDragStart={() => {
                  dragStart.current = structuredClone(design);
                }}
                onNodesChange={(changes) => {
                  if (tab !== "logical") return;
                  const moves = changes.filter((c) => c.type === "position");
                  if (!moves.length) return;
                  setDesign(
                    (d) =>
                      d && {
                        ...d,
                        nodes: d.nodes.map((n) => {
                          const m = moves.find((m) => m.id === n.id);
                          return m?.position
                            ? { ...n, ui_position: m.position }
                            : n;
                        }),
                      },
                  );
                  setDirty(true);
                  setCompiled(undefined);
                }}
                onNodeDragStop={() => {
                  if (dragStart.current) {
                    const before = dragStart.current;
                    setHistory((h) => [...h.slice(-49), before]);
                    setFuture([]);
                    dragStart.current = null;
                  }
                }}
                onEdgeDoubleClick={(_, e) => {
                  if (tab === "logical")
                    change({
                      ...design,
                      edges: design.edges.filter(
                        (item) => item.from + "->" + item.to !== e.id,
                      ),
                    });
                }}
              >
                <Background gap={24} size={1} color="#c5d2dd" />
                <Controls showInteractive={false} />
                {nodes.length > 8 && (
                  <MiniMap nodeColor="#8b9faf" pannable zoomable />
                )}
              </ReactFlow>
            )}
          </div>
          <div className="canvas-footer">
            <span>
              {tab === "logical"
                ? "Proposed signal flow · double-click a cable to remove it"
                : tab === "captured"
                  ? "Captured physical cables · settings may be incomplete"
                  : "Simulated physical preview · explicit shunts included"}
            </span>
            <span>
              {design.nodes.length} blocks · {design.edges.length} cables
            </span>
          </div>
          <section className="compile-results">
            <h2>Build review</h2>
            {compiled ? (
              <>
                <p>
                  {compiled.buildable
                    ? "Simulation preview ready"
                    : "Build blocked"}{" "}
                  · DSP load {compiled.dsp_cost ?? "not measured"}
                </p>
                {compiled.blockers.map((b: any, i: number) => (
                  <div className="blocker" key={i}>
                    <strong>{b.code}</strong>
                    <p>{b.message}</p>
                    {b.details?.missing && (
                      <ul>
                        {b.details.missing.map((m: string) => (
                          <li key={m}>{m}</li>
                        ))}
                      </ul>
                    )}
                  </div>
                ))}
                {compiled.warnings.map((w: string) => (
                  <p className="caption" key={w}>
                    {w}
                  </p>
                ))}
                {compiled.buildable && (
                  <p className="caption">
                    To request the exact approval-bound preview in chat, use{" "}
                    <code>design preview {context.project}</code>. This canvas
                    cannot apply or save to the device.
                  </p>
                )}
              </>
            ) : (
              <p className="caption">
                Compile to check routing and device capability. Draft changes
                never send MIDI.
              </p>
            )}
          </section>
        </main>
        <aside className="inspector">
          <h2>{node ? "Block settings" : "Select a block"}</h2>
          {node ? (
            <>
              <label>
                Name
                <input
                  value={node.label ?? node.id}
                  onChange={(e) => editNode({ label: e.target.value })}
                />
              </label>
              <div className="block-id">
                {node.id} · {node.type}
              </div>
              <label className="check">
                <input
                  type="checkbox"
                  checked={node.bypass ?? false}
                  onChange={(e) => editNode({ bypass: e.target.checked })}
                />{" "}
                Bypass in default state
              </label>
              <div className="channels">
                {"ABCD".split("").map((ch) => (
                  <button
                    className={channel === ch ? "active" : ""}
                    key={ch}
                    onClick={() => setChannel(ch)}
                  >
                    {ch}
                  </button>
                ))}
              </div>
              <small>
                Editing channel {channel}. Scene {scene}:{" "}
                {selectedScene?.channels?.[node.id] ??
                  node.active_channel ??
                  "A"}{" "}
                active.
              </small>
              <label>
                Model preference
                <input
                  value={node.model ?? ""}
                  onChange={(e) =>
                    editNode({ model: e.target.value || undefined })
                  }
                  placeholder="Not selected"
                />
              </label>
              <p className="caption">
                A preference is not a verified device model. Mock preview
                requires an authored mock option.
              </p>
              <label>
                Why this block
                <textarea
                  value={node.rationale ?? ""}
                  onChange={(e) => editNode({ rationale: e.target.value })}
                  placeholder="Explain its musical role…"
                />
              </label>
              <div className="parameter-list">
                {Object.entries(values).map(([name, value]) => (
                  <label key={name}>
                    {name}
                    <input
                      value={String(value)}
                      onChange={(e) => {
                        const raw = e.target.value;
                        const parsed =
                          raw !== "" && Number.isFinite(Number(raw))
                            ? Number(raw)
                            : raw;
                        const channels = structuredClone(node.channels ?? {});
                        channels[channel] = {
                          ...(channels[channel] ?? {}),
                          [name]: parsed,
                        };
                        const parameters = { ...node.parameters };
                        delete parameters[name];
                        editNode({ channels, parameters });
                      }}
                    />
                  </label>
                ))}
              </div>
              <label>
                Parameter
                <select
                  value={paramName}
                  onChange={(e) => setParamName(e.target.value)}
                >
                  <option value="">Choose or type below</option>
                  {/* Detailed definitions are discovered through the CLI; custom names stay explicit. */}
                  {(typeDef?.parameters ?? []).map((p: any) => (
                    <option key={p.id} value={p.name}>
                      {p.label}
                    </option>
                  ))}
                </select>
                <input
                  aria-label="Parameter name"
                  value={paramName}
                  onChange={(e) => setParamName(e.target.value)}
                  placeholder="gain, time, mix…"
                />
              </label>
              <label>
                Value
                <input
                  value={paramValue}
                  onChange={(e) => setParamValue(e.target.value)}
                />
              </label>
              <button
                className="wide"
                disabled={!paramName || !paramValue}
                onClick={() => {
                  const v: Setting = Number.isFinite(Number(paramValue))
                    ? Number(paramValue)
                    : paramValue;
                  editNode({
                    channels: {
                      ...node.channels,
                      [channel]: {
                        ...node.channels?.[channel],
                        [paramName]: v,
                      },
                    },
                  });
                  setParamName("");
                  setParamValue("");
                }}
              >
                Set draft value
              </button>
              <label>
                Channel for this scene
                <select
                  value={
                    selectedScene?.channels?.[node.id] ??
                    node.active_channel ??
                    "A"
                  }
                  onChange={(e) => {
                    const scenes = [...(design.scenes ?? [])];
                    let s = scenes.find((s) => s.number === scene);
                    if (!s) {
                      s = { number: scene, channels: {}, bypass: {} };
                      scenes.push(s);
                    }
                    s = {
                      ...s,
                      channels: { ...s.channels, [node.id]: e.target.value },
                    };
                    change({
                      ...design,
                      scenes: scenes.map((x) => (x.number === scene ? s! : x)),
                    });
                  }}
                >
                  {"ABCD".split("").map((c) => (
                    <option key={c}>{c}</option>
                  ))}
                </select>
              </label>
              <button
                className="wide"
                onClick={() => {
                  setAdvanced(JSON.stringify(node, null, 2));
                  setModal("node");
                }}
              >
                Edit complete block JSON
              </button>
              <button
                className="wide"
                onClick={() => {
                  run(async () => {
                    if (dirty) await save();
                    download(
                      await blob(
                        "render?block=" +
                          encodeURIComponent(node.id) +
                          "&scene=" +
                          scene,
                      ),
                      node.id + ".svg",
                    );
                  });
                }}
              >
                Export block setting sheet
              </button>
              <button
                className="danger wide"
                onClick={() => {
                  change(removeNode(design, node.id));
                  setSelected("");
                }}
              >
                Remove block from draft
              </button>
            </>
          ) : (
            <>
              <p>Inspect a block to review its role, channel and settings.</p>
              <p className="caption">
                Unknown captured values remain unknown. No device settings are
                inferred from a diagram.
              </p>
            </>
          )}
        </aside>
      </div>
      <section className="audio-panel">
        <div>
          <h2>Reference & audition</h2>
          <p>Use a guitar excerpt or a full mix. Processing stays local.</p>
          <input
            aria-label="Import audio"
            type="file"
            accept="audio/*"
            onChange={(e) => {
              const file = e.target.files?.[0];
              if (file) run(() => importFile(file));
            }}
          />
          {audioUrl && (
            <audio aria-label="Audio playback" controls src={audioUrl} />
          )}
        </div>
        <div className="excerpt">
          <label>
            Start (s)
            <input
              type="number"
              min="0"
              value={excerpt.start}
              onChange={(e) =>
                setExcerpt({ ...excerpt, start: Number(e.target.value) })
              }
            />
          </label>
          <label>
            Duration (s)
            <input
              type="number"
              min=".25"
              max="120"
              value={excerpt.duration}
              onChange={(e) =>
                setExcerpt({ ...excerpt, duration: Number(e.target.value) })
              }
            />
          </label>
          <label className="check">
            <input
              type="checkbox"
              checked={excerpt.separate}
              onChange={(e) =>
                setExcerpt({ ...excerpt, separate: e.target.checked })
              }
            />{" "}
            Extract guitar from mix
          </label>
          <button
            disabled={!audioId || busy || job?.status === "running"}
            onClick={() =>
              run(async () =>
                setJob(await api("jobs", { audio_id: audioId, ...excerpt })),
              )
            }
          >
            Analyze excerpt
          </button>
          {job?.status === "running" && (
            <button
              onClick={() =>
                run(async () =>
                  setJob(await api("jobs/" + job.id + "/cancel", {})),
                )
              }
            >
              Cancel analysis
            </button>
          )}
        </div>
        <div className="analysis">
          {analysis ? (
            <>
              <strong>
                {analysis.measurements.spectral_centroid_hz.toFixed(0)} Hz
                spectral centroid
              </strong>
              <p>
                Crest {analysis.measurements.crest_db.toFixed(1)} dB · RMS{" "}
                {analysis.measurements.rms_dbfs.toFixed(1)} dBFS
              </p>
              <div className="bands">
                {Object.entries(analysis.measurements.band_energy).map(
                  ([name, value]) => (
                    <label key={name}>
                      {name.replace("_", " ")}
                      <meter min="0" max="1" value={Number(value)} />
                    </label>
                  ),
                )}
              </div>
              {analysis.processing.separation && (
                <button onClick={() => run(showStem)}>
                  Audition extracted guitar
                </button>
              )}
              <button
                onClick={() => {
                  setReference(analysis);
                  setMessage(
                    "Reference selected. Import and analyze your recorded result next.",
                  );
                }}
              >
                Use as reference
              </button>
              {reference && reference.id !== analysis.id && (
                <button
                  onClick={() =>
                    run(async () =>
                      setComparison(
                        await api("audio/compare", {
                          reference: reference.id,
                          recording: analysis.id,
                        }),
                      ),
                    )
                  }
                >
                  Compare with reference
                </button>
              )}
              <button
                onClick={() => {
                  change({
                    ...design,
                    references: [
                      ...(design.references ?? []),
                      {
                        analysis_id: analysis.id,
                        audio_id: audioId,
                        excerpt: analysis.excerpt,
                      },
                    ],
                  });
                  setMessage(
                    "Analysis attached to design. Ask the AI in chat to inspect this revision and propose a chain.",
                  );
                }}
              >
                Attach to design for AI
              </button>
              {comparison && (
                <p>
                  Recorded centroid difference:{" "}
                  {comparison.centroid_difference_hz.toFixed(0)} Hz. RMS
                  playback gain suggestion:{" "}
                  {comparison.recording_gain_db_for_rms_match.toFixed(1)} dB.
                  Compare by ear; this is not an accuracy score.
                </p>
              )}
            </>
          ) : (
            <p className="caption">
              Analysis does not identify the original rig. Mixed tracks and
              separated stems may contain multiple guitars or artifacts. Let the
              AI explain a proposed chain before you approve it.
            </p>
          )}
        </div>
      </section>
      {reference && analysis && reference.id !== analysis.id && (
        <Audition
          reference={reference}
          recording={analysis}
          fetchAudio={(id) => blob("audio/" + id)}
        />
      )}
      {modal && modal !== "block" && (
        <div className="modal-backdrop">
          <section className="modal">
            <button
              className="close"
              aria-label="Close dialog"
              onClick={() => setModal("")}
            >
              ×
            </button>
            <h2>
              {modal === "protocol"
                ? "Protocol coverage"
                : modal === "node"
                  ? "Complete block settings"
                  : "Scenes, modifiers & controllers"}
            </h2>
            {modal === "protocol" ? (
              <>
                <p>
                  Full automatic USB preset construction is not yet validated.
                </p>
                <table>
                  <thead>
                    <tr>
                      <th>Function</th>
                      <th>Read</th>
                      <th>Write</th>
                      <th>Evidence</th>
                    </tr>
                  </thead>
                  <tbody>
                    {context.protocol.entries.map((r: any) => (
                      <tr key={r.id}>
                        <td>{r.name}</td>
                        <td>
                          {r.read_implemented ? "Implemented" : "Missing"}
                        </td>
                        <td>
                          {r.write_implemented ? "Implemented" : "Missing"}
                        </td>
                        <td>{r.hardware_validation ?? r.scope ?? r.frame}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </>
            ) : modal === "performance" ? (
              <Performance
                design={design}
                catalog={context.catalog}
                onChange={change}
                onAdvanced={() => {
                  setAdvanced(
                    JSON.stringify(
                      {
                        scenes: design.scenes ?? [],
                        active_scene: design.active_scene ?? 1,
                        modifiers: design.modifiers ?? [],
                        controllers: design.controllers ?? {},
                      },
                      null,
                      2,
                    ),
                  );
                  setModal("performance-json");
                }}
              />
            ) : (
              <>
                <p>
                  JSON uses stable design block IDs. Changes remain a draft
                  until validated and compiled.
                </p>
                <textarea
                  className="json-editor"
                  aria-label="Advanced design JSON"
                  value={advanced}
                  onChange={(e) => setAdvanced(e.target.value)}
                />
                <button
                  className="primary"
                  onClick={() =>
                    run(async () => {
                      const value = JSON.parse(advanced);
                      if (modal === "node") {
                        if (value.id !== selected)
                          throw new Error("Keep stable block ID unchanged");
                        const candidate = {
                          ...design,
                          nodes: design.nodes.map((n) =>
                            n.id === selected ? value : n,
                          ),
                        };
                        change(
                          (await api("validate", { design: candidate })).design,
                        );
                      } else
                        change(
                          (
                            await api("validate", {
                              design: { ...design, ...value },
                            })
                          ).design,
                        );
                      setModal("");
                    })
                  }
                >
                  Update draft
                </button>
              </>
            )}
          </section>
        </div>
      )}
    </div>
  );
}
createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
