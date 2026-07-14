// SahouNode — zenoh-ts glue. All boundary semantics are delegated to the core (wasm) (design §1, option B).
// Symmetric with the Python version, runtimes/py/python/sahou/_engine.py.
import {
  CongestionControl,
  Duration,
  Priority,
  Query,
  Reliability,
  ReplyError,
  Sample,
  Session,
} from "@eclipse-zenoh/zenoh-ts";
import type { CoreModule, CoreRuntime } from "./core.js";
import { Diag, SahouRejected, SahouUnreachable, fmtDiags } from "./diag.js";

export type Json = Record<string, unknown>;
export type RejectHandler = (conn: string, diags: Diag[]) => void | Promise<void>;

export interface NodePlan {
  publishes: string[];
  subscribes: string[];
  queries: string[];
  answers: string[];
}

interface QosSpec {
  reliability: string;
  congestion: string;
  priority: string;
  express: boolean;
}

const PRIORITY: Record<string, Priority> = {
  realtime: Priority.REAL_TIME,
  interactive_high: Priority.INTERACTIVE_HIGH,
  interactive_low: Priority.INTERACTIVE_LOW,
  data_high: Priority.DATA_HIGH,
  data: Priority.DATA,
  data_low: Priority.DATA_LOW,
  background: Priority.BACKGROUND,
};

const utf8Strict = new TextDecoder("utf-8", { fatal: true });

/** Decode the wire attachment to a string. Non-UTF-8 (a non-sahou sender) yields undefined → the core returns missing_schema_hash. */
function decodeAttachment(att: { toBytes(): Uint8Array } | null | undefined): string | undefined {
  if (att == null) return undefined;
  try {
    return utf8Strict.decode(att.toBytes());
  } catch {
    return undefined;
  }
}

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

/** Convert a core throw (message = diags JSON) into a SahouRejected. Anything else is re-thrown as-is. */
export function toRejected(e: unknown): never {
  if (e instanceof Error) {
    try {
      throw new SahouRejected(JSON.parse(e.message) as Diag[]);
    } catch (parsed) {
      if (parsed instanceof SahouRejected) throw parsed;
    }
  }
  throw e;
}

export class SahouNode {
  readonly node: string;
  /** Diagnostic code → cumulative reject count (counted NO; never silently dropped). */
  readonly rejectCounts = new Map<string, number>();

  private readonly core: CoreModule;
  private readonly rt: CoreRuntime;
  private readonly session: Session;
  private readonly plan: NodePlan;
  private readonly ns: string;
  private readonly connInfo = new Map<string, { key: string; hash: string }>();
  private readonly publishers = new Map<string, { put(payload: string, opts: { attachment: string }): Promise<void> }>();
  private readonly txSeq = new Map<string, number>();
  private readonly rxSeq = new Map<string, number>();
  private readonly handles: { undeclare(): Promise<void> }[] = [];
  /** `${conn} ${hash}` -> the core's handshake verdict (a blocked verdict replays the core diags; Important-2). */
  private readonly verdicts = new Map<string, { verdict: "accepted" | "blocked"; diags: Diag[] }>();
  private readonly pending = new Set<string>();
  private onRejectGlobal: RejectHandler | undefined;

  static async create(core: CoreModule, session: Session, descriptorJson: string, node: string): Promise<SahouNode> {
    let rt: CoreRuntime;
    let plan: NodePlan;
    try {
      rt = new core.WasmRuntime(descriptorJson);
      plan = JSON.parse(rt.node_plan(node)) as NodePlan;
    } catch (e) {
      toRejected(e); // turn the core diagnostics (JSON) into an exception
    }
    const n = new SahouNode(core, rt, session, node, plan);
    await n.declareContracts();
    return n;
  }

  private constructor(core: CoreModule, rt: CoreRuntime, session: Session, node: string, plan: NodePlan) {
    this.core = core;
    this.rt = rt;
    this.session = session;
    this.node = node;
    this.plan = plan;
    this.ns = rt.namespace();
  }

  /** Contract queryable for every connection this node participates in (design §5-1: content-addressed; every participant declares it). */
  private async declareContracts(): Promise<void> {
    const conns = new Set([
      ...this.plan.publishes,
      ...this.plan.subscribes,
      ...this.plan.queries,
      ...this.plan.answers,
    ]);
    for (const conn of conns) {
      const fragJson = this.rt.contract_fragment(conn);
      const frag = JSON.parse(fragJson) as { key: string; hash: string };
      this.connInfo.set(conn, { key: frag.key, hash: frag.hash });
      const contractKey = `${this.ns}/@sahou/contract/${conn}/${frag.hash}`;
      const q = await this.session.declareQueryable(contractKey, {
        handler: async (query: Query) => {
          try {
            await query.reply(contractKey, fragJson);
          } catch (e) {
            console.warn(`[sahou] contract queryable reply failed on ${contractKey}`, e);
          } finally {
            await query.finalize();
          }
        },
      });
      this.handles.push(q);
    }
  }

