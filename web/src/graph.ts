export type Setting = string | number | boolean;
export type DesignNode = {
  id: string;
  type: string;
  label?: string;
  parameters?: Record<string, Setting>;
  channels?: Record<string, Record<string, Setting>>;
  bypass?: boolean | null;
  active_channel?: string | null;
  model?: string;
  model_parameter?: string;
  rationale?: string;
  position?: { row: number; column: number };
  ui_position?: { x: number; y: number };
  [key: string]: unknown;
};
export type Design = {
  design_version: string;
  name: string;
  intent?: string;
  mode?: string;
  nodes: DesignNode[];
  edges: { from: string; to: string }[];
  scenes?: {
    number: number;
    name?: string;
    channels?: Record<string, string>;
    bypass?: Record<string, boolean>;
    controllers?: Record<string, number>;
  }[];
  active_scene?: number;
  modifiers?: unknown[];
  controllers?: Record<string, unknown>;
  references?: unknown[];
  captured?: any;
  unknowns?: string[];
  [key: string]: unknown;
};
export function logicalPositions(
  d: Design,
): Record<string, { x: number; y: number }> {
  const levels: Record<string, number> = {};
  let pending = d.nodes.map((n) => n.id);
  for (let pass = 0; pending.length && pass < 85; pass++) {
    const ready = pending.filter((id) =>
      d.edges
        .filter((e) => e.to === id)
        .every((e) => levels[e.from] !== undefined),
    );
    if (!ready.length) break;
    for (const id of ready) {
      levels[id] = Math.max(
        0,
        ...d.edges.filter((e) => e.to === id).map((e) => levels[e.from] + 1),
      );
      pending = pending.filter((i) => i !== id);
    }
  }
  const rows: Record<number, number> = {};
  const dryLevels = new Set<number>();
  for (const edge of d.edges) {
    for (let level = levels[edge.from] + 1; level < levels[edge.to]; level++)
      dryLevels.add(level);
  }
  return Object.fromEntries(
    d.nodes.map((n, i) => {
      const level = levels[n.id] ?? i;
      const row = rows[level] ?? 0;
      rows[level] = row + 1;
      const lanes = dryLevels.has(level)
        ? [1, -1, 2, -2, 3, 0]
        : [0, 1, -1, 2, -2, 3];
      return [
        n.id,
        n.ui_position ?? { x: level * 238, y: (lanes[row] ?? row) * 160 },
      ];
    }),
  );
}
export function canConnect(d: Design, from: string, to: string): boolean {
  if (from === to || d.edges.some((e) => e.from === from && e.to === to))
    return false;
  const seen = new Set<string>();
  const stack = [to];
  while (stack.length) {
    const id = stack.pop()!;
    if (id === from) return false;
    if (seen.has(id)) continue;
    seen.add(id);
    stack.push(...d.edges.filter((e) => e.from === id).map((e) => e.to));
  }
  return true;
}
export function removeNode(d: Design, id: string): Design {
  return {
    ...d,
    nodes: d.nodes.filter((n) => n.id !== id),
    edges: d.edges.filter((e) => e.from !== id && e.to !== id),
    scenes: d.scenes?.map((s) => ({
      ...s,
      channels: Object.fromEntries(
        Object.entries(s.channels ?? {}).filter(([k]) => k !== id),
      ),
      bypass: Object.fromEntries(
        Object.entries(s.bypass ?? {}).filter(([k]) => k !== id),
      ),
    })),
    modifiers: d.modifiers?.filter((m: any) => m.block !== id),
  };
}

// A partial USB capture only establishes values for its observed scene.
export function sceneSetting(
  d: Design,
  n: DesignNode,
  number: number,
  field: "bypass" | "active_channel",
): boolean | string | null {
  const scene = d.scenes?.find((s) => s.number === number);
  const values = field === "bypass" ? scene?.bypass : scene?.channels;
  if (values && Object.hasOwn(values, n.id)) return values[n.id];
  if (
    d.captured &&
    !d.captured.current.complete &&
    number !== d.captured.current.active_scene
  )
    return null;
  if (Object.hasOwn(n, field)) return n[field] ?? null;
  return field === "bypass" ? false : "A";
}
