// An edge's appearance = a derived display of the contract (not validation §4). loose red = a
// visualization of typing:"any" (the core intentionally skips validating any → the GUI shows
// "unvalidated" in red; the division of labor in design §4).
// Colors come from the --graph-* tokens in style.css (resolved by graph/theme.ts; recovers Task 12).
import type { Connection, Contract, Slot } from "../core-bridge";
import { DARK_FALLBACK, graphTheme, type GraphTheme } from "./theme";

// Backwards-compatible color constants (= fallback values. When CSS is loaded, --graph-* is authoritative)
export const AMBER = DARK_FALLBACK.edgeBestEffort;
export const GRAY = DARK_FALLBACK.edgeReliable;
export const VIOLET = DARK_FALLBACK.edgeQuery;
export const RED = DARK_FALLBACK.edgeLoose;

export type SlotKey = "payload" | "request" | "response";

/** The pattern decides the number and role of shape slots (derived from the spike). */
export const slotsFor = (c: Connection): SlotKey[] =>
  c.pattern === "query" ? ["request", "response"] : ["payload"];

const slotLoose = (s: Slot | undefined): boolean => !s || s.typing === "any";

export const connLoose = (c: Connection): boolean => slotsFor(c).some((k) => slotLoose(c[k]));

/** Capabilities are derived from the wiring, not the node (roles removed; upper spec §3). */
export function nodeCaps(contract: Contract, id: string): string[] {
  const caps = new Set<string>();
  for (const c of Object.values(contract.connections)) {
    if (c.from === id) caps.add(c.pattern === "query" ? "qry" : "pub");
    if (c.to.includes(id)) caps.add(c.pattern === "query" ? "ans" : "sub");
  }
  return [...caps];
}

export function edgeStyle(c: Connection, t: GraphTheme = graphTheme()): Record<string, unknown> {
  const base = {
    increasedLineWidthForHitTesting: 16,
    cursor: "pointer",
    labelFill: t.edgeLabel,
    labelFontSize: 11,
    labelBackground: true,
    labelBackgroundFill: t.edgeLabelBg,
    labelBackgroundRadius: 3,
  };
  if (connLoose(c)) return { ...base, stroke: t.edgeLoose, lineWidth: 2, lineDash: [3, 3], endArrow: true };
  if (c.pattern === "query") {
    return { ...base, stroke: t.edgeQuery, lineWidth: 2, lineDash: [2, 3], endArrow: true, startArrow: true };
  }
  const best = (c.reliability ?? "best_effort") === "best_effort";
  return {
    ...base,
    stroke: best ? t.edgeBestEffort : t.edgeReliable,
    lineWidth: 2,
    lineDash: best ? [6, 4] : [0],
    endArrow: true,
  };
}
