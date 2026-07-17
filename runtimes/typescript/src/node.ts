// node entry point: wasm core (nodejs target) + automatic link spawn + zenoh-ts (WS → link).
import { spawn } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import net from "node:net";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import type { CoreRuntime } from "./core.js";
import { loadCore } from "./core-node.js";
import { SahouNode, toRejected, type VitalsSeed } from "./engine.js";
import { linkUnavailable, openSessionWithTimeout } from "./session.js";

export { SahouNode } from "./engine.js";
export type { Json, NodePlan, RejectHandler, VitalsSeed } from "./engine.js";
export { SahouError, SahouRejected, SahouUnreachable } from "./diag.js";
export type { Diag } from "./diag.js";

export interface ConnectOptions {
  node: string;
  /** Default ws://127.0.0.1:<port>. Pointing at a remote link disables automatic spawn. */
  locator?: string;
  /** Default 10000. */
  port?: number;
  /** Automatic link spawn (default true; false disables both probing and spawning). */
  spawnLink?: boolean;
  /** Node self-report: liveliness token + vitals queryable at <ns>/@sahou/vitals/<node> (default true).
   *  Any LAN peer can read it — see README "Vitals". */
  vitals?: boolean;
}

const moduleDir = dirname(fileURLToPath(import.meta.url));

/** Version from the nearest package.json at/above `dir` (undefined when unreadable — omitted, not faked). */
function nearestPkgVersion(dir: string): string | undefined {
  for (let d = dir; ; ) {
    const p = join(d, "package.json");
    if (existsSync(p)) {
      try {
        const v = (JSON.parse(readFileSync(p, "utf-8")) as { version?: unknown }).version;
        if (typeof v === "string") return v;
      } catch {
        return undefined;
      }
    }
    const parent = dirname(d);
    if (parent === d) return undefined;
    d = parent;
  }
}

/** zenoh-ts version via a node_modules walk-up from this module. Plain fs on purpose:
 *  the package exports no version and hides its package.json behind `exports`, and
 *  import.meta.resolve is unavailable under vitest. */
function zenohTsVersion(): string | undefined {
  for (let d = moduleDir; ; ) {
    const p = join(d, "node_modules", "@eclipse-zenoh", "zenoh-ts", "package.json");
    if (existsSync(p)) {
      try {
        const v = (JSON.parse(readFileSync(p, "utf-8")) as { version?: unknown }).version;
        return typeof v === "string" ? v : undefined;
      } catch {
        return undefined;
      }
    }
    const parent = dirname(d);
    if (parent === d) return undefined;
    d = parent;
  }
}

function readDescriptor(descriptor: string | object): string {
  if (typeof descriptor === "object") return JSON.stringify(descriptor);
  if (descriptor.endsWith(".json") && existsSync(descriptor)) return readFileSync(descriptor, "utf-8");
  return descriptor; // the JSON string itself
}

function portOpen(port: number, host = "127.0.0.1"): Promise<boolean> {
  return new Promise((res) => {
    const s = net.connect(port, host);
    s.on("connect", () => {
      s.destroy();
      res(true);
    });
    s.on("error", () => res(false));
    s.setTimeout(500, () => {
      s.destroy();
      res(false);
    });
  });
}

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

/** Remediation steps for the node entry point (entry.test.ts checks for the presence of "sahou link" and "SAHOU_LINK_CMD"). */
const nodeHint = (port: number): string =>
  `put the sahou CLI on PATH (cargo install / a distributed binary), point SAHOU_LINK_CMD at an executable, ` +
  `or start \`sahou link --port ${port}\` yourself`;

/**
 * Automatically spawn a local link if none exists (with idle auto-shutdown, so no orphans; design §7).
 * The `portOpen` guard is the first line of defense (it suppresses a reconnect tight loop in node): because
 * we connect only after confirming that a local link is already present or that a spawned one has its port
 * open, reckless retries in `openSessionWithTimeout` (session.ts) are unlikely.
 */
async function ensureLink(locator: string, port: number): Promise<void> {
  const m = /^ws:\/\/(?:localhost|127\.0\.0\.1):(\d+)/.exec(locator);
  if (!m) return; // do not spawn when a remote link is specified
  const wsPort = Number(m[1]);
  if (await portOpen(wsPort)) return; // already present (shared)
  const cmd = process.env.SAHOU_LINK_CMD ?? "sahou";
  const extra = (process.env.SAHOU_LINK_ARGS ?? "").split(/\s+/).filter(Boolean);
  const child = spawn(cmd, ["link", "--port", String(wsPort), ...extra], { detached: true, stdio: "ignore" });
  // Wrap in an object: TS control-flow narrowing cannot track reassignment through a closure and would
  // collapse the variable to `never`. Reading through a property is correctly typed as Error | null.
  const spawnState: { error: Error | null } = { error: null };
  child.on("error", (e) => {
    spawnState.error = e; // ENOENT arrives asynchronously
  });
  child.unref(); // even if this process dies, the link stays alive and self-terminates after going idle
  for (let i = 0; i < 40; i++) {
    await sleep(500);
    if (spawnState.error) {
      throw linkUnavailable(`cannot connect to sahou link (spawn failed: ${cmd} (${spawnState.error.message}))`, nodeHint(port));
    }
    if (await portOpen(wsPort)) return;
  }
  throw linkUnavailable("cannot connect to sahou link (timed out waiting for startup)", nodeHint(port));
}

export async function connect(descriptor: string | object, opts: ConnectOptions): Promise<SahouNode> {
  const descJson = readDescriptor(descriptor);
  const core = loadCore();
  // Inspect the core before transport (say NO as early as possible, at the right place).
  let rt: CoreRuntime;
  try {
    rt = new core.WasmRuntime(descJson); // a constructor throw means the wasm was not generated → no free needed
  } catch (e) {
    toRejected(e);
  }
  try {
    rt.node_plan(opts.node);
  } catch (e) {
    toRejected(e);
  } finally {
    rt.free(); // always free this validation-only instance, even if node_plan throws
  }
  const port = opts.port ?? 10000;
  const locator = opts.locator ?? `ws://127.0.0.1:${port}`;
  if (opts.spawnLink !== false) await ensureLink(locator, port);
  const session = await openSessionWithTimeout(locator, nodeHint(port));
  const seed: VitalsSeed | undefined =
    opts.vitals === false
      ? undefined
      : { sahou: nearestPkgVersion(moduleDir) ?? "unknown", zenoh: zenohTsVersion(), transport: "ws-link" };
  return SahouNode.create(core, session, descJson, opts.node, seed);
}
