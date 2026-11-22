// Steps of .github/workflows/release.yml that are easier to test in Node:
//
//   node scripts/release-ci.mjs check-version [--tag v1.2.3]
//   node scripts/release-ci.mjs notes --tag v1.2.3 > notes.md
//   node scripts/release-ci.mjs manifest --tag v1.2.3 --notes notes.md \
//     --dir release-assets --repo owner/name > latest.json
import { execFileSync } from "node:child_process";
import { readdirSync, readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { parseArgs } from "node:util";

import {
  compareVersions,
  isPrerelease,
  parseVersion,
  readVersions,
  releaseNotes,
  updaterManifest,
  versionProblems,
} from "./lib/release.mjs";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
/** The updater's name for each archive, from the arch in its file name. */
const PLATFORMS = { aarch64: "darwin-aarch64" };
const ARCHIVE = /_([a-z0-9_]+)\.app\.tar\.gz$/;

const { positionals, values } = parseArgs({
  allowPositionals: true,
  options: {
    dir: { type: "string" },
    notes: { type: "string" },
    repo: { type: "string" },
    tag: { type: "string" },
  },
});

/** @param {string | undefined} tag */
function versionOf(tag) {
  if (!tag?.startsWith("v")) {
    throw new Error(`${tag ?? "No tag"} isn't a release tag like v1.2.3`);
  }
  const version = tag.slice(1);
  parseVersion(version);
  return version;
}

/** @param {string[]} args */
function git(args) {
  return execFileSync("git", args, { cwd: root, encoding: "utf8" }).trim();
}

/**
 * The tag the notes start after: the newest older stable release, or for a
 * prerelease the newest older tag of any kind.
 * @param {string} version
 */
function previousTag(version) {
  const older = git(["tag", "--list", "v*"])
    .split("\n")
    .filter(Boolean)
    .flatMap((tag) => {
      try {
        return [{ tag, version: versionOf(tag) }];
      } catch {
        return [];
      }
    })
    .filter((t) => compareVersions(t.version, version) < 0)
    .filter((t) => isPrerelease(version) || !isPrerelease(t.version))
    .sort((a, b) => compareVersions(b.version, a.version));
  return older[0]?.tag ?? null;
}

const commands = {
  "check-version"() {
    const expected = values.tag ? versionOf(values.tag) : null;
    const problems = versionProblems(readVersions(root), expected);
    if (problems.length > 0) {
      for (const problem of problems) {
        console.error(problem);
      }
      process.exit(1);
    }
    console.log(`Versions agree: ${Object.values(readVersions(root))[0]}`);
  },
  manifest() {
    const version = versionOf(values.tag);
    if (!(values.dir && values.repo && values.notes)) {
      throw new Error("manifest needs --dir, --repo and --notes");
    }
    const platforms = {};
    for (const file of readdirSync(values.dir)) {
      const arch = ARCHIVE.exec(file)?.[1];
      if (!arch) {
        continue;
      }
      const platform = PLATFORMS[arch];
      if (!platform) {
        throw new Error(`No updater platform for ${file}`);
      }
      platforms[platform] = {
        signature: readFileSync(join(values.dir, `${file}.sig`), "utf8").trim(),
        url: `https://github.com/${values.repo}/releases/download/${values.tag}/${file}`,
      };
    }
    const manifest = updaterManifest({
      notes: readFileSync(values.notes, "utf8").trim(),
      platforms,
      pubDate: new Date(),
      version,
    });
    console.log(JSON.stringify(manifest, null, 2));
  },
  notes() {
    const version = versionOf(values.tag);
    const previous = previousTag(version);
    const range = previous ? `${previous}..${values.tag}` : values.tag;
    const subjects = git(["log", "--format=%s", range ?? "HEAD"])
      .split("\n")
      .filter(Boolean);
    console.log(releaseNotes(subjects));
  },
};

const command = commands[/** @type {keyof typeof commands} */ (positionals[0])];
if (!command) {
  console.error(`Usage: release-ci.mjs <${Object.keys(commands).join("|")}>`);
  process.exit(2);
}
command();
