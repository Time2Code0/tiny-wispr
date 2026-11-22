// Builds the language model worker and places it where Tauri expects a
// sidecar: src-tauri/binaries/tw-llm-worker-<target triple>.
import { execFileSync } from "node:child_process";
import { copyFileSync, mkdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const release = process.argv.includes("--release");
const triple = /host: (\S+)/.exec(
  execFileSync("rustc", ["-vV"], { encoding: "utf8" })
)?.[1];
if (!triple) {
  throw new Error("Could not read the host target from rustc");
}

execFileSync(
  "cargo",
  ["build", "-p", "tw-llm-worker", ...(release ? ["--release"] : [])],
  { cwd: root, stdio: "inherit" }
);

const extension = triple.includes("windows") ? ".exe" : "";
const built = join(
  root,
  "target",
  release ? "release" : "debug",
  `tw-llm-worker${extension}`
);
const sidecars = join(root, "src-tauri", "binaries");
mkdirSync(sidecars, { recursive: true });
copyFileSync(built, join(sidecars, `tw-llm-worker-${triple}${extension}`));
