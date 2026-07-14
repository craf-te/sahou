// Immutable updates of the contract JSON (global immutability convention / design §3.2). Every
// function returns a new value and never mutates its arguments. This is structural editing only —
// it performs no validation (NO); that is the core wasm's job (§4).
import type { Connection, Contract, Field } from "../core-bridge";

export function uniqueId(base: string, pool: Record<string, unknown>): string {
  let id = base;
  let i = 1;
  while (id in pool) id = `${base}_${i++}`;
  return id;
}

// ---- node ----

/** Add a node from a value confirmed in the modal (UX redesign: "sprouts on click" is gone).
 *  Empty or duplicate names are a NO at the boundary (null) — the dialog shows the reason beforehand. */
export interface AddNodeInput {
  name: string;
  kind: "sahou" | "external";
}

export function addNode(c: Contract, input: AddNodeInput): { contract: Contract; id: string } | null {
  const id = input.name.trim();
  if (id === "" || id in c.nodes) return null;
  return { contract: { ...c, nodes: { ...c.nodes, [id]: { kind: input.kind } } }, id };
}

export function deleteNode(c: Contract, id: string): Contract {
  const nodes = { ...c.nodes };
  delete nodes[id];
  const connections = Object.fromEntries(
    Object.entries(c.connections).filter(([, conn]) => conn.from !== id && !conn.to.includes(id)),
  );
  return { ...c, nodes, connections };
}

export function renameNode(c: Contract, id: string, next: string): Contract {
  if (next === id || next === "" || next in c.nodes) return c; // collision/empty is a no-op
  const nodes = Object.fromEntries(
    Object.entries(c.nodes).map(([k, v]) => [k === id ? next : k, v]),
  );
  const connections = Object.fromEntries(
    Object.entries(c.connections).map(([cid, conn]) => [
      cid,
      {
        ...conn,
        from: conn.from === id ? next : conn.from,
        to: conn.to.map((t) => (t === id ? next : t)),
      },
    ]),
  );
  return { ...c, nodes, connections };
}

export function setNodeKind(c: Contract, id: string, kind: "sahou" | "external"): Contract {
  return { ...c, nodes: { ...c.nodes, [id]: { ...c.nodes[id], kind } } };
}

// ---- connection ----

/** Add a connection from from/to/pattern confirmed in the modal (UX redesign: no more placeholder
 *  connections). A missing `from` is a NO at the boundary (null). `to` is folded to valid targets
 *  (not `from`, and existing nodes) — so the editing layer also cannot create an invalid wiring
 *  (self-loop / unknown node), matching the direction of §9. */
export interface AddConnectionInput {
  from: string;
  to: string[];
  pattern: "pub_sub" | "query";
}

export function addConnection(
  c: Contract,
  input: AddConnectionInput,
): { contract: Contract; id: string } | null {
  if (!(input.from in c.nodes)) return null;
  const to = input.to.filter((t) => t !== input.from && t in c.nodes);
  const id = uniqueId(`${input.from}_to_${to[0] ?? "x"}`, c.connections);
  // pub_sub defaults to "reliable" (a spike-derived UX: start from a wiring you don't want to drop).
  // query has only request/response slots (no payload = an edit that never creates unexpected_slot).
  const conn: Connection =
    input.pattern === "query"
      ? { pattern: "query", from: input.from, to, request: { typing: "any" }, response: { typing: "any" } }
      : {
          pattern: "pub_sub", from: input.from, to,
          reliability: "reliable", congestion: "block",
          payload: { typing: "any" },
        };
  return { contract: { ...c, connections: { ...c.connections, [id]: conn } }, id };
}

export function deleteConnection(c: Contract, id: string): Contract {
  const connections = { ...c.connections };
  delete connections[id];
  return { ...c, connections };
}

export function renameConnection(c: Contract, id: string, next: string): Contract {
  if (next === id || next === "" || next in c.connections) return c;
  const connections = Object.fromEntries(
    Object.entries(c.connections).map(([k, v]) => [k === id ? next : k, v]),
  );
  return { ...c, connections };
}

/** Partial update. An undefined value in the patch means "delete the key" (serialize restores the default). */
export function updateConnection(c: Contract, id: string, patch: Partial<Connection>): Contract {
  const prev = c.connections[id];
  if (!prev) return c;
  const next: Connection = { ...prev, ...patch };
  for (const [k, v] of Object.entries(patch)) {
    if (v === undefined) delete (next as unknown as Record<string, unknown>)[k];
  }
  return { ...c, connections: { ...c.connections, [id]: next } };
}

