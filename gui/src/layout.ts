// layout.sahou.json (GUI-only, kept separate from the contract §6). It holds coordinates only and is never
// written back into the contract.
export interface LayoutFile {
  nodes: Record<string, { x: number; y: number }>;
}

export const emptyLayout = (): LayoutFile => ({ nodes: {} });

export const withNodePos = (l: LayoutFile, id: string, x: number, y: number): LayoutFile => ({
  ...l,
  nodes: { ...l.nodes, [id]: { x, y } },
});
