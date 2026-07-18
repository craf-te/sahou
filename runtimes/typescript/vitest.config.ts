import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    environment: "node",
    // Only the unit/integration suite; the real-browser e2e (tests-e2e/**) runs under its own
    // vitest.e2e.config.ts so the default `vitest run` stays fast and playwright-free.
    include: ["tests/**/*.test.ts"],
    // Filters the benign "WebSocket error during close" unhandled rejection from zenoh-ts's
    // fire-and-forget Session.close() (see tests/setup.ts); re-throws anything else.
    setupFiles: ["./tests/setup.ts"],
    testTimeout: 30_000,
    hookTimeout: 60_000,
    fileParallelism: false,
    // @eclipse-zenoh/zenoh-ts imports wasm via ESM (a build that assumes a bundler).
    // The bare Node ESM loader cannot interpret this, so enable it with a flag (experimental on Node v24).
    poolOptions: {
      forks: { execArgv: ["--experimental-wasm-modules"] },
      threads: { execArgv: ["--experimental-wasm-modules"] },
    },
  },
});
