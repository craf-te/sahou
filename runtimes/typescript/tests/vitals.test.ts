// Vitals (spec: notes/sahou-vitals-spec.md): the engine declares a liveliness token +
// a vitals queryable at <ns>/@sahou/vitals/<node>; the payload is built by the core.
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { Session } from "@eclipse-zenoh/zenoh-ts";
import { afterEach, describe, expect, it } from "vitest";
import type { CoreRuntime } from "../src/core.js";
import { loadCore } from "../src/core-node.js";
import { SahouNode, connect } from "../src/node.js";
import { fetchOne, rawSession, spawnLink, tokenVisible, waitFor, type LinkHandle } from "./helpers.js";

const fixture = (name: string) =>
  readFileSync(fileURLToPath(new URL(`../../python/tests/fixtures/${name}`, import.meta.url)), "utf-8");
const descBase = fixture("descriptor_base.json");
const pkgVersion = (rel: string): string =>
  (JSON.parse(readFileSync(fileURLToPath(new URL(rel, import.meta.url)), "utf-8")) as { version: string }).version;

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
let node: SahouNode | undefined;
let raw: Session | undefined;

afterEach(async () => {
  await node?.close();
  await raw?.close();
  await link?.stop();
  node = raw = link = undefined;
});

describe("vitals (node engine via link)", () => {
  it("vitals queryable serves a versioned payload built by the core", async () => {
    link = await spawnLink();
    node = await connect(descBase, { node: "sensor", port: link.port, spawnLink: false });
    raw = await rawSession(link.port);
    const observer = raw;
    const vkey = withCore((rt) => rt.vitals_key("sensor"));
    let payload: string | null = null;
    expect(
      await waitFor(async () => {
        payload = await fetchOne(observer, vkey);
        return payload !== null;
      }),
      "vitals query returned nothing",
    ).toBe(true);
    const v = JSON.parse(payload!);
    expect(v.vitals_format).toBe(1);
    expect(v.node).toBe("sensor");
    expect(v.runtime.lang).toBe("typescript");
    expect(v.runtime.transport).toBe("ws-link");
    // versions come from package metadata on disk, not faked (spec §1.2)
    expect(v.runtime.sahou).toBe(pkgVersion("../package.json"));
    expect(v.runtime.zenoh).toBe(pkgVersion("../node_modules/@eclipse-zenoh/zenoh-ts/package.json"));
    expect(Number.isInteger(v.uptime_secs) && v.uptime_secs >= 0).toBe(true);
    expect(Object.keys(v.connections).length).toBeGreaterThan(0);
    for (const c of Object.values(v.connections) as { role: string; hash: string }[]) {
      expect(["from", "to"]).toContain(c.role);
      expect(c.hash).toHaveLength(16);
    }
    expect(v.handshake).toEqual({}); // no mismatches seen in this test
  });

  it("liveliness token appears and disappears on close", async () => {
    link = await spawnLink();
    node = await connect(descBase, { node: "sensor", port: link.port, spawnLink: false });
    raw = await rawSession(link.port);
    const observer = raw;
    const vkey = withCore((rt) => rt.vitals_key("sensor"));
    expect(await waitFor(() => tokenVisible(observer, vkey)), "liveliness token not visible").toBe(true);
    await node.close();
    node = undefined;
    expect(
      await waitFor(async () => !(await tokenVisible(observer, vkey))),
      "the token should be auto-removed after close",
    ).toBe(true);
  });

  it("vitals: false declares neither the token nor the queryable", async () => {
    link = await spawnLink();
    node = await connect(descBase, { node: "sensor", port: link.port, spawnLink: false, vitals: false });
    raw = await rawSession(link.port);
    const observer = raw;
    // first prove the mesh routes at all, via a contract queryable this node DOES declare
    const info = node.connectionInfo("touch");
    const ns = withCore((rt) => rt.namespace());
    const contractKey = `${ns}/@sahou/contract/touch/${info.hash}`;
    expect(
      await waitFor(async () => (await fetchOne(observer, contractKey)) !== null),
      "mesh did not converge (contract queryable unreachable)",
    ).toBe(true);
    // …then assert the vitals surface is absent, distinguishing opt-out from non-convergence
    const vkey = withCore((rt) => rt.vitals_key("sensor"));
    expect(await fetchOne(observer, vkey)).toBeNull();
    expect(await tokenVisible(observer, vkey)).toBe(false);
  });

  it("a browser-shaped seed reports transport 'browser' and omits zenoh (engine-level)", async () => {
    // The browser entry cannot run under vitest; this exercises the same shared-engine path
    // with the exact seed browser.ts passes ({ sahou, transport: "browser" }, no zenoh key).
    link = await spawnLink();
    const core = loadCore();
    const session = await rawSession(link.port);
    node = await SahouNode.create(core, session, descBase, "sensor", {
      sahou: "0.0.0-test",
      transport: "browser",
    });
    raw = await rawSession(link.port);
    const observer = raw;
    const vkey = withCore((rt) => rt.vitals_key("sensor"));
    let payload: string | null = null;
    expect(
      await waitFor(async () => {
        payload = await fetchOne(observer, vkey);
        return payload !== null;
      }),
      "vitals query returned nothing",
    ).toBe(true);
    const v = JSON.parse(payload!);
    expect(v.runtime.transport).toBe("browser");
    expect(v.runtime.sahou).toBe("0.0.0-test");
    expect("zenoh" in v.runtime).toBe(false); // omitted, not faked
  });
});
