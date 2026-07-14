import { defineConfig } from "vite";

// Set publicDir one level up (runtime/) so gen/descriptor.json is served at /gen/descriptor.json
// (In ②d the output location was reorganized under runtime/gen/. Keep publicDir=runtime/ and resolve it on the fetch-path side.)
export default defineConfig({ publicDir: "..", server: { port: 5173 } });
