import { build, context } from "esbuild";
import { cp, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const out = path.join(root, "dist", "firefox");
const watch = process.argv.includes("--watch");
const entries = {
  "background/index": path.join(root, "src/background/index.ts"),
  "content/index": path.join(root, "src/content/index.ts"),
  "popup/index": path.join(root, "src/popup/index.ts"),
  "options/index": path.join(root, "src/options/index.ts"),
  "confirmation/index": path.join(root, "src/confirmation/index.ts"),
};

await rm(out, { recursive: true, force: true });
await mkdir(out, { recursive: true });
const base = JSON.parse(
  await readFile(path.join(root, "manifests/base.json"), "utf8"),
);
const firefox = JSON.parse(
  await readFile(path.join(root, "manifests/firefox.json"), "utf8"),
);
await writeFile(
  path.join(out, "manifest.json"),
  `${JSON.stringify(deepMerge(base, firefox), null, 2)}\n`,
);
await cp(path.join(root, "static"), out, { recursive: true });

const options = {
  entryPoints: entries,
  outdir: out,
  bundle: true,
  format: "esm",
  platform: "browser",
  target: "firefox142",
  sourcemap: false,
  minify: !watch,
  treeShaking: true,
  legalComments: "none",
  logLevel: "info",
};

if (watch) {
  const buildContext = await context(options);
  await buildContext.watch();
  console.log("Watching Ravyn Firefox extension sources...");
} else {
  await build(options);
}

function deepMerge(left, right) {
  if (Array.isArray(left) || Array.isArray(right)) return right ?? left;
  if (left && right && typeof left === "object" && typeof right === "object") {
    const result = { ...left };
    for (const [key, value] of Object.entries(right))
      result[key] = deepMerge(left[key], value);
    return result;
  }
  return right ?? left;
}
