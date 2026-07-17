// browser entry point: wasm core (web target). Cannot spawn → when not connected, return a NO with startup steps.
import type { CoreRuntime } from "./core.js";
import { loadCore } from "./core-browser.js";
import { SahouNode, toRejected, type VitalsSeed } from "./engine.js";
import { SAHOU_VERSION } from "./version.js";
import { openSessionWithTimeout } from "./session.js";

export { SahouNode } from "./engine.js";
export type { Json, NodePlan, RejectHandler } from "./engine.js";
export { SahouError, SahouRejected, SahouUnreachable } from "./diag.js";
export type { Diag } from "./diag.js";

export interface ConnectOptions {
  node: string;
  /** Default ws://127.0.0.1:10000 (a link on the same machine). */
  locator?: string;
  /** Node self-report: liveliness token + vitals queryable at <ns>/@sahou/vitals/<node> (default true).
   *  Any LAN peer can read it — see README "Vitals". */
  vitals?: boolean;
}

/** The browser cannot spawn a link → remediation guidance with startup steps. */
const BROWSER_HINT =
  "the browser cannot start a link. Start `sahou link` on the target machine and check the locator (ws://<host>:<port>)";

export async function connect(descriptor: string | object, opts: ConnectOptions): Promise<SahouNode> {
  const descJson = typeof descriptor === "object" ? JSON.stringify(descriptor) : descriptor;
  const core = await loadCore();
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
  const locator = opts.locator ?? "ws://127.0.0.1:10000";
  // zenoh-ts WS connections retry indefinitely against an unreachable locator (known behavior), and a bare
  // await never resolves (session.ts enforces a deadline). Because the browser cannot auto-spawn a link, the
  // caller must confirm that `sahou link` is running on the target machine and that the locator is correct
  // before calling connect() (do not hammer connect() in a tight loop).
  const session = await openSessionWithTimeout(locator, BROWSER_HINT);
  // zenoh-ts's version is not discoverable in a browser (no fs, package.json unexported) — omitted, not faked.
  const seed: VitalsSeed | undefined =
    opts.vitals === false ? undefined : { sahou: SAHOU_VERSION, transport: "browser" };
  return SahouNode.create(core, session, descJson, opts.node, seed);
}
