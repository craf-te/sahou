import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import { SahouRejected, connect } from "../src/node.js";

const descBase = readFileSync(
  fileURLToPath(new URL("../../py/tests/fixtures/descriptor_base.json", import.meta.url)),
  "utf-8",
);

async function rejectedDiags(p: Promise<unknown>): Promise<{ code: string; message: string }[]> {
  try {
    await p;
  } catch (e) {
    if (e instanceof SahouRejected) return e.diags;
    throw e;
  }
  throw new Error("should be SahouRejected");
}

describe("node entry point: environment boundary", () => {
  it("link absent + spawn disabled → structured NO with link_unavailable (with steps)", async () => {
    const diags = await rejectedDiags(connect(descBase, { node: "display", port: 49731, spawnLink: false }));
    expect(diags[0].code).toBe("link_unavailable");
    expect(diags[0].message).toContain("sahou link");
  });

  it("spawn target binary not found → link_unavailable (with SAHOU_LINK_CMD guidance)", async () => {
    process.env.SAHOU_LINK_CMD = "sahou-definitely-not-exists";
    try {
      const diags = await rejectedDiags(connect(descBase, { node: "display", port: 49732 }));
      expect(diags[0].code).toBe("link_unavailable");
      expect(diags[0].message).toContain("SAHOU_LINK_CMD");
    } finally {
      delete process.env.SAHOU_LINK_CMD;
    }
  });

  it("an unknown node is a NO from the core's diagnostics, before transport", async () => {
    // The implementation runs in the order "inspect core → secure link → Session.open" (say NO as early as possible, at the right place).
    // The proof: even on a port with no link, it returns unknown_node rather than link_unavailable.
    const diags = await rejectedDiags(connect(descBase, { node: "ghost", port: 49733, spawnLink: false }));
    expect(diags[0].code).toBe("unknown_node");
  });
});