  // ---- Public API -----------------------------------------------------

  connectionInfo(conn: string): { key: string; hash: string } {
    const info = this.connInfo.get(conn);
    if (!info) {
      throw new SahouRejected([
        { code: "role_mismatch", path: `connections.${conn}`, message: `node '${this.node}' does not participate in this connection` },
      ]);
    }
    return { ...info };
  }

  onReject(cb: RejectHandler): void {
    this.onRejectGlobal = cb;
  }

  async publish(conn: string, payload: Json): Promise<void> {
    const seq = this.nextSeq(this.txSeq, conn);
    const res = JSON.parse(this.rt.prepare_publish(this.node, conn, JSON.stringify(payload), BigInt(seq)));
    if (!res.ok) throw new SahouRejected(res.diags); // send-boundary NO → do not put
    const pub = await this.publisher(conn, res.msg.qos as QosSpec);
    await pub.put(res.msg.wire, { attachment: res.msg.attachment });
  }

  async subscribe(
    conn: string,
    handler: (payload: Json) => void | Promise<void>,
    opts: { onReject?: RejectHandler } = {},
  ): Promise<void> {
    if (!this.plan.subscribes.includes(conn)) {
      throw new SahouRejected([
        { code: "role_mismatch", path: `connections.${conn}`, message: `node '${this.node}' is not a receiver on this connection` },
      ]);
    }
    const key = this.connectionInfo(conn).key;
    const sub = await this.session.declareSubscriber(key, {
      handler: (sample: Sample) => this.handleSample(conn, handler, opts.onReject, sample),
    });
    this.handles.push(sub);
  }

  /** The requester side of query boundaries ①–④. Returns { delivered, response, diags, timedOut }. */
  async query(
    conn: string,
    payload: Json,
    opts: { timeoutMs?: number } = {},
  ): Promise<{ delivered: boolean; response: Json | null; diags: Diag[]; timedOut: boolean }> {
    const seq = this.nextSeq(this.txSeq, conn);
    const res = JSON.parse(this.rt.prepare_request(this.node, conn, JSON.stringify(payload), BigInt(seq)));
    if (!res.ok) throw new SahouRejected(res.diags); // ① send boundary = do not even issue the get
    let delivered = false;
    let response: Json | null = null;
    const diagsAll: Diag[] = [];
    let gotAny = false;
    try {
      const receiver = await this.session.get(res.msg.key, {
        payload: res.msg.wire,
        attachment: res.msg.attachment,
        timeout: Duration.milliseconds.of(opts.timeoutMs ?? 2000),
      });
      if (receiver) {
        for await (const reply of receiver) {
          gotAny = true;
          const r = reply.result();
          if (r instanceof ReplyError) {
            // envelope interpretation lives in a single core implementation (bad_reply_envelope = retryable)
            diagsAll.push(...(JSON.parse(this.core.wasm_parse_reply_err(r.payload().toBytes())) as Diag[]));
            continue;
          }
          const rseq = this.nextSeq(this.rxSeq, conn);
          const wire = r.payload().toBytes();
          const att = decodeAttachment(r.attachment?.());
          let out = JSON.parse(this.rt.accept_reply(this.node, conn, wire, att, BigInt(rseq), undefined));
          if (out.result === "hash_mismatch") out = this.resolveMismatch(conn, out.sender_hash, wire, rseq, "reply");
          if (out.result === "accept") {
            // ④ reply receive boundary
            delivered = true;
            response = JSON.parse(out.payload) as Json;
            break;
          }
          diagsAll.push(...(out.diags as Diag[]));
        }
      }
    } catch (e) {
      console.error(`[sahou] query get failed on '${conn}'`, e); // a failure of the get itself is treated like a timeout (retryable)
    }
    return { delivered, response, diags: diagsAll, timedOut: !gotAny };
  }

  /** Confirmed delivery (Z20): return on a 200-equivalent / fatal (4xx) throws SahouRejected immediately / retryable is resent. */
  async queryConfirmed(
    conn: string,
    payload: Json,
    opts: { timeoutMs?: number; retries?: number; backoffMs?: number } = {},
  ): Promise<Json> {
    const retries = opts.retries ?? 3;
    const backoff = opts.backoffMs ?? 300;
    for (let attempt = 1; ; attempt++) {
      const r = await this.query(conn, payload, { timeoutMs: opts.timeoutMs });
      if (r.delivered) return r.response as Json;
      const cls = this.core.wasm_classify_delivery(r.timedOut, JSON.stringify(r.diags));
      if (cls === "fatal") throw new SahouRejected(r.diags);
      if (attempt > retries) throw new SahouUnreachable(conn, attempt);
      await sleep(backoff * attempt);
    }
  }

