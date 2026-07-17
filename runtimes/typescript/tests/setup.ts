// zenoh-ts 1.9.0's Session.close() fire-and-forgets its inner close(): session.js calls
// `this.inner.close()` without awaiting or returning it, and inner.close() awaits Link.close().
// When the WS close handshake loses the race with server-side socket teardown, Link.close()
// rejects with "WebSocket error during close" — and because nobody awaits inner.close(), that
// rejection is unhandled. Our engine already awaits session.close() correctly (see engine.ts),
// so the promise we hold resolves immediately and the background rejection is unrecoverable at
// our layer. It only surfaces where the server dies mid-close (CI on Linux/undici), not on macOS.
//
// Swallow *only* that exact message; re-throw everything else so genuine unhandled rejections
// still fail the run. Vitest steps aside as soon as a second unhandledRejection listener exists
// (its worker handler bails on `process.listeners(event).length > 1`), which makes this handler
// the sole arbiter — hence the deliberate re-throw of anything we don't recognize.
//
// Upstream fix belongs in zenoh-ts (Session.close should `return this.inner.close()`).
const ZENOH_CLOSE_RACE = "WebSocket error during close";

process.on("unhandledRejection", (reason) => {
  if (reason instanceof Error && reason.message === ZENOH_CLOSE_RACE) return;
  throw reason; // not ours → surface it (Vitest no longer will); becomes an uncaughtException
});
