import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import { spawn } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const metadata = JSON.parse(
  await readFile(path.join(root, "package.json"), "utf8"),
);
const xpi = path.join(
  root,
  "artifacts",
  `ravyn-firefox-${metadata.version}.xpi`,
);
const source = path.join(
  root,
  "artifacts",
  `ravyn-firefox-source-${metadata.version}.zip`,
);

await run("npm", ["run", "package"], root);
await run("npm", ["run", "build:source"], root);
const first = [await digest(xpi), await digest(source)];
await run("npm", ["run", "package"], root);
await run("npm", ["run", "build:source"], root);
const second = [await digest(xpi), await digest(source)];

if (first[0] !== second[0] || first[1] !== second[1]) {
  throw new Error(
    `Extension artifacts are not reproducible: ${first.join(" ")} != ${second.join(" ")}`,
  );
}
console.log(`Reproducible XPI: ${first[0]}`);
console.log(`Reproducible source: ${first[1]}`);

async function digest(file) {
  return createHash("sha256")
    .update(await readFile(file))
    .digest("hex");
}

function run(command, args, cwd) {
  return new Promise((resolve, reject) => {
    const isWindows = process.platform === "win32";
    const child = isWindows
      ? spawn([command, ...args].join(" "), {
          cwd,
          stdio: "inherit",
          shell: true,
        })
      : spawn(command, args, { cwd, stdio: "inherit" });
    child.once("error", reject);
    child.once("exit", (code) =>
      code === 0
        ? resolve()
        : reject(new Error(`${command} exited with ${code}`)),
    );
  });
}
