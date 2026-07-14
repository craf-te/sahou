#!/usr/bin/env node
const { spawnSync } = require("child_process");
const path = require("path");
const os = require("os");

// Add .cargo/bin to PATH
const cargoPath = path.join(os.homedir(), ".cargo", "bin");
const env = { ...process.env, PATH: `${cargoPath}${path.delimiter}${process.env.PATH}` };

// Run wasm-pack
const result = spawnSync("wasm-pack", [
  "build",
  "../core",
  "--release",
  "--target",
  "web",
  "--out-dir",
  "../gui/src/core-wasm",
  "--",
  "--features",
  "wasm",
], { env, stdio: "inherit", shell: true });

// If spawnSync fails to even start the process, result.error is set and status becomes null.
// Calling process.exit(null) then means exit 0 (treated as success), silently swallowing the build
// failure, so we exit explicitly.
if (result.error) {
  console.error("Failed to start wasm-pack:", result.error.message);
  process.exit(1);
}
// When status is null/undefined (e.g. terminated by a signal), also fall back to a failure (1).
process.exit(result.status ?? 1);
