// Deriving the G6 render data (pure function). direct = one direct edge per target / bus = pub_sub
// with multiple targets shown as 1+N edges via a topic node (derived from spike notes/009).
// Colors come from the --graph-* tokens in style.css (resolved by graph/theme.ts; recovers Task 12).
import type { Contract } from "../core-bridge";
import type { LayoutFile } from "../layout";
import { edgeStyle, nodeCaps } from "./edge-style";
import { graphTheme } from "./theme";

export type WiringMode = "direct" | "bus";
export const TOPIC_PREFIX = "__topic__";

/** The minimal shape structurally compatible with G6's NodeData/EdgeData (id/source/target required; the index signature makes it assignable to G6 types). */
export interface GraphNode {
  id: string;
  type?: string;
  style?: Record<string, unknown>;
  data?: Record<string, unknown>;
  [key: string]: unknown;
}
export interface GraphEdge {
  id?: string;
  source: string;
  target: string;
  style?: Record<string, unknown>;
  data?: Record<string, unknown>;
  [key: string]: unknown;
}
export interface GraphData {
  nodes: GraphNode[];
  edges: GraphEdge[];
}

export function buildData(contract: Contract, layout: LayoutFile, mode: WiringMode): GraphData {
  const t = graphTheme();
  const nodes: GraphNode[] = Object.entries(contract.nodes).map(([id, n]) => {
    const pos = layout.nodes[id] ?? { x: 100, y: 100 };
    const ext = n.kind === "external";
    const caps = nodeCaps(contract, id);
    return {
      id,
      type: "rect",
      style: {
        x: pos.x, y: pos.y, size: [160, 54], radius: 8,
        fill: ext ? t.extFill : t.nodeFill,
        stroke: ext ? t.extStroke : t.nodeStroke,
        lineWidth: 2,
        labelText: `${id}${caps.length ? "  ⟨" + caps.join("·") + "⟩" : ""}`,
        labelFill: t.nodeLabel, labelFontSize: 12, labelPlacement: "center",
        ports: [{ key: "right", placement: [1, 0.5] }, { key: "left", placement: [0, 0.5] }],
        portR: 4, portFill: t.portFill, portStroke: t.portStroke, portLineWidth: 1.5,
      },
      data: { kind: n.kind ?? "sahou" },
    };
  });
  const edges: GraphEdge[] = [];
  for (const [cid, c] of Object.entries(contract.connections)) {
    const useBus = mode === "bus" && c.pattern === "pub_sub" && c.to.length > 1;
    const tag = c.pattern === "query" ? " ⇄" : "";
    if (useBus) {
      const tid = `${TOPIC_PREFIX}${cid}`;
      const pp = layout.nodes[c.from] ?? { x: 100, y: 100 };
      const cx = c.to.reduce((s, t) => s + (layout.nodes[t]?.x ?? 0), 0) / c.to.length;
      const cy = c.to.reduce((s, t) => s + (layout.nodes[t]?.y ?? 0), 0) / c.to.length;
      nodes.push({
        id: tid,
        type: "rect",
        style: {
          x: Math.round((pp.x + cx) / 2), y: Math.round((pp.y + cy) / 2),
          size: [130, 30], radius: 15,
          fill: t.topicFill, stroke: t.topicStroke, lineWidth: 1.5,
          labelText: cid, labelFill: t.topicLabel, labelFontSize: 10, labelPlacement: "center",
        },
        data: { topic: cid },
      });
      edges.push({ id: `${cid}__pub`, source: c.from, target: tid, style: { ...edgeStyle(c, t), labelText: cid }, data: { connection: cid } });
      for (const to of c.to) {
        edges.push({ id: `${cid}__${to}`, source: tid, target: to, style: { ...edgeStyle(c, t), labelText: "" }, data: { connection: cid } });
      }
    } else {
      for (const to of c.to) {
        edges.push({ id: `${cid}__${to}`, source: c.from, target: to, style: { ...edgeStyle(c, t), labelText: `${cid}${tag}` }, data: { connection: cid } });
      }
    }
  }
  return { nodes, edges };
}
