import { useState } from "react";
import { api, errorMessage, type BackendStatus, type UndoAvailable } from "../../lib/api";
import { type Location, type PickedFolder, STANDARD_LOCATIONS, locationLabel } from "../../lib/folders";
import type { Busy } from "../Toolbar";

interface StartScreenProps {
  backends: BackendStatus | null;
  busy: Busy;
  undoAvailable: UndoAvailable | null;
  location: Location;
  picked: PickedFolder | null;
  onLocation: (location: Location) => void;
  onPickFolder: () => void;
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

interface FolderChooserProps {
  location: Location;
  picked: PickedFolder | null;
  isDisabled: boolean;
  onLocation: (location: Location) => void;
  onPickFolder: () => void;
}

function FolderChooser({ location, picked, isDisabled, onLocation, onPickFolder }: FolderChooserProps) {
  return (
    <div className="folder-chooser" role="group" aria-label="Folder to organize">
      {STANDARD_LOCATIONS.map((l) => (
        <button
          key={l.id}
          aria-pressed={location === l.id}
          disabled={isDisabled}
          onClick={() => onLocation(l.id)}
        >
          {l.label}
        </button>
      ))}
      {picked && (
        <button aria-pressed={location === "picked"} disabled={isDisabled} onClick={() => onLocation("picked")} title={picked.path}>
          {picked.name}
        </button>
      )}
      <button disabled={isDisabled} onClick={onPickFolder}>
        Other folder…
      </button>
    </div>
  );
}

export function StartScreen(props: StartScreenProps) {
  const { backends, busy, undoAvailable, location, picked, onScan, onUndo, onKeySaved, onError } = props;
  const folder = locationLabel(location, picked);
  const needsKey = backends !== null && !backends.claude_code && !backends.api_key;
  return (
    <section className="hero" aria-labelledby="start-title">
      <h2 id="start-title" className="hero__title">
        Tidy a folder
      </h2>
      <p className="hero__lede">
        Pick a folder and scan it to see where the space goes. Claude then proposes folders and points out what can go,
        and you approve every change before anything moves.
      </p>
      <FolderChooser
        location={location}
        picked={picked}
        isDisabled={busy !== null}
        onLocation={props.onLocation}
        onPickFolder={props.onPickFolder}
      />
      {location === "picked" && picked && <p className="toolbar__meta">{picked.path}</p>}
      <button className="btn btn--primary btn--lg" onClick={onScan} disabled={busy !== null}>
        {busy === "scanning" ? "Scanning…" : `Scan ${folder}`}
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
