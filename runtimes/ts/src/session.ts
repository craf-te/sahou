// Shared Session.open for the link WS (deduplicates node/browser; ②c backlog).
import { Config, Session } from "@eclipse-zenoh/zenoh-ts";
import { Diag, SahouRejected } from "./diag.js";

/** Upper bound (ms) on how long to wait for Session.open itself. */
export const SESSION_OPEN_TIMEOUT_MS = 3000;

/** Structured NO for an unreachable link. hint = per-entry remediation steps (node = spawn guidance / browser = startup guidance). */
export function linkUnavailable(detail: string, hint: string): SahouRejected {
  const diag: Diag = { code: "link_unavailable", path: "$", message: `${detail}. ${hint}` };
  return new SahouRejected([diag]);
}

/**
 * Enforce an upper time bound on `Session.open`.
 * zenoh-ts WS connections retry indefinitely against an unreachable locator (known behavior), and a bare
 * await never resolves. Here we give up and return `link_unavailable` (say NO as early as possible).
 * If a connection is established in the background after the deadline, discard and close it (do not silently hold on to it).
 *
 * Important: this timeout only cuts off the "wait" of Session.open; it cannot stop zenoh-ts's internal
 * WS reconnect loop (an upstream bug where RemoteLink.new fails to increment retries). Callers must not
 * hammer connect() in a tight loop (see the comments in each entry point).
 */
export function openSessionWithTimeout(locator: string, hint: string): Promise<Session> {
  return new Promise((resolve, reject) => {
    let settled = false;
    const timer = setTimeout(() => {
      if (settled) return;
      settled = true;
      reject(
        linkUnavailable(
          `cannot connect to link (${locator}): timed out (${SESSION_OPEN_TIMEOUT_MS}ms)`,
          hint,
        ),
      );
    }, SESSION_OPEN_TIMEOUT_MS);
    Session.open(new Config(locator)).then(
      (session) => {
        if (settled) {
          void session
            .close()
            .catch((e) => console.warn("[sahou] failed to close a session established after the deadline", e));
          return;
        }
        settled = true;
        clearTimeout(timer);
        resolve(session);
      },
      (e: unknown) => {
        if (settled) return;
        settled = true;
        clearTimeout(timer);
        reject(linkUnavailable(`cannot connect to link (${locator}): ${String(e)}`, hint));
      },
    );
  });
}
