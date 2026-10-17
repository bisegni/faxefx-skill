import { describe, it, expect } from "vitest";
import { canConnect, logicalPositions, removeNode, type Design } from "./graph";
const d: Design = {
  design_version: "1.0",
  name: "Example",
  nodes: [
    { id: "input", type: "input-1" },
    { id: "amp", type: "amp" },
    { id: "out", type: "output-1" },
  ],
  edges: [
    { from: "input", to: "amp" },
    { from: "amp", to: "out" },
  ],
};
describe("logical graph editing", () => {
  it("keeps a direct dry cable clear of the parallel effect", () => {
    const parallel = { ...d, edges: [...d.edges, { from: "input", to: "out" }] };
    const positions = logicalPositions(parallel);
    expect(positions.amp.y).not.toBe(positions.input.y);
    expect(positions.out.y).toBe(positions.input.y);
  });
  it("rejects backwards cycles and duplicate cables", () => {
    expect(canConnect(d, "out", "input")).toBe(false);
    expect(canConnect(d, "input", "amp")).toBe(false);
    expect(canConnect(d, "input", "out")).toBe(true);
  });
  it("lays out dependencies and preserves canvas-only positions", () => {
    expect(logicalPositions(d).out.x).toBeGreaterThan(
      logicalPositions(d).amp.x,
    );
    expect(
      logicalPositions({
        ...d,
        nodes: d.nodes.map((n) => ({ ...n, ui_position: { x: 47, y: 92 } })),
      }).amp,
    ).toEqual({ x: 47, y: 92 });
  });
  it("removes cables and performance references with a node", () => {
    const result = removeNode(
      {
        ...d,
        scenes: [{ number: 1, channels: { amp: "B" }, bypass: { amp: true } }],
        modifiers: [{ block: "amp", ref: "modifier1" }],
      },
      "amp",
    );
    expect(result.edges).toEqual([]);
    expect(result.scenes?.[0].channels).toEqual({});
    expect(result.modifiers).toEqual([]);
  });
});
