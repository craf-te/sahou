import vue from "@vitejs/plugin-vue";
import { defineConfig } from "vitest/config";

// During dev, proxy /api to the Rust backend (`sahou gui --port 4649`).
// In production, `vite build` → the CLI's rust-embed bundles dist/ into the binary (design §1).
export default defineConfig({
  plugins: [vue()],
  server: { port: 5179, proxy: { "/api": "http://127.0.0.1:4649" } },
  test: { environment: "happy-dom", passWithNoTests: true },
});
