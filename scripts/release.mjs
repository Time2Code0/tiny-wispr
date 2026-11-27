// Prepares a release: `pnpm release 0.2.0` (or `0.2.0-rc.1`).
//
// Sets the version in package.json, tauri.conf.json and Cargo.toml,
// refreshes Cargo.lock, adds the notes to CHANGELOG.md, commits
// `chore(release): v0.2.0` and tags it. It doesn't push: it prints the
// command, and pushing the tag starts .github/workflows/release.yml.
import { execFileSync } from "node:child_process";
import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

import {
  compareVersions,
  isPrerelease,
  prependChangelog,
  readVersions,
  releaseNotes,
  versionProblems,
  writeVersions,
} from "./lib/release.mjs";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");

/** @param {string} command @param {string[]} args */
function run(command, args) {
  return execFileSync(command, args, { cwd: root, encoding: "utf8" }).trim();
}

/** @param {string} message */
function fail(message) {
  console.error(message);
  process.exit(1);
}

const [, , version] = process.argv;
if (!version) {
  fail("Usage: pnpm release <version>, for example pnpm release 0.2.0");
}

const current = readVersions(root);
const problems = versionProblems(current);
if (problems.length > 0) {
  fail(problems.join("\n"));
}
const previous = run("git", ["tag", "--list", "v*", "--sort=-v:refname"])
  .split("\n")
  .find(Boolean);
const currentVersion = current["package.json"] ?? "0.0.0";
// Before the first tag, the version in the files hasn't been released yet.
const order = compareVersions(version, currentVersion);
if (order < 0 || (order === 0 && previous)) {
  fail(`${version} isn't newer than ${currentVersion}`);
}
if (run("git", ["status", "--porcelain"])) {
  fail("Commit or stash your changes first");
}
const branch = run("git", ["branch", "--show-current"]);
if (!isPrerelease(version) && branch !== "main") {
  fail(`Release ${version} from main; ${branch} can only make prereleases`);
}
const tag = `v${version}`;
if (run("git", ["tag", "--list", tag])) {
  fail(`${tag} already exists`);
}

const subjects = run("git", [
  "log",
  "--format=%s",
  previous ? `${previous}..HEAD` : "HEAD",
])
  .split("\n")
  .filter(Boolean);
const notes = releaseNotes(subjects);

writeVersions(root, version);
run("cargo", ["update", "--workspace", "--offline"]);
const changelogPath = join(root, "CHANGELOG.md");
const changelog = existsSync(changelogPath)
  ? readFileSync(changelogPath, "utf8")
  : "";
writeFileSync(
  changelogPath,
  prependChangelog(changelog, version, notes, new Date())
);

run("git", [
  "add",
  "package.json",
  "src-tauri/tauri.conf.json",
  "Cargo.toml",
  "Cargo.lock",
  "CHANGELOG.md",
]);
run("git", ["commit", "--quiet", "-m", `chore(release): ${tag}`]);
run("git", ["tag", "--annotate", tag, "-m", `Tiny Wispr ${version}`]);

console.log(`Tagged ${tag}:\n\n${notes}\n`);
console.log(
  `Push it to build the release:\n  git push origin ${branch} ${tag}`
);
