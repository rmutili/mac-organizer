import { useState } from "react";
import { api, errorMessage, type BackendStatus, type UndoAvailable } from "../../lib/api";
import type { Busy } from "../Toolbar";

interface StartScreenProps {
  backends: BackendStatus | null;
  busy: Busy;
  undoAvailable: UndoAvailable | null;
  onScan: () => void;
  onUndo: () => void;
  onKeySaved: () => void;
  onError: (message: string) => void;
}

function ApiKeyForm({ onSaved, onError }: { onSaved: () => void; onError: (m: string) => void }) {
  const [key, setKey] = useState("");
  const [isSaving, setIsSaving] = useState(false);
  return (
    <form
      style={{ display: "flex", gap: "var(--space-2)" }}
      onSubmit={async (e) => {
        e.preventDefault();
        setIsSaving(true);
        try {
          await api.setApiKey(key);
          setKey("");
          onSaved();
        } catch (err) {
          onError(errorMessage(err));
        } finally {
          setIsSaving(false);
        }
      }}
    >
      <input
        className="field"
        type="password"
        autoComplete="off"
        value={key}
        onChange={(e) => setKey(e.currentTarget.value)}
        placeholder="Anthropic API key (sk-ant-…)"
        aria-label="Anthropic API key"
        style={{ width: 280 }}
      />
      <button className="btn" type="submit" disabled={isSaving || !key}>
        Save to Keychain
      </button>
    </form>
  );
}

export function StartScreen({ backends, busy, undoAvailable, onScan, onUndo, onKeySaved, onError }: StartScreenProps) {
  const needsKey = backends !== null && !backends.claude_code && !backends.api_key;
  return (
    <section className="hero" aria-labelledby="start-title">
      <h2 id="start-title" className="hero__title">
        Tidy your Downloads
      </h2>
      <p className="hero__lede">
        Scan the folder to see where the space goes. Claude then proposes folders and points out what can go, and you
        approve every change before anything moves.
      </p>
      <button className="btn btn--primary btn--lg" onClick={onScan} disabled={busy !== null}>
        {busy === "scanning" ? "Scanning…" : "Scan Downloads"}
      </button>
      {needsKey && (
        <>
          <p className="toolbar__meta">Claude Code isn't installed. Add an API key to let Claude organize:</p>
          <ApiKeyForm onSaved={onKeySaved} onError={onError} />
        </>
      )}
      {undoAvailable && (
        <button className="btn btn--danger" onClick={onUndo} disabled={busy !== null}>
          Undo last apply ({undoAvailable.moved} moved, {undoAvailable.trashed} trashed)
        </button>
      )}
    </section>
  );
}
