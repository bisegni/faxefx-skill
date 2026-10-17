import { useState } from "react";
import type { Design } from "./graph";

export function Performance({
  design,
  catalog,
  onChange,
  onAdvanced,
}: {
  design: Design;
  catalog: any;
  onChange: (d: Design) => void;
  onAdvanced: () => void;
}) {
  const [page, setPage] = useState("scenes");
  const [number, setNumber] = useState(design.active_scene ?? 1);
  const [controller, setController] = useState("manual1");
  const scene = design.scenes?.find((s) => s.number === number) ?? {
    number,
    channels: {},
    bypass: {},
    controllers: {},
  };
  function scenePatch(patch: Partial<typeof scene>) {
    onChange({
      ...design,
      scenes: [
        ...(design.scenes ?? []).filter((s) => s.number !== number),
        { ...scene, ...patch },
      ].sort((a, b) => a.number - b.number),
    });
  }
  const controllerFields: Record<string, string[]> = {
    manual: ["value"],
    lfo: ["rate", "depth", "shape"],
    envelope: ["attack", "release", "gain"],
    sequencer: ["steps", "rate"],
    pitch: ["minimum", "maximum"],
    "scene-controller": ["values"],
  };
  const prefix = controller.startsWith("scene-controller")
    ? "scene-controller"
    : controller.replace(/\d+$/, "");
  const settings = (design.controllers?.[controller] ?? {}) as Record<
    string,
    any
  >;
  const modifiers = (design.modifiers ?? []) as any[];
  const sourceNames = [
    ...Array.from({ length: 16 }, (_, i) => "external" + (i + 1)),
    ...Array.from({ length: 5 }, (_, i) => "manual" + (i + 1)),
    "lfo1",
    "lfo2",
    "envelope",
    "sequencer",
    "pitch",
    "scene-controller1",
    "scene-controller2",
  ];
  function updateModifier(index: number, patch: any) {
    onChange({
      ...design,
      modifiers: modifiers.map((m, i) =>
        i === index ? { ...m, ...patch } : m,
      ),
    });
  }
  return (
    <div className="performance">
      <nav>
        {["scenes", "controllers", "modifiers"].map((p) => (
          <button
            key={p}
            className={page === p ? "active" : ""}
            onClick={() => setPage(p)}
          >
            {p[0].toUpperCase() + p.slice(1)}
          </button>
        ))}
        <button onClick={onAdvanced}>Advanced JSON</button>
      </nav>
      {page === "scenes" && (
        <>
          <div className="performance-row">
            <label>
              Scene
              <select
                value={number}
                onChange={(e) => setNumber(Number(e.target.value))}
              >
                {Array.from({ length: 8 }, (_, i) => (
                  <option key={i} value={i + 1}>
                    {i + 1}
                  </option>
                ))}
              </select>
            </label>
            <label>
              Scene name
              <input
                maxLength={31}
                value={scene.name ?? ""}
                onChange={(e) => scenePatch({ name: e.target.value })}
              />
            </label>
            <label className="check">
              <input
                type="checkbox"
                checked={(design.active_scene ?? 1) === number}
                onChange={() => onChange({ ...design, active_scene: number })}
              />{" "}
              Initial scene
            </label>
          </div>
          <table>
            <thead>
              <tr>
                <th>Block</th>
                <th>Channel</th>
                <th>Bypass in scene</th>
              </tr>
            </thead>
            <tbody>
              {design.nodes
                .filter((n) => !["shunt", "wire"].includes(n.type))
                .map((n) => (
                  <tr key={n.id}>
                    <td>
                      {n.label ?? n.id}
                      <small>{n.type}</small>
                    </td>
                    <td>
                      <select
                        aria-label={"Scene channel " + n.id}
                        value={
                          scene.channels?.[n.id] ?? n.active_channel ?? "A"
                        }
                        onChange={(e) =>
                          scenePatch({
                            channels: {
                              ...scene.channels,
                              [n.id]: e.target.value,
                            },
                          })
                        }
                      >
                        {"ABCD".split("").map((c) => (
                          <option key={c}>{c}</option>
                        ))}
                      </select>
                    </td>
                    <td>
                      <input
                        aria-label={"Scene bypass " + n.id}
                        type="checkbox"
                        checked={scene.bypass?.[n.id] ?? n.bypass ?? false}
                        onChange={(e) =>
                          scenePatch({
                            bypass: {
                              ...scene.bypass,
                              [n.id]: e.target.checked,
                            },
                          })
                        }
                      />
                    </td>
                  </tr>
                ))}
            </tbody>
          </table>
          <div className="performance-row">
            {["scene-controller1", "scene-controller2"].map((c) => (
              <label key={c}>
                {c}
                <input
                  type="number"
                  min="0"
                  max="1"
                  step=".01"
                  value={scene.controllers?.[c] ?? ""}
                  placeholder="Unspecified"
                  onChange={(e) =>
                    scenePatch({
                      controllers: {
                        ...scene.controllers,
                        [c]: Number(e.target.value),
                      },
                    })
                  }
                />
              </label>
            ))}
          </div>
          <p className="caption">
            Scenes select shared block channels; they do not contain separate
            copies of every parameter.
          </p>
        </>
      )}
      {page === "controllers" && (
        <>
          <label>
            Preset controller
            <select
              value={controller}
              onChange={(e) => setController(e.target.value)}
            >
              {sourceNames
                .filter((s) => !s.startsWith("external"))
                .map((c) => (
                  <option key={c}>{c}</option>
                ))}
            </select>
          </label>
          <div className="performance-row">
            {(controllerFields[prefix] ?? []).map((field) => (
              <label key={controller + field}>
                {field}
                {field === "shape" ? (
                  <select
                    value={settings[field] ?? ""}
                    onChange={(e) =>
                      onChange({
                        ...design,
                        controllers: {
                          ...design.controllers,
                          [controller]: {
                            ...settings,
                            [field]: e.target.value,
                          },
                        },
                      })
                    }
                  >
                    <option value="">Unspecified</option>
                    {["sine", "triangle", "square", "random"].map((s) => (
                      <option key={s}>{s}</option>
                    ))}
                  </select>
                ) : (
                  <input
                    key={controller + field}
                    aria-label={"Controller " + field}
                    type={
                      field === "steps" || field === "values"
                        ? "text"
                        : "number"
                    }
                    step="any"
                    defaultValue={
                      Array.isArray(settings[field])
                        ? JSON.stringify(settings[field])
                        : (settings[field] ?? "")
                    }
                    placeholder={
                      field === "values"
                        ? "Eight values, e.g. [0,0,0,0,0,0,0,0]"
                        : field === "steps"
                          ? "Array of 1–32 values"
                          : ""
                    }
                    onBlur={(e) => {
                      if (e.target.value !== "") {
                        let value: any;
                        try {
                          value =
                            field === "steps" || field === "values"
                              ? JSON.parse(e.target.value)
                              : Number(e.target.value);
                        } catch {
                          e.target.setCustomValidity(
                            "Enter a JSON array of numbers",
                          );
                          e.target.reportValidity();
                          return;
                        }
                        e.target.setCustomValidity("");
                        onChange({
                          ...design,
                          controllers: {
                            ...design.controllers,
                            [controller]: { ...settings, [field]: value },
                          },
                        });
                      }
                    }}
                  />
                )}
              </label>
            ))}
          </div>
          <button
            onClick={() => {
              const controllers = { ...design.controllers };
              delete controllers[controller];
              onChange({ ...design, controllers });
            }}
          >
            Remove explicit controller settings
          </button>
          <p className="caption">
            Units and ranges are checked by compilation. Hardware behavior
            remains subject to the connected device profile.
          </p>
        </>
      )}
      {page === "modifiers" && (
        <>
          <button
            disabled={modifiers.length >= 32}
            onClick={() => {
              const ref = Array.from(
                { length: 32 },
                (_, i) => "modifier" + (i + 1),
              ).find((r) => !modifiers.some((m) => m.ref === r));
              onChange({
                ...design,
                modifiers: [
                  ...modifiers,
                  {
                    ref,
                    block:
                      design.nodes.find((n) => n.type === "amp")?.id ??
                      design.nodes[0]?.id ??
                      "",
                    parameter: "",
                    source: "external1",
                    settings: {},
                  },
                ],
              });
            }}
          >
            + Add modifier assignment
          </button>
          {modifiers.map((m, index) => {
            const node = design.nodes.find((n) => n.id === m.block);
            const type = catalog.types.find((t: any) => t.alias === node?.type);
            return (
              <section className="modifier-card" key={m.ref}>
                <strong>{m.ref}</strong>
                <div className="performance-row">
                  <label>
                    Block
                    <select
                      value={m.block}
                      onChange={(e) =>
                        updateModifier(index, {
                          block: e.target.value,
                          parameter: "",
                        })
                      }
                    >
                      {design.nodes.map((n) => (
                        <option key={n.id} value={n.id}>
                          {n.label ?? n.id}
                        </option>
                      ))}
                    </select>
                  </label>
                  <label>
                    Parameter
                    <select
                      value={m.parameter}
                      onChange={(e) =>
                        updateModifier(index, { parameter: e.target.value })
                      }
                    >
                      <option value="">Choose a parameter</option>
                      <option value="bypass">Bypass</option>
                      {(type?.parameters ?? []).map((p: any) => (
                        <option key={p.id} value={p.name}>
                          {p.label}
                        </option>
                      ))}
                    </select>
                  </label>
                  <label>
                    Source
                    <select
                      value={m.source}
                      onChange={(e) =>
                        updateModifier(index, { source: e.target.value })
                      }
                    >
                      {sourceNames.map((s) => (
                        <option key={s}>{s}</option>
                      ))}
                    </select>
                  </label>
                </div>
                <div className="performance-row">
                  {[
                    "minimum",
                    "maximum",
                    "start",
                    "end",
                    "mid",
                    "slope",
                    "scale",
                    "offset",
                    "damping",
                  ].map((field) => (
                    <label key={field}>
                      {field}
                      <input
                        type="number"
                        step="any"
                        value={m.settings?.[field] ?? ""}
                        placeholder="Unspecified"
                        onChange={(e) => {
                          const settings = { ...m.settings };
                          if (e.target.value === "") delete settings[field];
                          else settings[field] = Number(e.target.value);
                          updateModifier(index, { settings });
                        }}
                      />
                    </label>
                  ))}
                </div>
                <label className="check">
                  <input
                    type="checkbox"
                    checked={m.settings?.["auto-engage"] ?? false}
                    onChange={(e) =>
                      updateModifier(index, {
                        settings: {
                          ...m.settings,
                          "auto-engage": e.target.checked,
                        },
                      })
                    }
                  />{" "}
                  Auto-engage
                </label>
                <button
                  className="danger"
                  onClick={() =>
                    onChange({
                      ...design,
                      modifiers: modifiers.filter((_, i) => i !== index),
                    })
                  }
                >
                  Remove assignment
                </button>
              </section>
            );
          })}
          <p className="caption">
            The compiler validates target parameters and assignments. A manual
            parameter change may require explicitly removing an existing
            modifier first.
          </p>
        </>
      )}
    </div>
  );
}
