import { describe, expect, test } from "vitest";
import type { FileItem, Plan } from "./api";
import {
  fullyTrashedDuplicateGroups,
  initialDecisions,
  itemsInView,
  setDecision,
  summarize,
  toDecisions,
  toggleTrash,
  viewCounts,
} from "./review";

function item(id: number, name: string, overrides: Partial<FileItem> = {}): FileItem {
  return {
    id,
    name,
    extension: "",
    category: "other",
    is_dir: false,
    size_bytes: 100 * (id + 1),
    file_count: 1,
    modified_at: 0,
    accessed_at: 0,
    duplicate_group: null,
    sensitive: false,
    ...overrides,
  };
}

const items = [
  item(0, "a.pdf"),
  item(1, "a (1).pdf", { duplicate_group: 0 }),
  item(2, "Docker.dmg"),
  item(3, "maybe.zip"),
  item(4, "client_secret.json", { sensitive: true }),
  item(5, "loose.txt"),
];

const plan: Plan = {
  folders: [
    { name: "Docs", description: "" },
    { name: "Installers", description: "" },
  ],
  moves: [
    { item_id: 0, folder: "Docs", reason: "pdf" },
    { item_id: 1, folder: "Docs", reason: "pdf" },
    { item_id: 2, folder: "Installers", reason: "dmg" },
    { item_id: 3, folder: "Installers", reason: "zip" },
    { item_id: 4, folder: "Docs", reason: "json" },
  ],
  deletions: [
    { item_id: 1, reason: "copy", confidence: "medium" },
    { item_id: 2, reason: "old installer", confidence: "high" },
    { item_id: 3, reason: "maybe", confidence: "low" },
  ],
  rejected: [],
};

describe("initialDecisions", () => {
  const decisions = initialDecisions(items, plan);

  test("pre-marks high-confidence and verified-duplicate deletions for the Trash", () => {
    expect(decisions[1]).toEqual({ kind: "trash" });
    expect(decisions[2]).toEqual({ kind: "trash" });
  });

  test("leaves uncertain deletions on their planned move", () => {
    expect(decisions[3]).toEqual({ kind: "move", folder: "Installers" });
  });

  test("keeps items Claude didn't place", () => {
    expect(decisions[5]).toEqual({ kind: "keep" });
  });
});

describe("decision updates", () => {
  test("setDecision returns a new object and leaves the original alone", () => {
    const before = initialDecisions(items, plan);
    const after = setDecision(before, 0, { kind: "keep" });

    expect(after[0]).toEqual({ kind: "keep" });
    expect(before[0]).toEqual({ kind: "move", folder: "Docs" });
  });

  test("toggleTrash marks an item, and unmarking restores its planned move", () => {
    const start = initialDecisions(items, plan);
    const trashed = toggleTrash(start, plan, 0);
    expect(trashed[0]).toEqual({ kind: "trash" });

    const restored = toggleTrash(trashed, plan, 0);
    expect(restored[0]).toEqual({ kind: "move", folder: "Docs" });
    expect(toggleTrash(toggleTrash(start, plan, 5), plan, 5)[5]).toEqual({ kind: "keep" });
  });
});

describe("views", () => {
  const decisions = initialDecisions(items, plan);

  test("folder views follow the current decision", () => {
    const docs = itemsInView({ kind: "folder", name: "Docs" }, items, plan, decisions);
    expect(docs.map((i) => i.id)).toEqual([0, 4]);
  });

  test("review views list suggestions, duplicates and credential files", () => {
    const ids = (view: Parameters<typeof itemsInView>[0]) =>
      itemsInView(view, items, plan, decisions).map((i) => i.id);
    expect(ids({ kind: "to-delete" })).toEqual([1, 2, 3]);
    expect(ids({ kind: "duplicates" })).toEqual([1]);
    expect(ids({ kind: "attention" })).toEqual([4]);
    expect(ids({ kind: "unchanged" })).toEqual([5]);
  });

  test("viewCounts reports items and bytes per view", () => {
    const counts = viewCounts(items, plan, decisions);
    expect(counts["folder:Docs"]).toEqual({ count: 2, bytes: 100 + 500 });
    expect(counts["to-delete"].count).toBe(3);
  });
});

describe("summary and payload", () => {
  test("counts moves, trash and the space it frees", () => {
    const decisions = initialDecisions(items, plan);
    expect(summarize(items, decisions)).toEqual({ moves: 3, trash: 2, freedBytes: 200 + 300 });
  });

  test("toDecisions sends IDs and folder names only, skipping kept items", () => {
    const decisions = setDecision(initialDecisions(items, plan), 4, { kind: "keep" });
    expect(toDecisions(decisions)).toEqual({
      moves: [
        { item_id: 0, folder: "Docs" },
        { item_id: 3, folder: "Installers" },
      ],
      trash: [1, 2],
    });
  });
});

describe("fullyTrashedDuplicateGroups", () => {
  const copies = [
    item(0, "x.pdf", { duplicate_group: 0 }),
    item(1, "x (1).pdf", { duplicate_group: 0 }),
    item(2, "y.pdf", { duplicate_group: 1 }),
    item(3, "y (1).pdf", { duplicate_group: 1 }),
  ];

  test("counts groups where no copy would stay", () => {
    const decisions = {
      0: { kind: "trash" },
      1: { kind: "trash" },
      2: { kind: "trash" },
      3: { kind: "keep" },
    } as const;
    expect(fullyTrashedDuplicateGroups(copies, decisions)).toBe(1);
  });
});
