// Shared by the release scripts and the release workflow. Pure functions
// where possible, so `release.test.ts` can cover them.
import { readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";

/** `X.Y.Z` or `X.Y.Z-pre.N`; build metadata isn't used. */
const VERSION = /^(\d+)\.(\d+)\.(\d+)(?:-([0-9A-Za-z.-]+))?$/;
const CARGO_VERSION = /(\[workspace\.package\][^[]*?\nversion = ")([^"]+)(")/;
const SUBJECT = /^(\w+)(?:\(([^)]*)\))?(!)?: (.+)$/;

/** @param {string} version */
export function parseVersion(version) {
  const match = VERSION.exec(version);
  if (!match) {
    throw new Error(`${version} isn't a version like 1.2.3 or 1.2.3-rc.1`);
  }
  return {
    major: Number(match[1]),
    minor: Number(match[2]),
    patch: Number(match[3]),
    pre: match[4] ?? null,
  };
}

/** @param {string} version */
export function isPrerelease(version) {
  return parseVersion(version).pre !== null;
}

const NUMERIC = /^\d+$/;
const JSON_VERSION = /("version":\s*")[^"]+(")/;

/**
 * One dot-separated prerelease part against another: numbers by value,
 * words alphabetically, numbers before words.
 * @param {string} a
 * @param {string} b
 */
function compareIdentifiers(a, b) {
  const an = NUMERIC.test(a);
  const bn = NUMERIC.test(b);
  if (an && bn) {
    return Number(a) - Number(b);
  }
  if (an !== bn) {
    return an ? -1 : 1;
  }
  return a.localeCompare(b);
}

/**
 * `rc.1` against `rc.2`; no prerelease sorts after any.
 * @param {string | null} a
 * @param {string | null} b
 */
function comparePrereleases(a, b) {
  if (a === b) {
    return 0;
  }
  if (a === null || b === null) {
    return a === null ? 1 : -1;
  }
  const left = a.split(".");
  const right = b.split(".");
  const shared = Math.min(left.length, right.length);
  for (let i = 0; i < shared; i += 1) {
    const order = compareIdentifiers(left[i], right[i]);
    if (order !== 0) {
      return order;
    }
  }
  return left.length - right.length;
}

/**
 * Semver order: `1.2.3-rc.1` < `1.2.3` < `1.2.4`.
 * @param {string} a
 * @param {string} b
 * @returns {number} negative, zero or positive
 */
export function compareVersions(a, b) {
  const x = parseVersion(a);
  const y = parseVersion(b);
  return (
    x.major - y.major ||
    x.minor - y.minor ||
    x.patch - y.patch ||
    comparePrereleases(x.pre, y.pre)
  );
}

/**
 * The version in each of the three files that must agree.
 * @param {string} root
 */
export function readVersions(root) {
  const pkg = JSON.parse(readFileSync(join(root, "package.json"), "utf8"));
  const tauri = JSON.parse(
    readFileSync(join(root, "src-tauri/tauri.conf.json"), "utf8")
  );
  const cargo = CARGO_VERSION.exec(
    readFileSync(join(root, "Cargo.toml"), "utf8")
  );
  return {
    "Cargo.toml": cargo?.[2] ?? null,
    "package.json": pkg.version ?? null,
    "src-tauri/tauri.conf.json": tauri.version ?? null,
  };
}

/**
 * Problems with the versions, empty when they agree (and equal `expected`).
 * @param {Record<string, string | null>} versions
 * @param {string | null} expected
 */
export function versionProblems(versions, expected = null) {
  const problems = [];
  const values = new Set(Object.values(versions));
  if (values.size > 1) {
    problems.push(
      `The versions differ: ${Object.entries(versions)
        .map(([file, v]) => `${file} ${v}`)
        .join(", ")}`
    );
  }
  for (const [file, version] of Object.entries(versions)) {
    if (version === null) {
      problems.push(`${file} has no version`);
    } else if (expected !== null && version !== expected) {
      problems.push(`${file} is ${version}, the tag says ${expected}`);
    }
  }
  return problems;
}

/**
 * Writes `version` into the three files, keeping their formatting.
 * @param {string} root
 * @param {string} version
 */
export function writeVersions(root, version) {
  parseVersion(version);
  for (const file of ["package.json", "src-tauri/tauri.conf.json"]) {
    const path = join(root, file);
    const text = readFileSync(path, "utf8");
    const next = text.replace(JSON_VERSION, `$1${version}$2`);
    writeFileSync(path, next);
  }
  const cargo = join(root, "Cargo.toml");
  writeFileSync(
    cargo,
    readFileSync(cargo, "utf8").replace(CARGO_VERSION, `$1${version}$3`)
  );
}

const SECTIONS = /** @type {const} */ ([
  ["feat", "Features"],
  ["fix", "Fixes"],
  ["perf", "Performance"],
]);

/** "add push-to-talk" → "Add push-to-talk". @param {string} text */
function sentence(text) {
  return text.charAt(0).toUpperCase() + text.slice(1);
}

/**
 * Markdown notes from conventional commit subjects. Only features, fixes and
 * performance work are listed; chores, docs, tests and builds are left out.
 * @param {string[]} subjects newest first, as `git log` prints them
 */
export function releaseNotes(subjects) {
  /** @type {Map<string, string[]>} */
  const groups = new Map();
  for (const subject of subjects) {
    const match = SUBJECT.exec(subject.trim());
    if (!match) {
      continue;
    }
    const [, type, , breaking, description] = match;
    const items = groups.get(type) ?? [];
    items.push(
      breaking ? `${sentence(description)} (breaking)` : sentence(description)
    );
    groups.set(type, items);
  }
  const parts = [];
  for (const [type, title] of SECTIONS) {
    const items = groups.get(type);
    if (items?.length) {
      parts.push(`### ${title}\n\n${items.map((i) => `- ${i}`).join("\n")}`);
    }
  }
  return parts.length > 0
    ? parts.join("\n\n")
    : "Small changes behind the scenes.";
}

/**
 * The `latest.json` the updater reads.
 * @param {{
 *   version: string,
 *   notes: string,
 *   pubDate: Date,
 *   platforms: Record<string, { url: string, signature: string }>,
 * }} release
 */
export function updaterManifest({ version, notes, pubDate, platforms }) {
  parseVersion(version);
  if (Object.keys(platforms).length === 0) {
    throw new Error("The manifest needs at least one platform");
  }
  for (const [platform, { url, signature }] of Object.entries(platforms)) {
    if (!url.startsWith("https://")) {
      throw new Error(`${platform}: the download address must use https`);
    }
    if (!signature.trim()) {
      throw new Error(`${platform}: the signature is empty`);
    }
  }
  return {
    notes,
    platforms,
    pub_date: pubDate.toISOString(),
    version,
  };
}

/**
 * Which release file the updater downloads on each platform, by file name.
 * The plugin looks for `<os>-<arch>-<installer>` first, then `<os>-<arch>`,
 * so a `.deb` install gets the `.deb` and every other Linux install the
 * AppImage.
 */
const UPDATE_FILES = /** @type {const} */ ([
  [/_aarch64\.app\.tar\.gz$/, ["darwin-aarch64-app", "darwin-aarch64"]],
  [/_x64-setup\.exe$/, ["windows-x86_64-nsis", "windows-x86_64"]],
  [/_amd64\.AppImage$/, ["linux-x86_64-appimage", "linux-x86_64"]],
  [/_amd64\.deb$/, ["linux-x86_64-deb"]],
]);

/**
 * The updater platforms each signed release file serves. Files without a
 * `.sig` next to them aren't updates (the DMG, the checksums).
 * @param {string[]} files names in the release folder
 * @returns {Record<string, string>} platform → file name
 */
export function updatePlatforms(files) {
  const present = new Set(files);
  /** @type {Record<string, string>} */
  const platforms = {};
  for (const file of files) {
    if (!present.has(`${file}.sig`)) {
      continue;
    }
    const match = UPDATE_FILES.find(([pattern]) => pattern.test(file));
    if (!match) {
      throw new Error(`${file} is signed, but no updater platform uses it`);
    }
    for (const platform of match[1]) {
      platforms[platform] = file;
    }
  }
  return platforms;
}

/**
 * Prepends a section for `version` to the changelog text.
 * @param {string} changelog
 * @param {string} version
 * @param {string} notes
 * @param {Date} date
 */
export function prependChangelog(changelog, version, notes, date) {
  const heading = "# Changelog\n";
  const body = changelog.startsWith(heading)
    ? changelog.slice(heading.length).trimStart()
    : changelog.trimStart();
  const day = date.toISOString().slice(0, 10);
  const section = `## ${version} (${day})\n\n${notes}\n`;
  return `${heading}\n${section}${body ? `\n${body}` : ""}`;
}
