import { fileURLToPath } from "node:url";
import { defineConfig } from "vite";
import wasm from "vite-plugin-wasm";

// Build-only config (no dev server) for the browser-vitals e2e fixture. root = the fixture dir
// so index.html is the build entry; the bundle lands in a gitignored dir inside tests-e2e.
//
// vite-plugin-wasm is required because @eclipse-zenoh/zenoh-ts's key-expr module imports its wasm
// as an ES module (`import * as wasm from "./zenoh_keyexpr_wasm_bg.wasm"`), which Vite cannot
// resolve natively (it is the same ESM-wasm build that makes the node suite pass
// --experimental-wasm-modules; see vitest.config.ts). The sahou core (wasm-pack "web" target)
// uses `new URL("sahou_core_bg.wasm", import.meta.url)` instead, which Vite handles on its own.
// `build.target: "esnext"` covers the top-level await that vite-plugin-wasm's glue emits, so no
// vite-plugin-top-level-await is needed.
export default defineConfig({
  root: fileURLToPath(new URL("fixture", import.meta.url)),
  plugins: [wasm()],
  build: {
    target: "esnext",
    outDir: fileURLToPath(new URL("dist-fixture", import.meta.url)),
    emptyOutDir: true, // the outDir sits outside root; opt in to letting Vite clean it
  },
});
