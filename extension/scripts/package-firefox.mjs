import { mkdir, readFile, rm } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { spawn } from "node:child_process";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const manifest = JSON.parse(
  await readFile(path.join(root, "dist/firefox/manifest.json"), "utf8"),
);
const artifacts = path.join(root, "artifacts");
const target = path.join(artifacts, `ravyn-firefox-${manifest.version}.xpi`);
await mkdir(artifacts, { recursive: true });
await rm(target, { force: true });
await run(
  "python3",
  [
    "scripts/deterministic-zip.py",
    "--root",
    "dist/firefox",
    "--output",
    target,
    ".",
  ],
  root,
);
console.log(target);

function run(command, args, cwd) {
  return new Promise((resolve, reject) => {
    const child = spawn(command, args, { cwd, stdio: "inherit" });
    child.once("error", reject);
    child.once("exit", (code) =>
      code === 0
        ? resolve()
        : reject(new Error(`${command} exited with ${code}`)),
    );
  });
}
