import { mkdirSync, mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { describe, expect, it } from "vitest";

import {
  compareVersions,
  isPrerelease,
  prependChangelog,
  readVersions,
  releaseNotes,
  updatePlatforms,
  updaterManifest,
  versionProblems,
  writeVersions,
} from "./release.mjs";

function fixture(version = "0.1.0") {
  const root = mkdtempSync(join(tmpdir(), "tw-release-"));
  mkdirSync(join(root, "src-tauri"));
  writeFileSync(
    join(root, "package.json"),
    `{\n  "name": "tiny-wispr",\n  "version": "${version}",\n  "private": true\n}\n`
  );
  writeFileSync(
    join(root, "src-tauri/tauri.conf.json"),
    `{\n  "productName": "Tiny Wispr",\n  "version": "${version}",\n  "identifier": "x"\n}\n`
  );
  writeFileSync(
    join(root, "Cargo.toml"),
    `[workspace]\nmembers = []\n\n[workspace.package]\nversion = "${version}"\nedition = "2024"\n\n[workspace.dependencies]\nserde = { version = "1" }\n`
  );
  return root;
}

describe("versions", () => {
  it("orders prereleases before their release", () => {
    expect(compareVersions("0.2.0-rc.1", "0.2.0")).toBeLessThan(0);
    expect(compareVersions("0.2.0-rc.2", "0.2.0-rc.10")).toBeLessThan(0);
    expect(compareVersions("0.2.1", "0.2.0")).toBeGreaterThan(0);
    expect(compareVersions("1.0.0", "1.0.0")).toBe(0);
    expect(isPrerelease("0.2.0-rc.1")).toBe(true);
    expect(isPrerelease("0.2.0")).toBe(false);
  });

  it("rejects what isn't a version", () => {
    expect(() => compareVersions("v1.0", "1.0.0")).toThrow();
  });

  it("reads and writes all three files", () => {
    const root = fixture();
    expect(versionProblems(readVersions(root), "0.1.0")).toEqual([]);
    writeVersions(root, "0.2.0-rc.1");
    expect(readVersions(root)).toEqual({
      "Cargo.toml": "0.2.0-rc.1",
      "package.json": "0.2.0-rc.1",
      "src-tauri/tauri.conf.json": "0.2.0-rc.1",
    });
    const cargo = readFileSync(join(root, "Cargo.toml"), "utf8");
    expect(cargo).toContain('serde = { version = "1" }');
  });

  it("names the file that disagrees with the tag", () => {
    const problems = versionProblems(
      {
        "Cargo.toml": "0.1.0",
        "package.json": "0.1.0",
        "src-tauri/tauri.conf.json": "0.1.0",
      },
      "0.2.0"
    );
    expect(problems).toContain("package.json is 0.1.0, the tag says 0.2.0");
  });
});

describe("release notes", () => {
  it("lists features, fixes and performance work only", () => {
    expect(
      releaseNotes([
        "chore(release): v0.1.0",
        "fix: keep the pill above full-screen apps",
        "feat: check for updates from settings",
        "docs: describe the updater",
        "perf(stt): load the voice model once",
        "feat!: drop the old settings file",
      ])
    ).toBe(
      [
        "### Features",
        "",
        "- Check for updates from settings",
        "- Drop the old settings file (breaking)",
        "",
        "### Fixes",
        "",
        "- Keep the pill above full-screen apps",
        "",
        "### Performance",
        "",
        "- Load the voice model once",
      ].join("\n")
    );
  });

  it("says something when nothing user-facing changed", () => {
    expect(releaseNotes(["chore: bump deps"])).toBe(
      "Small changes behind the scenes."
    );
  });

  it("prepends a dated section to the changelog", () => {
    const date = new Date("2026-11-22T10:00:00Z");
    const first = prependChangelog("", "0.1.0", "- A", date);
    expect(first).toBe("# Changelog\n\n## 0.1.0 (2026-11-22)\n\n- A\n");
    const second = prependChangelog(first, "0.2.0", "- B", date);
    expect(second).toBe(
      "# Changelog\n\n## 0.2.0 (2026-11-22)\n\n- B\n\n## 0.1.0 (2026-11-22)\n\n- A\n"
    );
  });
});

describe("updater manifest", () => {
  it("has the fields the updater reads", () => {
    const manifest = updaterManifest({
      notes: "- A",
      platforms: {
        "darwin-aarch64": {
          signature: "c2ln",
          url: "https://github.com/o/r/releases/download/v0.2.0/a.tar.gz",
        },
      },
      pubDate: new Date("2026-11-22T10:00:00Z"),
      version: "0.2.0",
    });
    expect(manifest).toEqual({
      notes: "- A",
      platforms: {
        "darwin-aarch64": {
          signature: "c2ln",
          url: "https://github.com/o/r/releases/download/v0.2.0/a.tar.gz",
        },
      },
      pub_date: "2026-11-22T10:00:00.000Z",
      version: "0.2.0",
    });
  });

  it("refuses insecure or unsigned downloads", () => {
    const base = { notes: "", pubDate: new Date(), version: "0.2.0" };
    expect(() =>
      updaterManifest({
        ...base,
        platforms: { "darwin-aarch64": { signature: "x", url: "http://a" } },
      })
    ).toThrow("https");
    expect(() =>
      updaterManifest({
        ...base,
        platforms: { "darwin-aarch64": { signature: " ", url: "https://a" } },
      })
    ).toThrow("signature");
  });
});

describe("update platforms", () => {
  it("maps each signed file to the platforms that install it", () => {
    expect(
      updatePlatforms([
        "SHA256SUMS",
        "Tiny-Wispr_0.2.0_aarch64.dmg",
        "Tiny-Wispr_0.2.0_aarch64.app.tar.gz",
        "Tiny-Wispr_0.2.0_aarch64.app.tar.gz.sig",
        "Tiny-Wispr_0.2.0_x64-setup.exe",
        "Tiny-Wispr_0.2.0_x64-setup.exe.sig",
        "Tiny-Wispr_0.2.0_amd64.AppImage",
        "Tiny-Wispr_0.2.0_amd64.AppImage.sig",
        "Tiny-Wispr_0.2.0_amd64.deb",
        "Tiny-Wispr_0.2.0_amd64.deb.sig",
      ])
    ).toEqual({
      "darwin-aarch64": "Tiny-Wispr_0.2.0_aarch64.app.tar.gz",
      "darwin-aarch64-app": "Tiny-Wispr_0.2.0_aarch64.app.tar.gz",
      "linux-x86_64": "Tiny-Wispr_0.2.0_amd64.AppImage",
      "linux-x86_64-appimage": "Tiny-Wispr_0.2.0_amd64.AppImage",
      "linux-x86_64-deb": "Tiny-Wispr_0.2.0_amd64.deb",
      "windows-x86_64": "Tiny-Wispr_0.2.0_x64-setup.exe",
      "windows-x86_64-nsis": "Tiny-Wispr_0.2.0_x64-setup.exe",
    });
  });

  it("refuses a signed file no platform uses", () => {
    expect(() => updatePlatforms(["a.zip", "a.zip.sig"])).toThrow("a.zip");
  });
});
