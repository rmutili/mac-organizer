import { describe, expect, test } from "vitest";
import { locationLabel } from "./folders";

describe("locationLabel", () => {
  test("names the standard folders", () => {
    expect(locationLabel("downloads", null)).toBe("Downloads");
    expect(locationLabel("documents", null)).toBe("Documents");
  });

  test("uses the picked folder's name", () => {
    expect(locationLabel("picked", { name: "Projects 2026", path: "/Users/x/Projects 2026" })).toBe(
      "Projects 2026",
    );
  });

  test("falls back when no folder has been picked yet", () => {
    expect(locationLabel("picked", null)).toBe("a folder");
  });
});