  /** The responder side of query (② receive boundary + ③ reply send boundary). */
  async answer(conn: string, fn: (req: Json) => Json | Promise<Json>): Promise<void> {
    if (!this.plan.answers.includes(conn)) {
      throw new SahouRejected([
        { code: "role_mismatch", path: `connections.${conn}`, message: `node '${this.node}' is not the responder on this connection` },
      ]);
    }
    const key = this.connectionInfo(conn).key;
    const q = await this.session.declareQueryable(key, {
      handler: async (query: Query) => {
        try {
          const rseq = this.nextSeq(this.rxSeq, conn);
          const p = query.payload();
          const wire = p ? p.toBytes() : new Uint8Array();
          const att = decodeAttachment(query.attachment?.());
          let out = JSON.parse(this.rt.accept_request(this.node, conn, wire, att, BigInt(rseq), undefined));
          if (out.result === "hash_mismatch") out = this.resolveMismatch(conn, out.sender_hash, wire, rseq, "request");
          if (out.result !== "accept") {
            // ② request receive boundary
            await query.replyErr(JSON.stringify({ diags: out.diags }));
            return;
          }
          let resp: Json;
          try {
            resp = await fn(JSON.parse(out.payload) as Json);
          } catch (e) {
            // a handler exception is returned as a 5xx-equivalent (retryable)
            await query.replyErr(JSON.stringify({ diags: [{ code: "handler_error", path: "$", message: String(e) }] }));
            return;
          }
          const tseq = this.nextSeq(this.txSeq, conn);
          const res = JSON.parse(this.rt.prepare_reply(this.node, conn, JSON.stringify(resp), BigInt(tseq)));
          if (!res.ok) {
            // ③ reply send boundary = do not reply with a broken response
            await query.replyErr(JSON.stringify({ diags: res.diags }));
            return;
          }
          await query.reply(res.msg.key, res.msg.wire, { attachment: res.msg.attachment });
        } catch (e) {
          console.error(`[sahou] internal error in queryable '${conn}'`, e); // last line of defense
        } finally {
          await query.finalize(); // close the querier's channel (013-3: required for Node)
        }
      },
    });
    this.handles.push(q);
  }

  async close(): Promise<void> {
    for (const h of this.handles) {
      try {
        await h.undeclare();
      } catch (e) {
        console.warn("[sahou] undeclare failed", e); // close is best-effort (log, don't swallow)
      }
    }
    try {
      await this.session.close();
    } catch (e) {
      console.warn("[sahou] session close failed", e);
    }
    // close is terminal (rt is not used afterwards). Free the core instance held for the whole lifetime here too.
    try {
      this.rt.free();
    } catch (e) {
      console.warn("[sahou] rt.free failed", e); // best-effort (log, don't swallow)
    }
  }

  // ---- Internals ------------------------------------------------------

  private nextSeq(counter: Map<string, number>, conn: string): number {
    const seq = counter.get(conn) ?? 0;
    counter.set(conn, seq + 1);
    return seq;
  }

  private async publisher(conn: string, qos: QosSpec) {
    let pub = this.publishers.get(conn);
    if (!pub) {
      const key = this.connectionInfo(conn).key;
      const prio = PRIORITY[qos.priority];
      if (prio === undefined) {
        // the descriptor enum is guaranteed by the core, so reaching here means a missing mapping in the glue (do not silently fall back)
        throw new SahouRejected([
          { code: "descriptor_error", path: "$", message: `unknown priority '${qos.priority}' (missing QoS mapping in the glue)` },
        ]);
      }
      pub = await this.session.declarePublisher(key, {
        reliability: qos.reliability === "reliable" ? Reliability.RELIABLE : Reliability.BEST_EFFORT,
        congestionControl: qos.congestion === "block" ? CongestionControl.BLOCK : CongestionControl.DROP,
        priority: prio,
        express: qos.express,
      });
      this.publishers.set(conn, pub);
    }
    return pub;
  }

  private countReject(conn: string, diags: Diag[], onReject?: RejectHandler): void {
    for (const d of diags) this.rejectCounts.set(d.code, (this.rejectCounts.get(d.code) ?? 0) + 1);
    const cb = onReject ?? this.onRejectGlobal;
    if (cb) {
      try {
        // sync throws are caught here; an async callback's rejection is caught by .catch() (both log only; the receive path continues)
        Promise.resolve(cb(conn, diags)).catch((e) => {
          console.error("[sahou] onReject callback failed", e);
        });
      } catch (e) {
        console.error("[sahou] onReject callback failed", e); // a failing user callback must not kill the receive path
      }
    } else {
      console.warn(`[sahou] reject on '${conn}': ${fmtDiags(diags)}`);
    }
  }

