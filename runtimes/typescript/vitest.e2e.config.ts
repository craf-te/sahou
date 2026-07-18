import { defineConfig } from "vitest/config";

// Real-browser e2e (playwright-driven Chromium + a vite-built fixture). Kept in a separate config
// so the default `vitest run` (vitest.config.ts) stays fast and playwright-free. Timeouts are
// generous: the first page load fetches + instantiates two wasm modules (sahou core + zenoh-ts).
export default defineConfig({
  test: {
    environment: "node",
    include: ["tests-e2e/**/*.test.ts"],
    // Same setup as the unit suite: swallow zenoh-ts's benign close-race unhandled rejection.
    setupFiles: ["./tests/setup.ts"],
    testTimeout: 60_000,
    hookTimeout: 120_000,
    fileParallelism: false,
    // The node-side observer imports zenoh-ts, whose wasm is imported via ESM (see vitest.config.ts).
    poolOptions: {
      forks: { execArgv: ["--experimental-wasm-modules"] },
      threads: { execArgv: ["--experimental-wasm-modules"] },
    },
  },
});
