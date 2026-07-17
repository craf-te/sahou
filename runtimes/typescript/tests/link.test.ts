import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { spawn } from "node:child_process";
import { afterEach, describe, expect, it } from "vitest";
import { connect, type SahouNode } from "../src/node.js";
import { SAHOU_BIN, freePort, portOpen, pump, sleep, spawnLink, waitFor } from "./helpers.js";

const fixture = (name: string) =>
  readFileSync(fileURLToPath(new URL(`../../python/tests/fixtures/${name}`, import.meta.url)), "utf-8");
const descBase = fixture("descriptor_base.json");

afterEach(() => {
  delete process.env.SAHOU_LINK_CMD;
  delete process.env.SAHOU_LINK_ARGS;
});

describe("link lifecycle (auto spawn / idle shutdown / sharing / no orphans)", () => {
  it("connect() auto-spawns a link, and after all clients disconnect it self-terminates on grace", async () => {
    const port = await freePort();
    process.env.SAHOU_LINK_CMD = SAHOU_BIN;
    process.env.SAHOU_LINK_ARGS = "--no-multicast --grace 4 --startup 30";
    expect(await portOpen(port)).toBe(false);
    const node = await connect(descBase, { node: "display", port });
    expect(await portOpen(port)).toBe(true); // auto spawn succeeded
    // The link's idle monitor samples on a 2-second tick (cli/src/link.rs TICK). Doing connect→close
    // in less than a tick means "no connection is ever observed, so it never becomes armed" → it falls
    // into the startup-timeout branch (by idle_step's design). Keep the connection for at least 1 tick.
    await sleep(2500);
    await node.close();
    // wait for the idle grace (4s) + the monitor tick → self-termination (no orphans)
    expect(await waitFor(async () => !(await portOpen(port)), 20_000, 500)).toBe(true);
  });

  it("shares an existing link: two nodes connect → survives one close → terminates on all close", async () => {
    const link = await spawnLink(["--grace", "4"]);
    const a = await connect(descBase, { node: "sensor", port: link.port, spawnLink: false });
    const b = await connect(descBase, { node: "display", port: link.port, spawnLink: false });
    const got: unknown[] = [];
    await b.subscribe("touch", (p) => got.push(p));
    await pump(a, "touch", { x: 0.5, phase: "move" }, () => got.length > 0);
    // let the idle monitor tick (2-second granularity) observe "a client is present" at least once
    // (connect→immediate close never becomes armed and falls into the startup-timeout branch; see the test above)
    await sleep(2500);
    await a.close();
    expect(await portOpen(link.port)).toBe(true); // stays alive while b remains
    await b.close();
    expect(await waitFor(async () => !(await portOpen(link.port)), 20_000, 500)).toBe(true);
    await link.stop(); // idempotent (already dead)
  });

  it("self-terminates on startup timeout if no client ever arrives", async () => {
    const port = await freePort();
    const child = spawn(SAHOU_BIN, ["link", "--port", String(port), "--no-multicast", "--startup", "3", "--grace", "2"], {
      stdio: "ignore",
    });
    const exited = await new Promise<boolean>((res) => {
      const t = setTimeout(() => res(false), 15_000);
      child.on("exit", () => {
        clearTimeout(t);
        res(true);
      });
    });
    expect(exited).toBe(true);
    expect(await portOpen(port)).toBe(false);
  });
});
