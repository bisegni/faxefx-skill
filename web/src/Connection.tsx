import { useState } from "react";

type Props = {
  device: any;
  dirty: boolean;
  onSave: () => Promise<unknown>;
  onDiscard: () => Promise<unknown>;
  request: (path: string, body?: unknown) => Promise<any>;
};
export function Connection({
  device,
  dirty,
  onSave,
  onDiscard,
  request,
}: Props) {
  const [input, setInput] = useState("");
  const [output, setOutput] = useState("");
  const [review, setReview] = useState(false);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const status = device?.status ?? "connecting";
  const reading = busy || status === "connecting" || status === "reading";
  async function refresh(choice?: "save" | "discard") {
    if (dirty && !choice) {
      setReview(true);
      return;
    }
    setBusy(true);
    setError("");
    try {
      if (choice === "save") await onSave();
      if (choice === "discard") await onDiscard();
      await request("device/capture", {
        ...(input ? { input } : {}),
        ...(output ? { output } : {}),
      });
      setReview(false);
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setBusy(false);
    }
  }
  return (
    <section className="device-strip" aria-label="Axe-Fx connection">
      <div>
        <strong>
          {status === "ready"
            ? "Axe-Fx capture ready"
            : status === "reading"
              ? "Reading current preset…"
              : status === "connecting"
                ? "Connecting to Axe-Fx…"
                : status === "idle"
                  ? "Stored design"
                  : "Axe-Fx connection needs attention"}
        </strong>
        {device?.preset && (
          <span>
            {" "}
            {device.preset.slot} · {device.preset.name} · Scene{" "}
            {device.preset.active_scene}
          </span>
        )}
        {device?.device && (
          <small>
            {device.device.model ??
              device.device.name ??
              device.device.backend ??
              "Axe-Fx III"}
          </small>
        )}
        {device?.captured_at && (
          <small>
            Captured {new Date(device.captured_at).toLocaleString()} · snapshot,
            refresh after device changes · partial settings
          </small>
        )}
        {device?.error && (
          <p role="alert">
            {device.error.message}
            {device.project ? " Last capture retained; it may be stale." : ""}
          </p>
        )}
        {error && <p role="alert">{error}</p>}
      </div>
      <div className="connection-actions">
        {(["inputs", "outputs"] as const).map(
          (kind) =>
            device?.ports?.[kind]?.length > 0 && (
              <label key={kind}>
                MIDI {kind === "inputs" ? "input" : "output"}
                <select
                  value={kind === "inputs" ? input : output}
                  disabled={reading}
                  onChange={(e) =>
                    (kind === "inputs" ? setInput : setOutput)(e.target.value)
                  }
                >
                  <option value="">
                    {device?.[kind === "inputs" ? "input" : "output"] ??
                      "Select Axe-Fx endpoint"}
                  </option>
                  {device.ports[kind].map((name: string) => (
                    <option key={name}>{name}</option>
                  ))}
                </select>
              </label>
            ),
        )}
        <button disabled={reading} onClick={() => void refresh()}>
          {device?.project ? "Refresh from Axe-Fx" : "Retry connection"}
        </button>
      </div>
      {review && (
        <div
          className="refresh-choice"
          role="dialog"
          aria-label="Unsaved draft"
        >
          <p>
            Keep your changes before reading a new preset? Previous projects
            remain stored locally.
          </p>
          <button onClick={() => void refresh("save")}>
            Save draft & refresh
          </button>
          <button onClick={() => void refresh("discard")}>
            Discard unsaved changes & refresh
          </button>
          <button onClick={() => setReview(false)}>Cancel</button>
        </div>
      )}
    </section>
  );
}
