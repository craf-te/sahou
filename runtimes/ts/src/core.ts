// Typed entry point into the core (wasm). ABI matches PyO3 SahouRuntime: same names, same JSON
// envelope (string/bytes in, JSON string out; seq is u64 = BigInt).
export interface CoreRuntime {
  namespace(): string;
  node_plan(node: string): string;
  prepare_publish(node: string, conn: string, payloadJson: string, seq: bigint): string;
  accept_sample(
    node: string,
    conn: string,
    wire: Uint8Array,
    attachment: string | null | undefined,
    seq: bigint,
    trusted?: string | null,
  ): string;
  prepare_request(node: string, conn: string, payloadJson: string, seq: bigint): string;
  accept_request(
    node: string,
    conn: string,
    wire: Uint8Array,
    attachment: string | null | undefined,
    seq: bigint,
    trusted?: string | null,
  ): string;
  prepare_reply(node: string, conn: string, payloadJson: string, seq: bigint): string;
  accept_reply(
    node: string,
    conn: string,
    wire: Uint8Array,
    attachment: string | null | undefined,
    seq: bigint,
    trusted?: string | null,
  ): string;
  contract_fragment(conn: string): string;
  handshake(conn: string, senderHash: string, theirsJson: string): string;
  free(): void;
}

export interface CoreModule {
  WasmRuntime: new (descriptorJson: string) => CoreRuntime;
  wasm_classify_delivery(timedOut: boolean, diagsJson: string): string;
  wasm_parse_reply_err(payload: Uint8Array): string;
}
