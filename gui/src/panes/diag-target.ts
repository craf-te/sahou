// Mapping a diagnostic path → the matching pane (unified path grammar spec §4:
// connections.<id>… / nodes.<id>…). The GUI defines no path grammar of its own — it only reads the
// first two elements of the core's path.
export function targetOf(path: string): { kind: "edge" | "node"; id: string } | null {
  const conn = /^connections\.([^.[\]]+)/.exec(path);
  if (conn) return { kind: "edge", id: conn[1] };
  const node = /^nodes\.([^.[\]]+)/.exec(path);
  if (node) return { kind: "node", id: node[1] };
  return null;
}
