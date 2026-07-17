import { execFileSync, spawn, type ChildProcess } from "node:child_process";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { loadCore } from "../src/core-node.js";
import { connect, type Diag, type SahouNode } from "../src/node.js";
import { spawnLink, waitFor, type LinkHandle } from "./helpers.js";

const fixture = (name: string) =>
  readFileSync(fileURLToPath(new URL(`../../python/tests/fixtures/${name}`, import.meta.url)), "utf-8");
const pyDir = fileURLToPath(new URL("../../python", import.meta.url));
const script = (name: string) => fileURLToPath(new URL(`./cross/${name}`, import.meta.url));
const core = loadCore();
const enc = new TextEncoder();

/**
 * Reliably kill the `uv run python ...` child process.
 * gotcha (demonstrated in this task): Windows has no exec (process image replacement), so `uv` spawns
 * python as a separate process (on Unix, uv usually replaces its own process with python via exec, so
 * killing the parent immediately kills the child, but that does not happen on Windows).
 * ChildProcess.kill() only terminates `uv.exe`; the grandchild `python.exe` survives as an orphan and
 * keeps publishing (this caused contamination where a previous test's sends leaked into a later test's receives).
 * `taskkill /t /f` reliably kills the whole process tree.
 */
function killTree(proc: ChildProcess | undefined): void {
  if (!proc || proc.pid === undefined) return;
  if (process.platform === "win32") {
    try {
      execFileSync("taskkill", ["/pid", String(proc.pid), "/t", "/f"], { stdio: "ignore" });
    } catch {
      // best-effort if the target has already exited, etc. (do not throw during test teardown)
    }
  } else {
    proc.kill();
  }
}

describe("cross-language: ABI byte-identical (PyO3 = wasm)", () => {
  it("the raw envelope strings are byte-identical in every case", () => {
    // Python side (run PyO3 in the uv environment)
    const out = execFileSync("uv", ["run", "python", script("abi_dump.py")], {
      cwd: pyDir,
      encoding: "utf8",
      env: { ...process.env, PYTHONUTF8: "1" },
    });
    const py = new Map<string, string>();
    for (const line of out.split("\n")) {
      if (!line.trim()) continue;
      const i = line.indexOf("\t");
      py.set(line.slice(0, i), line.slice(i + 1));
    }

    // wasm side (inputs 1:1 with abi_dump.py)
    const base = fixture("descriptor_base.json");
    const rt = new core.WasmRuntime(base);
    const rtAdd = new core.WasmRuntime(fixture("descriptor_additive.json"));
    const rtBrk = new core.WasmRuntime(fixture("descriptor_breaking.json"));
    const fragBase = rt.contract_fragment("touch");
    const fragAdd = rtAdd.contract_fragment("touch");
    const fragBrk = rtBrk.contract_fragment("touch");
    const hashBase = JSON.parse(fragBase).hash as string;
    const hashAdd = JSON.parse(fragAdd).hash as string;
    const hashBrk = JSON.parse(fragBrk).hash as string;
    const VALID = enc.encode('{"x":0.5,"phase":"move"}');
    const BAD = enc.encode('{"x":"bad","phase":"move"}');

    const wasm: Record<string, string> = {
      plan: rt.node_plan("display"),
      fragment: fragBase,
      prepare_ok: rt.prepare_publish("sensor", "touch", '{"x":0.5,"phase":"move"}', 0n),
      prepare_ng: rt.prepare_publish("sensor", "touch", '{"x":"bad","phase":"move"}', 0n),
      prepare_role: rt.prepare_publish("display", "touch", "{}", 0n),
      accept_ok: rt.accept_sample("display", "touch", VALID, hashBase, 0n, undefined),
      accept_ng: rt.accept_sample("display", "touch", BAD, hashBase, 0n, undefined),
      accept_nohash: rt.accept_sample("display", "touch", VALID, undefined, 0n, undefined),
      accept_mismatch: rt.accept_sample("display", "touch", VALID, "deadbeef00000000", 0n, undefined),
      hs_accept: rt.handshake("touch", hashAdd, fragAdd),
      hs_block: rt.handshake("touch", hashBrk, fragBrk),
      hs_undecodable: rt.handshake("touch", "deadbeef00000000", "{not json"),
      hs_wrong_hash: rt.handshake("touch", "0000000000000000", fragBase),
      hs_unknown_conn: rt.handshake("ghost", "deadbeef00000000", "{}"),
      reply_err_ok: core.wasm_parse_reply_err(enc.encode('{"diags":[{"code":"handler_error","path":"$","message":"boom"}]}')),
      reply_err_bad: core.wasm_parse_reply_err(enc.encode("garbage")),
      classify_fatal: core.wasm_classify_delivery(false, '[{"code":"type_mismatch","path":"$.x","message":"m"}]'),
      classify_retry: core.wasm_classify_delivery(true, ""),
    };

    expect(new Set(py.keys())).toEqual(new Set(Object.keys(wasm)));
    for (const [name, expected] of py) {
      expect(wasm[name], `case '${name}' is not byte-identical`).toBe(expected);
    }
  });
});

