// Downloads the native forno binary for this platform from GitHub releases.
// Set FORNO_REPO (owner/name) to the repository that publishes releases.
const { execFileSync } = require("child_process");
const fs = require("fs");
const os = require("os");
const path = require("path");

const pkg = require("./package.json");
const repo = process.env.FORNO_REPO || "OWNER/forno";
const targets = {
  "linux-x64": "x86_64-unknown-linux-musl",
  "linux-arm64": "aarch64-unknown-linux-musl",
  "darwin-x64": "x86_64-apple-darwin",
  "darwin-arm64": "aarch64-apple-darwin",
  "win32-x64": "x86_64-pc-windows-msvc",
};
const target = targets[`${process.platform}-${process.arch}`];
if (!target) {
  console.error(`forno: unsupported platform ${process.platform}-${process.arch}; try \`cargo install forno-cli\``);
  process.exit(1);
}
const win = process.platform === "win32";
const ext = win ? "zip" : "tar.gz";
const url = `https://github.com/${repo}/releases/download/v${pkg.version}/forno-${target}.${ext}`;
const dir = path.join(__dirname, "native");
fs.mkdirSync(dir, { recursive: true });
const archive = path.join(os.tmpdir(), `forno-${process.pid}.${ext}`);

async function main() {
  const res = await fetch(url);
  if (!res.ok) throw new Error(`download failed (${res.status}): ${url}`);
  fs.writeFileSync(archive, Buffer.from(await res.arrayBuffer()));
  if (win) {
    execFileSync("powershell", ["-NoProfile", "-Command", `Expand-Archive -Force '${archive}' '${dir}'`]);
  } else {
    execFileSync("tar", ["xzf", archive, "-C", dir]);
    fs.chmodSync(path.join(dir, "forno"), 0o755);
  }
  fs.unlinkSync(archive);
}
main().catch((e) => {
  console.error(`forno: ${e.message}`);
  process.exit(1);
});
