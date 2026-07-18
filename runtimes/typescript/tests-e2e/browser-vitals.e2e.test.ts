// Real-browser vitals e2e (spec notes/sahou-vitals-spec.md §5 verify item 1, browser side):
// prove that the browser entry (src/browser.ts), running in a real headless Chromium, connects
// through a real `sahou link` (WS), declares its liveliness token + vitals queryable, serves a
// core-built payload with runtime.transport === "browser" (zenoh omitted, not faked), and that
// closing the page (the crash-equivalent WS drop for a tab) removes the token within the grace
// window. One test — the browser + link + observer setup is expensive.
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import type { Session } from "@eclipse-zenoh/zenoh-ts";
import { type Browser, chromium } from "playwright";
import { afterEach, describe, expect, it } from "vitest";
import type { CoreRuntime } from "../src/core.js";
import { loadCore } from "../src/core-node.js";
import { fetchOne, freePort, rawSession, spawnLink, tokenVisible, waitFor, type LinkHandle } from "../tests/helpers.js";
import { serveDir } from "./server.js";

const FIXTURE_DIST = fileURLToPath(new URL("./dist-fixture", import.meta.url));
const readText = (rel: string) => readFileSync(fileURLToPath(new URL(rel, import.meta.url)), "utf-8");
const descBase = readText("../../python/tests/fixtures/descriptor_base.json");
const pkgVersion = (JSON.parse(readText("../package.json")) as { version: string }).version;

/** The core is the single source of the key shape (do not hand-derive keys in tests). */
function withCore<T>(fn: (rt: CoreRuntime) => T): T {
  const rt = new (loadCore().WasmRuntime)(descBase);
  try {
    return fn(rt);
  } finally {
    rt.free();
  }
}

let link: LinkHandle | undefined;
let browser: Browser | undefined;
let raw: Session | undefined;

afterEach(async () => {
  await browser?.close();
  await raw?.close();
  await link?.stop();
  browser = raw = link = undefined;
});

describe("browser vitals (real Chromium via the browser entry)", () => {
  it("declares vitals over a real WS link; the token drops when the page closes", async () => {
    link = await spawnLink();
    const site = await serveDir(FIXTURE_DIST, await freePort());
    try {
      browser = await chromium.launch(); // headless by default
      const page = await browser.newPage();
      // surface fixture / page errors into the test log (aids diagnosing a failed load)
      page.on("console", (m) => (m.type() === "error" ? console.error("[page]", m.text()) : undefined));
      page.on("pageerror", (e) => console.error("[pageerror]", e));
      await page.goto(`http://127.0.0.1:${site.port}/?link=${link.port}&node=sensor`);

      // First load fetches + instantiates two wasm modules (sahou core + zenoh-ts) → generous wait.
      const status = () => page.evaluate(() => window.__status);
      expect(
        await waitFor(async () => (await status()) === "connected", 45_000),
        `browser did not connect (last status: ${await status()})`,
      ).toBe(true);

      // Node-side observer. The vitals key comes from the core, never hand-derived.
      raw = await rawSession(link.port);
      const observer = raw;
      const vkey = withCore((rt) => rt.vitals_key("sensor"));

      expect(await waitFor(() => tokenVisible(observer, vkey)), "liveliness token not visible").toBe(true);

      let payload: string | null = null;
      expect(
        await waitFor(async () => {
          payload = await fetchOne(observer, vkey);
          return payload !== null;
        }),
        "vitals query returned nothing",
      ).toBe(true);
      const v = JSON.parse(payload!);
      // mirror tests/vitals.test.ts, but for the browser transport profile
      expect(v.vitals_format).toBe(1);
      expect(v.node).toBe("sensor");
      expect(v.runtime.lang).toBe("typescript");
      expect(v.runtime.transport).toBe("browser");
      expect(v.runtime.sahou).toBe(pkgVersion); // the version.ts constant == package.json (spec §1.2)
      expect("zenoh" in v.runtime).toBe(false); // zenoh-ts version is not discoverable in a browser → omitted, not faked
      expect(Number.isInteger(v.uptime_secs) && v.uptime_secs >= 0).toBe(true);
      expect(Object.keys(v.connections).length).toBeGreaterThan(0);
      for (const c of Object.values(v.connections) as { role: string; hash: string }[]) {
        expect(["from", "to"]).toContain(c.role);
        expect(c.hash).toHaveLength(16);
      }

      // Closing the page drops the WS (a tab close / crash) → the token disappears within the grace
      // window (~0.5 s empirically on the node path; allow up to ~10 s for CI headroom).
      await page.close();
      expect(
        await waitFor(async () => !(await tokenVisible(observer, vkey)), 10_000),
        "the token should be auto-removed after the page closes",
      ).toBe(true);
    } finally {
      await site.close();
    }
  });
});