describe("cross-language: real wire over Py(native) → link → Node(WS)", () => {
  let link: LinkHandle;
  let pyProc: ChildProcess | undefined;

  beforeAll(async () => {
    link = await spawnLink();
  });
  afterAll(async () => {
    killTree(pyProc);
    await link?.stop();
  });

  function spawnPy(desc: string): ChildProcess {
    // gotcha (demonstrated in this task; see link.rs): the link's peer_listen binds to `tcp/[::]:{port}`,
    // and on Windows the IPv4-mapped fallback does not work, so 127.0.0.1 gives ECONNREFUSED
    // (confirmed with a raw Socket / Test-NetConnection: IPv4 loopback is refused, IPv6 loopback is accepted).
    // Simply using the IPv6 loopback in the native peer's connect string resolves it (the link and the assertions are unchanged).
    return spawn(
      "uv",
      ["run", "python", script("py_pub.py"), "--desc", desc, "--connect", `tcp/[::1]:${link.peerPort}`],
      { cwd: pyDir, stdio: "ignore", env: { ...process.env, PYTHONUTF8: "1" } },
    );
  }

  it("happy path: Python publish → Node subscribe is delivered (zero config; link relay)", async () => {
    const b: SahouNode = await connect(fixture("descriptor_base.json"), { node: "display", port: link.port, spawnLink: false });
    try {
      const got: unknown[] = [];
      await b.subscribe("touch", (p) => got.push(p));
      pyProc = spawnPy("base");
      expect(await waitFor(() => got.length > 0, 30_000, 100)).toBe(true);
      expect(got[0]).toEqual({ x: 0.5, phase: "move" });
    } finally {
      killTree(pyProc);
      pyProc = undefined;
      await b.close();
    }
  });

  it("breaking: the Node receiver blocks the Python sender at handshake; diagnostics are byte-identical to the core", async () => {
    const b: SahouNode = await connect(fixture("descriptor_base.json"), { node: "display", port: link.port, spawnLink: false });
    try {
      const got: unknown[] = [];
      const rejects: Diag[][] = [];
      await b.subscribe("touch", (p) => got.push(p), { onReject: (_c, d) => rejects.push(d) });
      pyProc = spawnPy("breaking");
      const blocked = () => rejects.some((d) => d[0]?.code === "schema_incompatible");
      expect(await waitFor(blocked, 30_000, 100)).toBe(true);
      expect(got).toEqual([]);
      // Expected = the core's verdict (direct wasm call). If the block—using the real fragment fetched from
      // the Python-side contract queryable—is byte-identical to this, then "same Rust core = identical diagnostics
      // across all three languages" holds in the real wiring.
      const rtB = new core.WasmRuntime(fixture("descriptor_base.json"));
      const rtBrk = new core.WasmRuntime(fixture("descriptor_breaking.json"));
      const frag = rtBrk.contract_fragment("touch");
      const expected = JSON.parse(rtB.handshake("touch", JSON.parse(frag).hash, frag));
      const gotDiags = rejects.find((d) => d[0]?.code === "schema_incompatible");
      expect(gotDiags).toEqual(expected.diags);
    } finally {
      killTree(pyProc);
      pyProc = undefined;
      await b.close();
    }
  });
});
