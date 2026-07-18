// e2e fixture: drive the browser entry (src/browser.ts) against a real `sahou link` over WS.
// Reads ?link=<port>&node=<name> from the URL, connects, and reports progress on window.__status
// ("loading" → "connected" | "error: <message>") for the test to poll. The node handle is parked
// on window.__node so the engine (its liveliness token + vitals queryable) stays alive until the
// test closes the page.
import { connect } from "../../src/browser.ts";
import descriptor from "../../../python/tests/fixtures/descriptor_base.json";

declare global {
  interface Window {
    __status: string;
    __node?: unknown;
  }
}

/** window.__status is the machine-readable channel (the test compares it exactly); the DOM mirror is for humans driving the fixture manually. */
function show(status: string, detail = ""): void {
  window.__status = status;
  document.body.textContent = detail ? `${status} — ${detail}` : status;
}

show("loading");

async function main(): Promise<void> {
  const params = new URLSearchParams(location.search);
  const port = params.get("link");
  const node = params.get("node");
  if (!port || !node) throw new Error("missing ?link=<port>&node=<name>");
  const handle = await connect(descriptor, { node, locator: `ws://127.0.0.1:${port}` });
  window.__node = handle; // keep the engine alive (do not close) for the observer
  show("connected", `vitals declared as node "${node}" (close this tab and the liveliness token disappears)`);
}

main().catch((e: unknown) => {
  show(`error: ${e instanceof Error ? e.message : String(e)}`);
});
