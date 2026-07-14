// The G6 wiring (derived from spike notes/009; not copied). Only rendering and event dispatch — state
// lives in the store. The selection chrome colors come from the --graph-* tokens in style.css
// (graph/theme.ts; recovers Task 12).
import { Graph } from "@antv/g6";
import type { IElementDragEvent, IElementEvent } from "@antv/g6";
import type { GraphData } from "./build-data";
import { TOPIC_PREFIX } from "./build-data";
import { graphTheme } from "./theme";

export interface Selected {
  kind: "node" | "edge";
  id: string;
}

export interface GraphHandlers {
  onSelect(sel: Selected | null): void;
  onNodeMoved(id: string, x: number, y: number): void;
}

export function createGraphView(container: HTMLElement, data: GraphData, h: GraphHandlers) {
  const t = graphTheme();
  const graph = new Graph({
    container,
    data,
    node: {
      state: {
        selected: { lineWidth: 3, stroke: t.selStroke, halo: true, haloStroke: t.halo, haloOpacity: 0.45, haloLineWidth: 12 },
      },
    },
    edge: {
      type: "quadratic",
      state: {
        selected: { lineWidth: 4, halo: true, haloStroke: t.halo, haloOpacity: 0.5, haloLineWidth: 10 },
      },
    },
    behaviors: ["drag-canvas", "zoom-canvas", "drag-element", { type: "hover-activate", degree: 0 }],
    // Auto-separate parallel / bidirectional edges (upper spec §9: ProcessParallelEdges)
    transforms: [{ type: "process-parallel-edges", mode: "bundle", distance: 40 }],
    // NOTE: no `autoFit` — G6 re-fits the viewport on EVERY render, so with autoFit a field edit (which
    // re-renders via setData) would shift/zoom the whole graph and nodes would appear to move on their
    // own. We fit once after the initial render instead (see render()).
  });

  // Follow container resizes (bug fix): G6 sizes the canvas to the container at creation but does not
  // observe later size changes, so growing the window/pane would clip the graph. A ResizeObserver keeps
  // the canvas in sync with the actual container box. Guarded against zero sizes (e.g. while hidden by
  // v-show) and against post-destroy calls.
  let destroyed = false;
  const ro =
    typeof ResizeObserver !== "undefined"
      ? new ResizeObserver(() => {
          if (destroyed) return;
          const w = container.clientWidth;
          const h = container.clientHeight;
          if (w > 0 && h > 0) {
            try {
              graph.setSize(w, h);
            } catch {
              /* a size call racing a re-render/destroy is harmless */
            }
          }
        })
      : null;
  ro?.observe(container);

  let lastSel: string | null = null;
  const hi = (id: string) => {
    try {
      if (lastSel && lastSel !== id) graph.setElementState(lastSel, []);
      graph.setElementState(id, ["selected"]);
      lastSel = id;
    } catch {
      /* an element vanishing right after a re-render is harmless */
    }
  };
  const clear = () => {
    try {
      if (lastSel) graph.setElementState(lastSel, []);
    } catch {
      /* noop */
    }
    lastSel = null;
  };

  graph.on<IElementEvent>("node:click", (e) => {
    const id = String(e.target.id);
    if (id.startsWith(TOPIC_PREFIX)) {
      h.onSelect({ kind: "edge", id: id.slice(TOPIC_PREFIX.length) });
      return;
    }
    hi(id);
    h.onSelect({ kind: "node", id });
  });
  graph.on<IElementEvent>("edge:click", (e) => {
    const id = String(e.target.id);
    hi(id);
    h.onSelect({ kind: "edge", id: id.split("__")[0] });
  });
  graph.on("canvas:click", () => {
    clear();
    h.onSelect(null);
  });
  graph.on<IElementDragEvent>("node:dragend", (e) => {
    const id = String(e.target.id);
    if (id.startsWith(TOPIC_PREFIX)) return; // a topic node's coordinates are not written to layout
    try {
      const p = graph.getElementPosition(id);
      h.onNodeMoved(id, Math.round(p[0]), Math.round(p[1]));
    } catch {
      /* noop */
    }
  });

  return {
    async render(): Promise<void> {
      await graph.render();
      // Fit once on the initial render only (autoFit is intentionally off — see the constructor note).
      try {
        await graph.fitView();
      } catch {
        /* fitting is best-effort */
      }
    },
    setData(d: GraphData): void {
      graph.setData(d);
      void graph.render(); // no re-fit here: keep the user's current pan/zoom while editing
    },
    /** Run auto-layout (antv-dagre) once and return the settled coordinates (the caller saves them to
     *  layout.sahou.json §6). Uses a one-shot layout call (not setLayout) so it is NOT kept as the graph's
     *  active layout — otherwise every later render would re-run dagre and override manual node moves. */
    async autoLayout(nodeIds: string[]): Promise<Record<string, { x: number; y: number }>> {
      await graph.layout({ type: "antv-dagre", rankdir: "LR", nodesep: 30, ranksep: 120 });
      try {
        await graph.fitView();
      } catch {
        /* fitting is best-effort */
      }
      const out: Record<string, { x: number; y: number }> = {};
      for (const id of nodeIds) {
        try {
          const p = graph.getElementPosition(id);
          out[id] = { x: Math.round(p[0]), y: Math.round(p[1]) };
        } catch {
          /* noop */
        }
      }
      return out;
    },
    destroy(): void {
      destroyed = true;
      ro?.disconnect();
      graph.destroy();
    },
  };
}
