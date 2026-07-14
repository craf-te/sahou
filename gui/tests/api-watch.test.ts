// Server-down detection in openWatch (§bug4). EventSource is stubbed with a controllable fake so the
// grace-period logic can be driven with fake timers (happy-dom does not implement EventSource).
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { openWatch, WATCH_DOWN_GRACE_MS } from "../src/api";

class FakeEventSource {
  static instances: FakeEventSource[] = [];
  onopen: (() => void) | null = null;
  onmessage: ((m: { data: string }) => void) | null = null;
  onerror: (() => void) | null = null;
  closed = false;
  constructor(public url: string) {
    FakeEventSource.instances.push(this);
  }
  close() {
    this.closed = true;
  }
}

const latest = () => FakeEventSource.instances[FakeEventSource.instances.length - 1];

describe("openWatch server-down detection", () => {
  beforeEach(() => {
    FakeEventSource.instances = [];
    vi.stubGlobal("EventSource", FakeEventSource as unknown as typeof EventSource);
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
    vi.unstubAllGlobals();
  });

  it("fires onDown once after the grace period on a persistent error", () => {
    const onDown = vi.fn();
    openWatch(() => {}, { onDown });
    latest().onerror?.();
    expect(onDown).not.toHaveBeenCalled(); // not yet — within the grace period
    vi.advanceTimersByTime(WATCH_DOWN_GRACE_MS);
    expect(onDown).toHaveBeenCalledTimes(1);
    // repeated errors do not re-fire
    latest().onerror?.();
    vi.advanceTimersByTime(WATCH_DOWN_GRACE_MS);
    expect(onDown).toHaveBeenCalledTimes(1);
  });

  it("does not fire onDown if the connection recovers within the grace period", () => {
    const onDown = vi.fn();
    openWatch(() => {}, { onDown });
    latest().onerror?.();
    vi.advanceTimersByTime(WATCH_DOWN_GRACE_MS - 1);
    latest().onopen?.(); // reconnected: only transient
    vi.advanceTimersByTime(WATCH_DOWN_GRACE_MS);
    expect(onDown).not.toHaveBeenCalled();
  });

  it("does not fire onDown after unsubscribe (and closes the stream)", () => {
    const onDown = vi.fn();
    const stop = openWatch(() => {}, { onDown });
    const es = latest();
    es.onerror?.();
    stop();
    vi.advanceTimersByTime(WATCH_DOWN_GRACE_MS);
    expect(onDown).not.toHaveBeenCalled();
    expect(es.closed).toBe(true);
  });

  it("dispatches watch messages to onEvent", () => {
    const onEvent = vi.fn();
    openWatch(onEvent);
    latest().onmessage?.({ data: JSON.stringify({ kind: "schema", etag: "x" }) });
    expect(onEvent).toHaveBeenCalledWith({ kind: "schema", etag: "x" });
  });
});
