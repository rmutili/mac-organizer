import { useCallback, useEffect, useMemo, useState } from "react";
import { isTauri } from "@tauri-apps/api/core";
import {
  api,
  CANCELLED,
  errorMessage,
  events,
  type ApplyReport,
  type BackendStatus,
  type ClassifyProgress,
  type ModelId,
  type Plan,
  type ScanProgress,
  type ScanResponse,
  type UndoAvailable,
  type UndoReport,
} from "./lib/api";
import {
  type Decision,
  type DecisionMap,
  fullyTrashedDuplicateGroups,
  initialDecisions,
  setDecision,
  summarize,
  toDecisions,
  toggleTrash,
} from "./lib/review";
import { type Location, type PickedFolder, locationLabel } from "./lib/folders";
import { type Busy, Toolbar } from "./components/Toolbar";
import { StartScreen } from "./components/start/StartScreen";
import { HeatmapView } from "./components/overview/HeatmapView";
import { Explorer } from "./components/review/Explorer";
import { ActionBar } from "./components/review/ActionBar";
import { ApplySheet } from "./components/review/ApplySheet";

type Sheet =
  | { mode: "confirm" }
  | { mode: "applied"; report: ApplyReport }
  | { mode: "undone"; report: UndoReport }
  | null;

const NOT_IN_APP = "This page is running in a regular browser. Use the Mac Organizer window opened by `pnpm tauri dev`.";

