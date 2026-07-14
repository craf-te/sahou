// The single entry point to the core wasm (design §2.2). Decoding the {ok, ...} envelope and the TS
// type definitions for the contract / diagnostics are consolidated here. The GUI never reimplements
// any validation logic — every NO comes from the core through this file (§4).
import init, {
  wasm_descriptor,
  wasm_parse,
  wasm_parse_endpoints,
  wasm_sample,
  wasm_serialize,
  wasm_serialize_endpoints,
  wasm_validate_payload,
  wasm_validate_schema,
} from "./core-wasm/sahou_core.js";

/** Positioned structured diagnostic (same shape as the core's {code, path, message} — constraint ②). */
export interface SahouDiag {
  code: string;
  path: string;
  message: string;
}

// --- Contract serde shape (a JSON mirror of core/src/contract.rs; keys at their default are omitted) ---
export type TypeNameStr =
  | "int" | "float" | "bool" | "string" | "bytes" | "timestamp"
  | "enum" | "array" | "map" | "group" | "union";
export type TypeSpec = TypeNameStr | DetailedType;
export interface DetailedType {
  type: TypeNameStr;
  items?: TypeSpec;
  values?: string[];
  any_of?: TypeSpec[];
  fields?: Field[];
  min?: number;
  max?: number;
  max_len?: number;
}
export interface Field {
  name: string;
  type: TypeNameStr;
  required?: boolean; // omitted = true (serde skip)
  default?: unknown;
  min?: number;
  max?: number;
  max_len?: number;
  values?: string[];
  items?: TypeSpec;
  any_of?: TypeSpec[];
  fields?: Field[];
}
export interface Slot {
  typing: "any" | "typed";
  kind?: "record" | "opaque"; // omitted = record
  fields?: Field[];
  encoding?: string;
}
export type PriorityStr =
  | "realtime" | "interactive_high" | "interactive_low"
  | "data_high" | "data" | "data_low" | "background";
export interface Connection {
  pattern: "pub_sub" | "query";
  from: string;
  to: string[];
  key?: string;
  selector?: string; // query only (Task 5 / design §5.2)
  reliability?: "best_effort" | "reliable"; // omitted = best_effort
  congestion?: "drop" | "block"; // omitted = drop
  priority?: PriorityStr; // omitted = data
  express?: boolean;
  encoding?: "json";
  validate?: "full" | "sampled" | "off";
  payload?: Slot;
  request?: Slot;
  response?: Slot;
}
export interface ContractNode {
  kind?: "sahou" | "external"; // omitted = sahou
}
export interface Contract {
  schema: string;
  version: string;
  nodes: Record<string, ContractNode>;
  connections: Record<string, Connection>;
}
export interface NodeEndpoint {
  mode?: "auto" | "peer" | "client";
  connect?: string[];
}
export interface Endpoints {
  env?: string;
  namespace: string;
  router?: { enabled?: boolean; endpoint?: string };
  nodes: Record<string, NodeEndpoint>;
  plugins?: string[];
}
export interface DescriptorConnection extends Connection {
  key: string; // resolved keyexpr
  hash: string; // per-connection hash (16 hex)
}
export interface Descriptor {
  schema: string;
  version: string;
  namespace: string;
  nodes: Record<string, ContractNode>;
  connections: Record<string, DescriptorConnection>;
}

/** The core's NO (with diagnostics). The GUI never authors its own error text — the display always
 *  shows the diags verbatim (§6). */
export class CoreNo extends Error {
  constructor(public diags: SahouDiag[]) {
    super(diags.map((d) => `[${d.code}] @${d.path}: ${d.message}`).join("; "));
    this.name = "CoreNo";
  }
}

let ready = false;

/** Initialize wasm. The browser uses the default (fetch); tests (node) pass a byte array. */
export async function initCore(wasmInput?: BufferSource): Promise<void> {
  if (ready) return;
  await init(wasmInput === undefined ? undefined : { module_or_path: wasmInput });
  ready = true;
}

type Envelope = { ok: boolean; diags?: SahouDiag[] } & Record<string, unknown>;

function decode<T>(raw: string, key: string): T {
  const env = JSON.parse(raw) as Envelope;
  if (!env.ok) throw new CoreNo(env.diags ?? []);
  return env[key] as T;
}

export function parse(yaml: string): Contract {
  return decode<Contract>(wasm_parse(yaml), "contract");
}

export function serialize(contract: Contract): string {
  return decode<string>(wasm_serialize(JSON.stringify(contract)), "yaml");
}

/** Enumerate diagnostics (does not throw on a NO — for the diagnostics pane / status bar). */
export function validateSchema(yaml: string): SahouDiag[] {
  return (JSON.parse(wasm_validate_schema(yaml)) as Envelope).diags ?? [];
}

export function validatePayload(slot: Slot, payload: unknown): SahouDiag[] {
  const raw = wasm_validate_payload(JSON.stringify(slot), JSON.stringify(payload));
  return (JSON.parse(raw) as Envelope).diags ?? [];
}

export function sample(slot: Slot): unknown {
  return decode<unknown>(wasm_sample(JSON.stringify(slot)), "sample");
}

export function descriptor(yaml: string, endpointsYaml: string): Descriptor {
  return decode<Descriptor>(wasm_descriptor(yaml, endpointsYaml), "descriptor");
}

export function parseEndpoints(yaml: string): Endpoints {
  return decode<Endpoints>(wasm_parse_endpoints(yaml), "endpoints");
}

export function serializeEndpoints(endpoints: Endpoints): string {
  return decode<string>(wasm_serialize_endpoints(JSON.stringify(endpoints)), "yaml");
}
