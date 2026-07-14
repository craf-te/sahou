import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { loadCore } from "../src/core-node.js";
import { SahouRejected, SahouUnreachable, connect, type SahouNode } from "../src/node.js";
import { rawSession, sleep, spawnLink, waitFor, type LinkHandle } from "./helpers.js";

const fixture = (name: string) =>
  readFileSync(fileURLToPath(new URL(`../../py/tests/fixtures/${name}`, import.meta.url)), "utf-8");
const descBase = fixture("descriptor_base.json");
const core = loadCore();

let link: LinkHandle;
let a: SahouNode; // sensor = requester
let b: SahouNode; // display = responder

beforeAll(async () => {
  link = await spawnLink();
});
afterAll(async () => {
  await link?.stop();
});

async function freshPair(): Promise<void> {
  await b?.close();
  await a?.close();
  a = await connect(descBase, { node: "sensor", port: link.port, spawnLink: false });
  b = await connect(descBase, { node: "display", port: link.port, spawnLink: false });
}

describe("query: 4 boundaries + smart retry", () => {
  it("happy round-trip: answer → queryConfirmed returns on a 200-equivalent", async () => {
    await freshPair();
    await b.answer("ask", (req) => ({ level: req.sel === "levels" ? 3 : 0 }));
    const res = await a.queryConfirmed("ask", { sel: "levels" }, { timeoutMs: 1000, retries: 10, backoffMs: 100 });
    expect(res).toEqual({ level: 3 });
  });

  it("① a broken request is a send-boundary NO (does not even issue the get)", async () => {
    try {
      await a.query("ask", { sel: 123 } as never);
      expect.unreachable();
    } catch (e) {
      expect((e as SahouRejected).diags[0].code).toBe("type_mismatch");
    }
  });

  it("③ a broken response is fatal (4xx-equivalent; not resent)", async () => {
    await freshPair();
    let calls = 0;
    await b.answer("ask", () => {
      calls++;
      return { level: "high" } as never; // NO at the responder send boundary → reply_err
    });
    try {
      await a.queryConfirmed("ask", { sel: "x" }, { timeoutMs: 1000, retries: 10, backoffMs: 100 });
      expect.unreachable();
    } catch (e) {
      expect(e).toBeInstanceOf(SahouRejected);
      expect((e as SahouRejected).diags.some((d) => d.code === "type_mismatch")).toBe(true);
    }
    expect(calls).toBeLessThanOrEqual(2); // fatal is not resent (only discovery retries are allowed)
  });

  it("handler exception → handler_error (5xx-equivalent; retryable)", async () => {
    await freshPair();
    await b.answer("ask", () => {
      throw new Error("boom");
    });
    let r: Awaited<ReturnType<SahouNode["query"]>> | undefined;
    const found = await waitFor(async () => {
      r = await a.query("ask", { sel: "x" }, { timeoutMs: 1500 });
      return r.diags.length > 0;
    }, 10_000);
    expect(found).toBe(true);
    expect(r?.diags[0].code).toBe("handler_error");
    expect(core.wasm_classify_delivery(false, JSON.stringify(r?.diags))).toBe("retryable");
  });

  it("temporarily absent → recovers via retry / fully absent → SahouUnreachable", async () => {
    await freshPair();
    void (async () => {
      await sleep(1000);
      await b.answer("ask", () => ({ level: 7 }));
    })();
    const res = await a.queryConfirmed("ask", { sel: "x" }, { timeoutMs: 500, retries: 15, backoffMs: 200 });
    expect(res).toEqual({ level: 7 });

    await freshPair(); // no answer registered
    await expect(
      a.queryConfirmed("ask", { sel: "x" }, { timeoutMs: 300, retries: 2, backoffMs: 50 }),
    ).rejects.toBeInstanceOf(SahouUnreachable);
  });

  it("a broken reply_err envelope → bad_reply_envelope (retryable; does not misfire as FATAL)", async () => {
    await freshPair();
    const raw = await rawSession(link.port);
    const key = a.connectionInfo("ask").key;
    const q = await raw.declareQueryable(key, {
      handler: async (query) => {
        try {
          await query.replyErr("garbage not json");
        } finally {
          await query.finalize();
        }
      },
    });
    let r: Awaited<ReturnType<SahouNode["query"]>> | undefined;
    await waitFor(async () => {
      r = await a.query("ask", { sel: "x" }, { timeoutMs: 1500 });
      return r.diags.length > 0;
    }, 10_000);
    expect(r?.diags[0].code).toBe("bad_reply_envelope");
    await expect(
      a.queryConfirmed("ask", { sel: "x" }, { timeoutMs: 500, retries: 1, backoffMs: 50 }),
    ).rejects.toBeInstanceOf(SahouUnreachable);
    await q.undeclare();
    await raw.close();
  });
});
