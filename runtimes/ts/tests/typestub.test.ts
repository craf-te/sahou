// Demonstrate the effectiveness of the generated stub with tsc --noEmit (design §8; main battleground ②).
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const tscJs = fileURLToPath(new URL("../node_modules/typescript/lib/tsc.js", import.meta.url));
const fixture = (name: string) => fileURLToPath(new URL(`./typestub/${name}`, import.meta.url));

function runTsc(file: string): { ok: boolean; out: string } {
  const r = spawnSync(
    process.execPath,
    [tscJs, "--noEmit", "--strict", "--target", "es2022", "--module", "nodenext", "--moduleResolution", "nodenext", file],
    { encoding: "utf8" },
  );
  return { ok: r.status === 0, out: `${r.stdout}\n${r.stderr}` };
}

describe("type stub (tsc --noEmit)", () => {
  it("correct usage is clean (handler arguments are inferred)", () => {
    const r = runTsc(fixture("stub_ok.mts"));
    expect(r.out.trim() === "" || r.ok, r.out).toBe(true);
    expect(r.ok, r.out).toBe(true);
  });

  it("type mismatches, unknown connections, and non-participating directions turn red", () => {
    const r = runTsc(fixture("stub_bad.mts"));
    expect(r.ok, "broken usage passed clean (the stub is not working)").toBe(false);
    expect(r.out).toContain('"ghost"'); // literal mismatch for an unknown connection
    expect(r.out).toContain("publish"); // publish does not exist on visuals
    expect(r.out).toMatch(/stub_bad\.mts\(7,\d+\)/); // L7: number → string type mismatch
  });
});
