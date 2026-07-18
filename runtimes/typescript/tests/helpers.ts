import { ChildProcess, spawn } from "node:child_process";
import net from "node:net";
import { fileURLToPath } from "node:url";
import { Config, Duration, ReplyError, Sample, Session } from "@eclipse-zenoh/zenoh-ts";
import type { Json, SahouNode } from "../src/engine.js";

const exe = process.platform === "win32" ? "sahou.exe" : "sahou";
/** The real binary under test (cargo build -p sahou must have been run). */
export const SAHOU_BIN =
  process.env.SAHOU_BIN ?? fileURLToPath(new URL(`../../../target/debug/${exe}`, import.meta.url));

export const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

export function portOpen(port: number, host = "127.0.0.1"): Promise<boolean> {
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

export function freePort(): Promise<number> {
  return new Promise((res, rej) => {
    const srv = net.createServer();
    srv.listen(0, "127.0.0.1", () => {
      const port = (srv.address() as net.AddressInfo).port;
      srv.close(() => res(port));
    });
    srv.on("error", rej);
  });
}

/** Wait until the predicate becomes true (absorbs discovery/delivery jitter; symmetric with pytest's wait_until). */
export async function waitFor(pred: () => boolean | Promise<boolean>, timeoutMs = 8000, intervalMs = 50): Promise<boolean> {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (await pred()) return true;
    await sleep(intervalMs);
  }
  return pred();
}

export interface LinkHandle {
  port: number;
  peerPort: number;
  child: ChildProcess;
  stop(): Promise<void>;
}

/** Spawn a real link and wait until its WS is open. Defaults: multicast disabled + a longer grace (for test determinism). */
export async function spawnLink(extra: string[] = []): Promise<LinkHandle> {
  const port = await freePort();
  const peerPort = await freePort();
  // Defaults (60/60, prioritizing test determinism). clap does not allow a duplicate long flag,
  // so if `extra` contains a flag of the same name, drop the default and let `extra`'s value win (e.g. spawnLink(["--grace","4"])).
  const defaults = new Map([
    ["--startup", "60"],
    ["--grace", "60"],
  ]);
  for (let i = 0; i < extra.length; i += 2) defaults.delete(extra[i]);
  const child = spawn(
    SAHOU_BIN,
    [
      "link",
      "--port",
      String(port),
      "--peer-listen",
      String(peerPort),
      "--no-multicast",
      ...[...defaults.entries()].flat(),
      ...extra,
    ],
    { stdio: "ignore" },
  );
  const up = await waitFor(() => portOpen(port), 30_000);
  if (!up) {
    child.kill();
    throw new Error(`link does not start (SAHOU_BIN=${SAHOU_BIN}; check that cargo build -p sahou has been run)`);
  }
  return {
    port,
    peerPort,
    child,
    async stop() {
      child.kill();
      await waitFor(async () => !(await portOpen(port)), 10_000);
    },
  };
}

/** Helper that keeps sending until delivery (symmetric with pytest's pump). */
export async function pump(node: SahouNode, conn: string, payload: Json, received: () => boolean, n = 200): Promise<boolean> {
  for (let i = 0; i < n; i++) {
    await node.publish(conn, payload);
    if (received()) return true;
    await sleep(50);
  }
  return received();
}

/** A raw zenoh-ts session that bypasses validation (for receive-boundary tests; raw bypass). */
export function rawSession(port: number): Promise<Session> {
  return Session.open(new Config(`ws://127.0.0.1:${port}`));
}

/** Fetch a single reply payload from a queryable (null if none). Shared by the vitals suites (node + browser e2e). */
export async function fetchOne(session: Session, key: string): Promise<string | null> {
  const rx = await session.get(key, { timeout: Duration.milliseconds.of(1000) });
  if (!rx) return null;
  for await (const reply of rx) {
    const r = reply.result();
    if (!(r instanceof ReplyError)) return (r as Sample).payload().toString();
  }
  return null;
}

/** Whether a liveliness token exists at `key`, from this observer's vantage. Shared by the vitals suites. */
export async function tokenVisible(session: Session, key: string): Promise<boolean> {
  const rx = await session.liveliness().get(key, { timeout: Duration.milliseconds.of(1000) });
  if (!rx) return false;
  for await (const reply of rx) {
    if (!(reply.result() instanceof ReplyError)) return true;
  }
  return false;
}

/** Repeat put from a raw session until delivery (absorbs silent drops before the route converges; symmetric with pytest's pump_raw). */
export async function pumpRaw(
  raw: Session,
  key: string,
  payload: string,
  received: () => boolean,
  opts: { attachment?: string | Uint8Array } = {},
  n = 200,
): Promise<boolean> {
  for (let i = 0; i < n; i++) {
    if (opts.attachment === undefined) await raw.put(key, payload);
    else await raw.put(key, payload, { attachment: opts.attachment });
    if (received()) return true;
    await sleep(50);
  }
  return received();
}
