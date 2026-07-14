// Pure row models for the node-centric spreadsheet view. One sheet = one node; the sheet lists the
// connections that node sends (from === node) and receives (node in to). No editing / no validation here
// (editing goes through edits/contract-edits; the core does every NO §4).
import type { Connection, Contract, Slot } from "../core-bridge";

export type SlotName = "payload" | "request" | "response";

/** The slot names a connection carries, in a stable order (pub_sub → payload / query → request, response). */
export function slotNamesOf(conn: Connection): SlotName[] {
  return conn.pattern === "query" ? ["request", "response"] : ["payload"];
}

/** Connection ids this node sends on (from === node), in connection order. */
export function sends(c: Contract, node: string): string[] {
  return Object.keys(c.connections).filter((id) => c.connections[id].from === node);
}

/** Connection ids this node receives on (node is one of `to`), in connection order. */
export function receives(c: Contract, node: string): string[] {
  return Object.keys(c.connections).filter((id) => c.connections[id].to.includes(node));
}

/** A one-line summary of a slot's type for the row's collapsed view (the full type is edited on expand). */
export function typeSummary(slot: Slot | undefined): string {
  if (!slot) return "—";
  if (slot.typing === "any") return "any";
  if ((slot.kind ?? "record") === "opaque") {
    return slot.encoding ? `opaque (${slot.encoding})` : "opaque";
  }
  const fs = slot.fields ?? [];
  if (fs.length === 0) return "record {}";
  const head = fs.slice(0, 3).map((f) => `${f.name}: ${f.type}`).join(", ");
  return fs.length > 3 ? `${head}, +${fs.length - 3}` : head;
}
