// A thin client for the backend file API (raw text + etag). It never interprets the contract —
// that is core-bridge's job (keeping the design §1 division of labor inside the frontend too).
export type FileKind = "schema" | "layout" | "endpoints";

export interface FileState {
  text: string;
  etag: string;
}

export interface FilesResponse {
  schema: FileState | null;
  layout: FileState | null;
  endpoints: FileState | null;
  env: string;
}

export interface WatchEvent {
  kind: FileKind;
  etag: string | null; // null = file deleted
}

export async function getFiles(): Promise<FilesResponse> {
  const res = await fetch("/api/files");
  if (!res.ok) throw new Error(`GET /api/files returned ${res.status}`);
  return (await res.json()) as FilesResponse;
}

/** PUT 409 (compare-and-swap mismatch). currentEtag = the server's current etag. */
export class PutConflict extends Error {
  constructor(public currentEtag: string | null) {
    super("conflict with an external edit (etag mismatch)");
    this.name = "PutConflict";
  }
}

/** Compare-and-swap PUT. etag=null means "create new" (no If-Match). Returns the new etag.
 *  Besides 200/409/404 it can also return 400 (body read failure — carried over from Task 7);
 *  all of those are surfaced to the caller as generic errors without losing the local edit. */
export async function putFile(kind: FileKind, text: string, etag: string | null): Promise<string> {
  const headers: Record<string, string> = { "Content-Type": "text/plain; charset=utf-8" };
  if (etag !== null) headers["If-Match"] = etag;
  const res = await fetch(`/api/files/${kind}`, { method: "PUT", headers, body: text });
  const body = (await res.json()) as { etag?: string | null; error?: string };
  if (res.status === 409) throw new PutConflict(body.etag ?? null);
  if (!res.ok) throw new Error(body.error ?? `PUT /api/files/${kind} returned ${res.status}`);
  return body.etag as string;
}

/** Grace period before a persistently failing SSE connection is treated as "server stopped". */
export const WATCH_DOWN_GRACE_MS = 2500;

export interface WatchOptions {
  /** Called once when the SSE connection stays down past the grace period (server stopped). */
  onDown?: () => void;
}

/** Subscribe to SSE. The returned function unsubscribes.
 *  EventSource auto-reconnects on error; if it keeps failing past WATCH_DOWN_GRACE_MS (e.g. the backend
 *  was stopped), onDown fires once so the UI can react (close the app-mode window / show an overlay). */
export function openWatch(onEvent: (e: WatchEvent) => void, opts: WatchOptions = {}): () => void {
  const es = new EventSource("/api/watch");
  let downTimer: ReturnType<typeof setTimeout> | null = null;
  let closed = false;
  const clearDown = () => {
    if (downTimer !== null) {
      clearTimeout(downTimer);
      downTimer = null;
    }
  };
  es.onopen = clearDown; // reconnected before the grace period: it was only transient
  es.onmessage = (m) => onEvent(JSON.parse(m.data as string) as WatchEvent);
  es.onerror = () => {
    if (closed || downTimer !== null || !opts.onDown) return;
    downTimer = setTimeout(() => {
      if (!closed) opts.onDown?.();
    }, WATCH_DOWN_GRACE_MS);
  };
  return () => {
    closed = true;
    clearDown();
    es.close();
  };
}
