// Immutable updates of the endpoints JSON (symmetric with the contract; for the deploy tab §6).
// auto / empty = "don't write it" = progressive disclosure (upper spec §3: empty is the default = LAN auto).
import type { Endpoints, NodeEndpoint } from "../core-bridge";

function withNode(e: Endpoints, id: string, patch: Partial<NodeEndpoint>): Endpoints {
  const prev = e.nodes[id] ?? {};
  const next: NodeEndpoint = { ...prev, ...patch };
  for (const [k, v] of Object.entries(patch)) {
    if (v === undefined) delete (next as Record<string, unknown>)[k];
  }
  const nodes = { ...e.nodes };
  if (Object.keys(next).length === 0) delete nodes[id];
  else nodes[id] = next;
  return { ...e, nodes };
}

export const setNamespace = (e: Endpoints, ns: string): Endpoints => ({
  ...e,
  namespace: ns || "sahou",
});

export function setEnv(e: Endpoints, env: string): Endpoints {
  if (!env) {
    const { env: _env, ...rest } = e;
    return rest as Endpoints;
  }
  return { ...e, env };
}

export function setRouter(e: Endpoints, enabled: boolean, endpoint?: string): Endpoints {
  if (!enabled) {
    const { router: _router, ...rest } = e;
    return rest as Endpoints;
  }
  return { ...e, router: { enabled: true, ...(endpoint ? { endpoint } : {}) } };
}

export const setNodeMode = (e: Endpoints, id: string, mode: "auto" | "peer" | "client"): Endpoints =>
  withNode(e, id, { mode: mode === "auto" ? undefined : mode });

export function setNodeConnect(e: Endpoints, id: string, csv: string): Endpoints {
  const arr = csv.split(",").map((s) => s.trim()).filter(Boolean);
  return withNode(e, id, { connect: arr.length ? arr : undefined });
}

export function setPlugins(e: Endpoints, csv: string): Endpoints {
  const plugins = csv.split(",").map((s) => s.trim()).filter(Boolean);
  const { plugins: _plugins, ...rest } = e;
  return plugins.length ? { ...rest, plugins } : (rest as Endpoints);
}
