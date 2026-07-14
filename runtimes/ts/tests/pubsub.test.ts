import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { SahouRejected, connect, type SahouNode } from "../src/node.js";
import { pump, pumpRaw, rawSession, spawnLink, waitFor, type LinkHandle } from "./helpers.js";

const fixture = (name: string) =>
  readFileSync(fileURLToPath(new URL(`../../py/tests/fixtures/${name}`, import.meta.url)), "utf-8");
const descBase = fixture("descriptor_base.json");

let link: LinkHandle;
let a: SahouNode; // sensor (from)
let b: SahouNode; // display (to)

beforeAll(async () => {
  link = await spawnLink();
  a = await connect(descBase, { node: "sensor", port: link.port, spawnLink: false });
  b = await connect(descBase, { node: "display", port: link.port, spawnLink: false });
});

afterAll(async () => {
  await b?.close();
  await a?.close();
  await link?.stop();
});

describe("pub_sub (Node↔Node via link)", () => {
  it("happy path: publish → subscribe is delivered", async () => {
    const got: unknown[] = [];
    await b.subscribe("touch", (p) => got.push(p));
    expect(await pump(a, "touch", { x: 0.5, phase: "move" }, () => got.length > 0)).toBe(true);
    expect(got[0]).toEqual({ x: 0.5, phase: "move" });
  });

  it("send boundary: a broken payload is not put and throws SahouRejected / a non-from node gets role_mismatch", async () => {
    await expect(a.publish("touch", { x: "bad", phase: "move" })).rejects.toBeInstanceOf(SahouRejected);
    try {
      await b.publish("touch", { x: 0.5, phase: "move" });
      expect.unreachable();
    } catch (e) {
      expect((e as SahouRejected).diags[0].code).toBe("role_mismatch");
    }
  });

  it("receive boundary: a raw send that bypasses validation does not reach the handler and is a counted NO", async () => {
    const rejects: string[] = [];
    const got: unknown[] = [];
    await b.subscribe("touch", (p) => got.push(p), { onReject: (_c, d) => rejects.push(d[0].code) });
    const raw = await rawSession(link.port);
    const key = b.connectionInfo("touch").key;
    const hash = b.connectionInfo("touch").hash;
    // broken payload + correct hash → type_mismatch
    await pumpRaw(raw, key, '{"x":"bad","phase":"move"}', () => rejects.includes("type_mismatch"), { attachment: hash });
    expect(rejects).toContain("type_mismatch");
    // no attachment → missing_schema_hash (do not silently let a non-sahou sender through)
    await pumpRaw(raw, key, '{"x":0.5,"phase":"move"}', () => rejects.includes("missing_schema_hash"));
    expect(rejects).toContain("missing_schema_hash");
    expect(got).toEqual([]);
    await raw.close();
  });

  it("receive boundary: a non-UTF-8 attachment does not swallow the exception and is a NO as missing_schema_hash", async () => {
    // ②a final review (e56dc1d): a regression guard for fixing a non-UTF-8 attachment that used to be silent
    // in a catch-all (no reject count / no on_reject) into a structured NO with missing_schema_hash
    // (symmetric with the Python version test_non_utf8_attachment_is_rejected_not_silent).
    const rejects: string[] = [];
    const got: unknown[] = [];
    await b.subscribe("touch", (p) => got.push(p), { onReject: (_c, d) => rejects.push(d[0].code) });
    const raw = await rawSession(link.port);
    const key = b.connectionInfo("touch").key;
    const badAttachment = new Uint8Array([0xff, 0xfe, 0x00]); // non-UTF-8: input that makes TextDecoder(fatal) throw
    await pumpRaw(raw, key, '{"x":0.5,"phase":"move"}', () => rejects.includes("missing_schema_hash"), {
      attachment: badAttachment,
    });
    expect(rejects).toContain("missing_schema_hash");
    expect(got).toEqual([]);
    await raw.close();
  });

  it("send boundary: publishing to a connection not in the contract is not put and gives unknown_connection", async () => {
    try {
      await a.publish("ghost", { x: 0.5 });
      expect.unreachable();
    } catch (e) {
      expect(e).toBeInstanceOf(SahouRejected);
      expect((e as SahouRejected).diags[0].code).toBe("unknown_connection");
    }
  });
});
