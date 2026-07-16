import { mkdir, readFile, rm } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { spawn } from "node:child_process";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const packageMetadata = JSON.parse(
  await readFile(path.join(root, "package.json"), "utf8"),
);
const artifacts = path.join(root, "artifacts");
const output = path.join(
  artifacts,
  `ravyn-firefox-source-${packageMetadata.version}.zip`,
);
const entries = [
  "package.json",
  "package-lock.json",
  "tsconfig.json",
  "vitest.config.ts",
  "eslint.config.js",
  "web-ext-config.mjs",
  "manifests",
  "scripts",
  "src",
  "static",
  "test-pages",
  "README.md",
  "PRIVACY.md",
  "THREAT_MODEL.md",
  "AMO_SUBMISSION.md",
];

await mkdir(artifacts, { recursive: true });
await rm(output, { force: true });
await run(
  "python3",
  [
    "scripts/deterministic-zip.py",
    "--root",
    ".",
    "--output",
    output,
    ...entries,
  ],
  root,
);
console.log(output);

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