/** Switch the pattern. Drops the unrelated slots (request/response and selector for pub_sub, payload
 *  for query) — an edit that never creates unexpected_slot / unexpected_selector. */
export function setPattern(c: Contract, id: string, pattern: "pub_sub" | "query"): Contract {
  const prev = c.connections[id];
  if (!prev || prev.pattern === pattern) return c;
  if (pattern === "query") {
    const { payload: _payload, ...rest } = prev;
    return {
      ...c,
      connections: {
        ...c.connections,
        [id]: {
          ...rest,
          pattern,
          request: prev.request ?? { typing: "any" },
          response: prev.response ?? { typing: "any" },
        },
      },
    };
  }
  const { request: _request, response: _response, selector: _selector, ...rest } = prev;
  return {
    ...c,
    connections: {
      ...c.connections,
      [id]: { ...rest, pattern, payload: prev.payload ?? { typing: "any" } },
    },
  };
}

export function setFrom(c: Contract, id: string, from: string): Contract {
  const prev = c.connections[id];
  if (!prev) return c;
  return updateConnection(c, id, { from, to: prev.to.filter((t) => t !== from) });
}

export function toggleTarget(c: Contract, id: string, node: string): Contract {
  const prev = c.connections[id];
  if (!prev) return c;
  const to = prev.to.includes(node) ? prev.to.filter((t) => t !== node) : [...prev.to, node];
  return updateConnection(c, id, { to });
}

/** The preset side of the 3-way delivery choice (custom lets the pane patch reliability/congestion directly). */
export function setDelivery(c: Contract, id: string, m: "stream" | "reliable"): Contract {
  return m === "stream"
    ? updateConnection(c, id, { reliability: "best_effort", congestion: "drop" })
    : updateConnection(c, id, { reliability: "reliable", congestion: "block" });
}

// ---- fields tree (path = the index at each level. [] = root) ----

export function uniqueName(base: string, fields: Field[]): string {
  const names = new Set(fields.map((f) => f.name));
  let n = base;
  let i = 1;
  while (names.has(n)) n = `${base}${i++}`;
  return n;
}

/** Return a new tree with fn applied to the list at the level the path points to (the shared path for immutable updates). */
function mapAt(fields: Field[], path: number[], fn: (list: Field[]) => Field[]): Field[] {
  if (path.length === 0) return fn(fields);
  const [head, ...rest] = path;
  return fields.map((f, i) => (i === head ? { ...f, fields: mapAt(f.fields ?? [], rest, fn) } : f));
}

export function addFieldAt(fields: Field[], parentPath: number[]): Field[] {
  return mapAt(fields, parentPath, (list) => [
    ...list,
    { name: uniqueName("field", list), type: "float" },
  ]);
}

export function removeFieldAt(fields: Field[], path: number[]): Field[] {
  const at = path[path.length - 1];
  return mapAt(fields, path.slice(0, -1), (list) => list.filter((_, i) => i !== at));
}

/** An undefined value in the patch means "delete the key" (required: undefined = restore the default true / clear default). */
export function updateFieldAt(fields: Field[], path: number[], patch: Partial<Field>): Field[] {
  const at = path[path.length - 1];
  return mapAt(fields, path.slice(0, -1), (list) =>
    list.map((f, i) => {
      if (i !== at) return f;
      const next: Field = { ...f, ...patch };
      for (const [k, v] of Object.entries(patch)) {
        if (v === undefined) delete (next as unknown as Record<string, unknown>)[k];
      }
      return next;
    }),
  );
}

/** Wrap the selected rows into a single group (order preserved, inserted at the first selected position; derived from the spike). */
export function groupFieldsAt(fields: Field[], parentPath: number[], indices: number[]): Field[] {
  return mapAt(fields, parentPath, (list) => {
    const idx = new Set(indices);
    if (idx.size === 0) return list;
    const picked = list.filter((_, i) => idx.has(i));
    const rest: Field[] = [];
    let at = -1;
    list.forEach((f, i) => {
      if (idx.has(i)) {
        if (at < 0) at = rest.length;
      } else {
        rest.push(f);
      }
    });
    const group: Field = { name: uniqueName("group", list), type: "group", fields: picked };
    return [...rest.slice(0, at), group, ...rest.slice(at)];
  });
}

/** Ungroup: expand the group's contents in place. */
export function ungroupFieldAt(fields: Field[], path: number[]): Field[] {
  const at = path[path.length - 1];
  return mapAt(fields, path.slice(0, -1), (list) =>
    list.flatMap((f, i) => (i === at && f.type === "group" ? (f.fields ?? []) : [f])),
  );
}
