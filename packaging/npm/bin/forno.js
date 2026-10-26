#!/usr/bin/env node
// Thin launcher for the native binary downloaded by install.js.
const { spawnSync } = require("child_process");
const path = require("path");
const exe = path.join(__dirname, "..", "native", process.platform === "win32" ? "forno.exe" : "forno");
const r = spawnSync(exe, process.argv.slice(2), { stdio: "inherit" });
if (r.error) {
  console.error(`forno: ${r.error.message}. Reinstall the package or run \`node install.js\`.`);
  process.exit(1);
}
process.exit(r.status ?? 1);
