#!/usr/bin/env node
const fs = require("fs");
const path = require("path");

// Ensure dist/.gitkeep exists after build
const gitkeepPath = path.join(__dirname, "..", "dist", ".gitkeep");
const distDir = path.dirname(gitkeepPath);

if (!fs.existsSync(distDir)) {
  fs.mkdirSync(distDir, { recursive: true });
}
if (!fs.existsSync(gitkeepPath)) {
  fs.writeFileSync(gitkeepPath, "");
}
