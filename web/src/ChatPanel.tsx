import { useEffect, useState } from "react";

type Props = {
  project: string;
  revision: string;
  dirty: boolean;
  selected?: string;
  analysisId?: string;
  request: (path: string, body?: unknown) => Promise<any>;
  onAccepted: () => Promise<unknown>;
};
export function ChatPanel({
  project,
  revision,
  dirty,
  selected,
  analysisId,
  request,
  onAccepted,
}: Props) {
  const [state, setState] = useState<any>();
  const [text, setText] = useState("");
  const [error, setError] = useState("");
  const [pending, setPending] = useState(false);
  const [attach, setAttach] = useState(false);
  useEffect(() => {
    let active = true;
    setState(undefined);
    setError("");
    setText("");
    const refresh = () =>
      request("chat")
        .then((s) => {
          if (active) setState(s);
        })
        .catch((e) => {
          if (active) setError(e.message);
        });
    void refresh();
    const timer = setInterval(() => void refresh(), 650);
    return () => {
      active = false;
      clearInterval(timer);
    };
  }, [project]);
  async function action(path: string, body: any = {}) {
    setPending(true);
    setError("");
    try {
      const s = await request("chat/" + path, { project, ...body });
      setState(s);
      if (path === "message") setText("");
      if (path === "decision" && body.accept) await onAccepted();
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setPending(false);
    }
  }
  const running = state?.status === "running" || state?.status === "connecting";
  return (
    <section className="chat-panel" aria-label="AI preset assistant">
      <div className="chat-heading">
        <div>
          <h2>Talk through your tone</h2>
          <p>Codex helps refine this draft. Your Axe-Fx stays unchanged.</p>
        </div>
        <span className="chat-state">
          {state?.status ?? "Loading"}
          {state?.model ? " · " + state.model : ""}
        </span>
      </div>
      <p className="caption">
        Uses your local Codex sign-in and configured AI service. Messages,
        preset context, and attached analysis summaries are sent to that
        service. Raw recordings stay local.
      </p>
      {state?.status !== "ready" && state?.status !== "running" && (
        <button
          disabled={pending || running}
          onClick={() => void action("connect")}
        >
          {running ? "Connecting…" : "Connect Codex"}
        </button>
      )}
      {(error || state?.error) && (
        <p role="alert" className="chat-error">
          {error || state.error}
        </p>
      )}
      <div className="chat-messages" role="log" aria-live="polite">
        {!state?.messages?.length && (
          <p>
            Ask about the current routing, select a block to discuss it, or
            describe the sound you want.
          </p>
        )}
        {state?.messages?.map((m: any, i: number) => (
          <article className={"chat-message " + m.role} key={m.id ?? i}>
            <strong>{m.role === "user" ? "You" : "Preset assistant"}</strong>
            <pre>{m.text}</pre>
          </article>
        ))}
      </div>
      {state?.proposal && (
        <div className="proposal-review">
          <h3>Proposed draft changes</h3>
          <p>{state.proposal.summary}</p>
          <small>
            Based on revision {state.proposal.base.slice(0, 10)} · does not
            apply to hardware
          </small>
          {state.proposal.changes.map((change: any, i: number) => (
            <details key={i}>
              <summary>{change.path}</summary>
              <div className="proposal-diff">
                <pre>{JSON.stringify(change.before, null, 2)}</pre>
                <pre>{JSON.stringify(change.after, null, 2)}</pre>
              </div>
            </details>
          ))}
          <button
            className="primary"
            disabled={
              pending || running || dirty || state.proposal.base !== revision
            }
            onClick={() =>
              void action("decision", {
                id: state.proposal.id,
                base: revision,
                accept: true,
              })
            }
          >
            Accept draft changes
          </button>
          <button
            disabled={pending || running}
            onClick={() =>
              void action("decision", {
                id: state.proposal.id,
                base: revision,
                accept: false,
              })
            }
          >
            Reject changes
          </button>
          {(dirty || state.proposal.base !== revision) && (
            <p>The draft changed. Save it and ask for a fresh proposal.</p>
          )}
        </div>
      )}
      <form
        onSubmit={(e) => {
          e.preventDefault();
          void action("message", {
            text,
            base: revision,
            selected_block: selected || null,
            analysis_ids: attach && analysisId ? [analysisId] : [],
          });
        }}
      >
        {selected && <small>Selected block: {selected}</small>}
        {analysisId && (
          <label className="check">
            <input
              type="checkbox"
              checked={attach}
              onChange={(e) => setAttach(e.target.checked)}
            />{" "}
            Attach current analysis summary
          </label>
        )}
        <textarea
          aria-label="Message Codex"
          value={text}
          onChange={(e) => setText(e.target.value)}
          maxLength={12000}
          placeholder="Make the clean sound warmer while keeping the parallel delay…"
        />
        {dirty && (
          <p>
            Save your draft before sending so Codex sees your latest changes.
          </p>
        )}
        <div className="chat-controls">
          <button
            className="primary"
            type="submit"
            disabled={
              pending || dirty || state?.status !== "ready" || !text.trim()
            }
          >
            Send to Codex
          </button>
          {state?.status === "running" && (
            <button
              type="button"
              disabled={pending}
              onClick={() => void action("interrupt")}
            >
              Stop response
            </button>
          )}
        </div>
      </form>
    </section>
  );
}