  private handleSample(
    conn: string,
    handler: (payload: Json) => void | Promise<void>,
    onReject: RejectHandler | undefined,
    sample: { payload(): { toBytes(): Uint8Array }; attachment?(): { toBytes(): Uint8Array } | undefined },
  ): void {
    try {
      const seq = this.nextSeq(this.rxSeq, conn);
      const wire = sample.payload().toBytes();
      const att = decodeAttachment(sample.attachment?.());
      let out = JSON.parse(this.rt.accept_sample(this.node, conn, wire, att, BigInt(seq), undefined));
      if (out.result === "hash_mismatch") out = this.resolveMismatch(conn, out.sender_hash, wire, seq, "sample");
      if (out.result === "accept") {
        try {
          // sync throws are caught here; an async handler's rejection is caught by .catch() (both log only; the receive path continues)
          Promise.resolve(handler(JSON.parse(out.payload) as Json)).catch((e) => {
            console.error(`[sahou] handler failed on '${conn}'`, e);
          });
        } catch (e) {
          console.error(`[sahou] handler failed on '${conn}'`, e); // a handler exception must not kill the receive path
        }
      } else {
        this.countReject(conn, out.diags as Diag[], onReject);
      }
    } catch (e) {
      console.error(`[sahou] internal error while handling sample on '${conn}'`, e); // last line of defense (don't die silently)
    }
  }

  private resolveMismatch(
    conn: string,
    senderHash: string,
    wire: Uint8Array,
    seq: number,
    kind: "sample" | "request" | "reply",
  ): { result: string; diags?: Diag[]; payload?: string } {
    const k = `${conn} ${senderHash}`;
    const entry = this.verdicts.get(k);
    if (entry) {
      if (entry.verdict === "accepted") {
        const accept = {
          sample: this.rt.accept_sample,
          request: this.rt.accept_request,
          reply: this.rt.accept_reply,
        }[kind].bind(this.rt);
        return JSON.parse(accept(this.node, conn, wire, senderHash, BigInt(seq), senderHash));
      }
      // blocked: replay the core's handshake diags verbatim (no glue-authored wording; byte-identical across all three languages)
      return { result: "reject", diags: entry.diags };
    }
    void this.startHandshake(conn, senderHash).catch((e) => {
      console.error(`[sahou] handshake failed on '${conn}'`, e);
    });
    return {
      result: "reject",
      diags: [
        {
          code: "handshake_pending",
          path: `connections.${conn}`,
          message: `contract version mismatch detected; handshake in progress (sender_hash=${senderHash})`,
        },
      ],
    };
  }

  /** Design §5-2 to 5-4: fetch the contract once and cache the core's three-valued verdict. `unreachable` is not cached. */
  private async startHandshake(conn: string, senderHash: string): Promise<void> {
    const k = `${conn} ${senderHash}`;
    if (this.pending.has(k) || this.verdicts.has(k)) return;
    this.pending.add(k);
    try {
      const sel = `${this.ns}/@sahou/contract/${conn}/${senderHash}`;
      let fragment: string | null = null;
      try {
        const receiver = await this.session.get(sel, { timeout: Duration.milliseconds.of(2000) });
        if (receiver) {
          for await (const reply of receiver) {
            const r = reply.result();
            if (!(r instanceof ReplyError)) {
              fragment = r.payload().toString();
              break;
            }
          }
        }
      } catch (e) {
        console.warn(`[sahou] contract fetch failed: ${sel}`, e); // a failure of the get itself is treated as unreachable
      }
      if (fragment == null) {
        console.warn(`[sahou] [contract_unreachable] ${sel}: cannot fetch the contract (not cached; will retry on the next mismatch detection)`);
        return;
      }
      const res = JSON.parse(this.rt.handshake(conn, senderHash, fragment));
      if (res.verdict === "unreachable") {
        console.warn(`[sahou] [contract_unreachable] '${conn}' (sender=${senderHash}): ${fmtDiags(res.diags)} (not cached)`);
        return;
      }
      this.verdicts.set(k, { verdict: res.verdict, diags: (res.diags ?? []) as Diag[] });
      if (res.verdict === "accepted") {
        console.info(`[sahou] handshake accepted on '${conn}' (sender=${senderHash}, additive)`);
      } else {
        console.warn(`[sahou] [schema_incompatible] '${conn}' (sender=${senderHash}): ${fmtDiags(res.diags)}`);
      }
    } finally {
      this.pending.delete(k);
    }
  }
}
