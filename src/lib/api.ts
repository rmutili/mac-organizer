// Typed wrappers around the Rust commands and events. The webview only ever
// sends item IDs and folder names; Rust resolves them to paths.
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type Category =
  | "folder" | "image" | "video" | "audio" | "document" | "spreadsheet"
  | "presentation" | "archive" | "installer" | "code" | "other";

export type AgeBucket = "week" | "month" | "quarter" | "year" | "older";

export interface FileItem {
  id: number;
  name: string;
  extension: string;
  category: Category;
  is_dir: boolean;
  size_bytes: number;
  file_count: number;
  modified_at: number;
  accessed_at: number;
  duplicate_group: number | null;
  sensitive: boolean;
}

export interface HeatmapCell {
  category: Category;
  age: AgeBucket;
  size_bytes: number;
  file_count: number;
}

export interface ScanResponse {
  root: string;
  items: FileItem[];
  skipped: string[];
  heatmap: { cells: HeatmapCell[]; total_size_bytes: number; total_file_count: number };
}

export type Confidence = "low" | "medium" | "high";

export interface Plan {
  folders: { name: string; description: string }[];
  moves: { item_id: number; folder: string; reason: string }[];
  deletions: { item_id: number; reason: string; confidence: Confidence }[];
  rejected: { suggestion: string; reason: string }[];
}

export interface BackendStatus {
  claude_code: boolean;
  api_key: boolean;
}

export interface ScanProgress {
  phase: "listing" | "duplicates";
  done: number;
  total: number;
  name: string;
}

export interface ClassifyProgress {
  done: number;
  total: number;
  sorting: string[];
  sorting_count: number;
}

export interface Failure {
  name: string;
  error: string;
}

export interface ApplyReport {
  moved: number;
  trashed: number;
  freed_bytes: number;
  failures: Failure[];
}

export interface UndoAvailable {
  applied_at: number;
  moved: number;
  trashed: number;
}

export interface UndoReport {
  restored: number;
  failures: Failure[];
}

export interface Decisions {
  moves: { item_id: number; folder: string }[];
  trash: number[];
}

export const MODELS = [
  { id: "claude-haiku-5-5", label: "Haiku · fast" },
  { id: "claude-opus-5-5", label: "Opus · most careful" },
] as const;

export type ModelId = (typeof MODELS)[number]["id"];

export const api = {
  backends: () => invoke<BackendStatus>("claude_backends"),
  setApiKey: (key: string) => invoke<void>("set_api_key", { key }),
  scan: () => invoke<ScanResponse>("scan_downloads"),
  classify: (model: ModelId) => invoke<Plan>("classify_downloads", { model }),
  cancelClassify: () => invoke<void>("cancel_classify"),
  apply: (decisions: Decisions) => invoke<ApplyReport>("apply_plan", { decisions }),
  lastApply: () => invoke<UndoAvailable | null>("last_apply"),
  undoLastApply: () => invoke<UndoReport>("undo_last_apply"),
  thumbnail: (itemId: number) => invoke<string | null>("thumbnail", { itemId }),
  quickLook: (itemId: number) => invoke<void>("quick_look", { itemId }),
  revealInFinder: (itemId: number) => invoke<void>("reveal_in_finder", { itemId }),
};

export const events = {
  onScanProgress: (handler: (p: ScanProgress) => void): Promise<UnlistenFn> =>
    listen<ScanProgress>("scan-progress", (e) => handler(e.payload)),
  onClassifyProgress: (handler: (p: ClassifyProgress) => void): Promise<UnlistenFn> =>
    listen<ClassifyProgress>("classify-progress", (e) => handler(e.payload)),
};

export function errorMessage(error: unknown): string {
  if (error instanceof Error) return error.message;
  if (typeof error === "string") return error;
  return "Something went wrong.";
}

/** The backend's error for a run the person cancelled (`processes::CANCELLED`). */
export const CANCELLED = "Cancelled.";
