import { describe, expect, test } from "vitest";
import { formatAge, formatBytes } from "./format";

describe("formatBytes", () => {
  test("uses binary units with one decimal above bytes", () => {
    expect(formatBytes(0)).toBe("0 B");
    expect(formatBytes(512)).toBe("512 B");
    expect(formatBytes(1536)).toBe("1.5 KB");
    expect(formatBytes(63.3 * 1024 ** 3)).toBe("63.3 GB");
  });
});

describe("formatAge", () => {
  const now = 1_800_000_000;
  const day = 86_400;

  test("describes recent and old timestamps in plain words", () => {
    expect(formatAge(now - 60, now)).toBe("today");
    expect(formatAge(now - day, now)).toBe("yesterday");
    expect(formatAge(now - 12 * day, now)).toBe("12 days ago");
    expect(formatAge(now - 75 * day, now)).toBe("2 months ago");
    expect(formatAge(now - 800 * day, now)).toBe("2 years ago");
  });

  test("treats future timestamps as today", () => {
    expect(formatAge(now + day, now)).toBe("today");
  });
});