export default function App() {
  const [backends, setBackends] = useState<BackendStatus | null>(null);
  const [model, setModel] = useState<ModelId>("claude-haiku-5-5");
  const [location, setLocation] = useState<Location>("downloads");
  const [picked, setPicked] = useState<PickedFolder | null>(null);
  const [scan, setScan] = useState<ScanResponse | null>(null);
  const [plan, setPlan] = useState<Plan | null>(null);
  const [decisions, setDecisions] = useState<DecisionMap>({});
  const [busy, setBusy] = useState<Busy>(null);
  const [scanProgress, setScanProgress] = useState<ScanProgress | null>(null);
  const [classifyProgress, setClassifyProgress] = useState<ClassifyProgress | null>(null);
  const [undoAvailable, setUndoAvailable] = useState<UndoAvailable | null>(null);
  const [sheet, setSheet] = useState<Sheet>(null);
  const [error, setError] = useState<string | null>(isTauri() ? null : NOT_IN_APP);

  const refreshUndo = useCallback(() => {
    api.lastApply().then(setUndoAvailable, (e) => setError(errorMessage(e)));
  }, []);

  useEffect(() => {
    if (!isTauri()) return;
    api.backends().then(setBackends, (e) => setError(errorMessage(e)));
    refreshUndo();
    const unlisten = [events.onScanProgress(setScanProgress), events.onClassifyProgress(setClassifyProgress)];
    return () => unlisten.forEach((u) => u.then((stop) => stop()));
  }, [refreshUndo]);

  async function run(kind: Exclude<Busy, null>, action: () => Promise<void>) {
    if (!isTauri()) {
      setError(NOT_IN_APP);
      return;
    }
    setBusy(kind);
    setError(null);
    try {
      await action();
    } catch (e) {
      const message = errorMessage(e);
      // Cancelling is the person's choice, not an error.
      if (message !== CANCELLED) setError(message);
    } finally {
      setBusy(null);
      setScanProgress(null);
      setClassifyProgress(null);
    }
  }

  const clearReview = () => {
    setPlan(null);
    setDecisions({});
  };

  const doScan = () =>
    run("scanning", async () => {
      setSheet(null);
      clearReview();
      setScan(await api.scan(location));
      refreshUndo();
    });

  const doOrganize = () =>
    run("organizing", async () => {
      if (!scan) return;
      const next = await api.classify(model);
      setPlan(next);
      setDecisions(initialDecisions(scan.items, next));
    });

  const doApply = () =>
    run("applying", async () => {
      const report = await api.apply(toDecisions(decisions));
      setScan(null);
      clearReview();
      setSheet({ mode: "applied", report });
      refreshUndo();
    });

  const doUndo = () =>
    run("applying", async () => {
      const report = await api.undoLastApply();
      setScan(null);
      clearReview();
      setSheet({ mode: "undone", report });
      refreshUndo();
    });

  const doPickFolder = async () => {
    setError(null);
    try {
      const folder = await api.pickFolder();
      if (!folder) return;
      setPicked(folder);
      setLocation("picked");
    } catch (e) {
      setError(errorMessage(e));
    }
  };

  const changeFolder = () => {
    setSheet(null);
    setScan(null);
    clearReview();
  };

  const doCancel = () => {
    api.cancelClassify().catch((e) => setError(errorMessage(e)));
  };

  const decide = (id: number, decision: Decision) => setDecisions((d) => setDecision(d, id, decision));
  const toggle = (id: number) => {
    if (plan) setDecisions((d) => toggleTrash(d, plan, id));
  };
  const summary = useMemo(() => summarize(scan?.items ?? [], decisions), [scan, decisions]);
  const canOrganize = !!scan && !!backends && (backends.claude_code || backends.api_key);
  const backendLabel = backends?.claude_code
    ? "Using your Claude Code login"
    : backends?.api_key
      ? "Using your API key"
      : "";

  return (
    <div className="app">
      <Toolbar
        busy={busy}
        hasScan={!!scan}
        folderName={scan?.name ?? locationLabel(location, picked)}
        canOrganize={canOrganize}
        backendLabel={backendLabel}
        model={model}
        scanProgress={scanProgress}
        classifyProgress={classifyProgress}
        onModel={setModel}
        onScan={doScan}
        onOrganize={doOrganize}
        onCancel={doCancel}
        onChangeFolder={changeFolder}
      />

      <main style={{ display: "grid", gridTemplateRows: error ? "auto 1fr" : "1fr", minHeight: 0 }}>
        {error && (
          <p className="banner" role="alert">
            {error}
          </p>
        )}
        {scan && plan ? (
          <Explorer
            scan={scan}
            plan={plan}
            decisions={decisions}
            onDecide={decide}
            onToggleTrash={toggle}
            onError={setError}
          />
        ) : scan ? (
          <HeatmapView
            scan={scan}
            footer={
              <div className="overview__cta">
                <p>
                  Ask Claude to propose folders for these {scan.items.length.toLocaleString()} items and point out what
                  can go. Nothing changes until you review and apply.
                </p>
                <button
                  className="btn btn--primary btn--lg"
                  onClick={doOrganize}
                  disabled={busy !== null || !canOrganize}
                >
                  Organize with Claude
                </button>
              </div>
            }
          />
        ) : (
          <StartScreen
            backends={backends}
            busy={busy}
            undoAvailable={undoAvailable}
            location={location}
            picked={picked}
            onLocation={setLocation}
            onPickFolder={doPickFolder}
            onScan={doScan}
            onUndo={doUndo}
            onKeySaved={() => setBackends((b) => (b ? { ...b, api_key: true } : b))}
            onError={setError}
          />
        )}
      </main>

      {scan && plan && (
        <ActionBar summary={summary} isBusy={busy !== null} onApply={() => setSheet({ mode: "confirm" })} />
      )}

      {sheet?.mode === "confirm" && scan && (
        <ApplySheet
          mode="confirm"
          folderName={scan.name}
          summary={summary}
          allCopiesTrashed={fullyTrashedDuplicateGroups(scan.items, decisions)}
          isBusy={busy === "applying"}
          onConfirm={doApply}
          onCancel={() => setSheet(null)}
        />
      )}
      {sheet?.mode === "applied" && (
        <ApplySheet mode="applied" report={sheet.report} onUndo={doUndo} onDone={doScan} />
      )}
      {sheet?.mode === "undone" && <ApplySheet mode="undone" report={sheet.report} onDone={doScan} />}
    </div>
  );
}
