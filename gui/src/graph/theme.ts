// Resolving the graph chrome colors (recovers the Task 12 carry-over). G6 renders to canvas where
// CSS does not reach, so the --graph-* tokens from style.css (which carry light/dark via light-dark())
// are resolved to concrete colors through a measuring element and handed to JS. "Change a color = one
// place in style.css" then holds for the graph too.
// In environments where CSS is not loaded (unit tests, etc.) DARK_FALLBACK is used (never shown on screen).

export interface GraphTheme {
  nodeFill: string;
  nodeStroke: string;
  nodeLabel: string;
  extFill: string;
  extStroke: string;
  portFill: string;
  portStroke: string;
  topicFill: string;
  topicStroke: string;
  topicLabel: string;
  edgeLabel: string;
  edgeLabelBg: string;
  edgeReliable: string;
  edgeBestEffort: string;
  edgeQuery: string;
  edgeLoose: string;
  halo: string;
  selStroke: string;
}

const TOKENS: Record<keyof GraphTheme, string> = {
  nodeFill: "--graph-node-fill",
  nodeStroke: "--graph-node-stroke",
  nodeLabel: "--graph-node-label",
  extFill: "--graph-ext-fill",
  extStroke: "--graph-ext-stroke",
  portFill: "--graph-port-fill",
  portStroke: "--graph-port-stroke",
  topicFill: "--graph-topic-fill",
  topicStroke: "--graph-topic-stroke",
  topicLabel: "--graph-topic-label",
  edgeLabel: "--graph-edge-label",
  edgeLabelBg: "--graph-edge-label-bg",
  edgeReliable: "--graph-edge-reliable",
  edgeBestEffort: "--graph-edge-best-effort",
  edgeQuery: "--graph-edge-query",
  edgeLoose: "--graph-edge-loose",
  halo: "--graph-halo",
  selStroke: "--graph-sel-stroke",
};

/** A fallback equal to style.css's dark values (for tests / non-CSS environments only). */
export const DARK_FALLBACK: GraphTheme = {
  nodeFill: "#1e2430",
  nodeStroke: "#64748b",
  nodeLabel: "#e6e6e6",
  extFill: "#3a2a16",
  extStroke: "#f59e0b",
  portFill: "#0f1115",
  portStroke: "#8b95a5",
  topicFill: "#2a1e3a",
  topicStroke: "#a78bfa",
  topicLabel: "#d8ccf5",
  edgeLabel: "#cfd6e0",
  edgeLabelBg: "#12151c",
  edgeReliable: "#9aa4b2",
  edgeBestEffort: "#fbbf24",
  edgeQuery: "#a78bfa",
  edgeLoose: "#f87171",
  halo: "#22d3ee",
  selStroke: "#e6faff",
};

let cache: { key: string; value: GraphTheme } | null = null;

/** The current theme identifier (explicit data-theme > OS setting). Re-read the cache when it changes. */
function themeKey(root: HTMLElement): string {
  const explicit = root.getAttribute("data-theme") ?? "";
  const osDark =
    typeof matchMedia === "function" && matchMedia("(prefers-color-scheme: dark)").matches;
  return `${explicit}|${osDark}`;
}

/**
 * Resolve the --graph-* tokens to concrete colors (the computed color after light-dark() resolution).
 * getPropertyValue on a custom property returns light-dark() unresolved, so we attach var() to the
 * color of a measuring element and read the computed value.
 */
export function graphTheme(): GraphTheme {
  if (typeof document === "undefined" || !document.body) return DARK_FALLBACK;
  const root = document.documentElement;
  // token undefined = style.css not loaded (unit tests, etc.) → fallback
  if (!getComputedStyle(root).getPropertyValue("--graph-node-fill").trim()) return DARK_FALLBACK;
  const key = themeKey(root);
  if (cache?.key === key) return cache.value;

  const probe = document.createElement("span");
  probe.style.display = "none";
  document.body.appendChild(probe);
  const out = {} as GraphTheme;
  for (const [prop, token] of Object.entries(TOKENS) as [keyof GraphTheme, string][]) {
    probe.style.color = `var(${token})`;
    const c = getComputedStyle(probe).color;
    out[prop] = c && !c.includes("var(") ? c : DARK_FALLBACK[prop];
  }
  probe.remove();
  cache = { key, value: out };
  return out;
}
