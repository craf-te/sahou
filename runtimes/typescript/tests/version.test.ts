// Drift guard: SAHOU_VERSION is a committed constant (the browser has no fs to read
// package.json at runtime); this test is what keeps it honest at release-bump time.
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import { SAHOU_VERSION } from "../src/version.js";

describe("version constant", () => {
  it("matches package.json (bump src/version.ts together with the package version)", () => {
    const pkg = JSON.parse(
      readFileSync(fileURLToPath(new URL("../package.json", import.meta.url)), "utf-8"),
    ) as { version: string };
    expect(SAHOU_VERSION).toBe(pkg.version);
  });
});
