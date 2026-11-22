// Builds the debug app bundle and opens it, signed with a local self-signed
// certificate. An ad hoc signature changes with every build, and macOS ties
// Microphone, Accessibility and Input Monitoring grants to the signature, so
// each rebuild would lose them. A certificate that stays the same keeps them.
import { execFileSync } from "node:child_process";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const IDENTITY = "Tiny Wispr Development";
const BUNDLE_ID = "dev.time2code.tinywispr";
const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const keychain = join(
  process.env.HOME ?? "",
  "Library/Keychains/login.keychain-db"
);

const run = (command, args, options = {}) =>
  execFileSync(command, args, { encoding: "utf8", ...options });

const hasIdentity = () =>
  // Without -v, so the untrusted self-signed identity is listed too.
  run("security", ["find-identity", "-p", "codesigning", keychain]).includes(
    `"${IDENTITY}"`
  );

function createIdentity() {
  console.log(
    `Creating the "${IDENTITY}" signing certificate in your login keychain`
  );
  const dir = mkdtempSync(join(tmpdir(), "tiny-wispr-cert-"));
  try {
    const config = join(dir, "cert.cnf");
    writeFileSync(
      config,
      [
        "[req]",
        "distinguished_name = dn",
        "x509_extensions = ext",
        "prompt = no",
        "[dn]",
        `CN = ${IDENTITY}`,
        "[ext]",
        "basicConstraints = critical, CA:false",
        "keyUsage = critical, digitalSignature",
        "extendedKeyUsage = critical, codeSigning",
        "",
      ].join("\n")
    );
    const key = join(dir, "key.pem");
    const cert = join(dir, "cert.pem");
    const p12 = join(dir, "identity.p12");
    // The password only protects the file for the moment it exists.
    const password = "tiny-wispr";
    run("openssl", [
      "req",
      "-x509",
      "-newkey",
      "rsa:2048",
      "-nodes",
      "-days",
      "3650",
      "-config",
      config,
      "-keyout",
      key,
      "-out",
      cert,
    ]);
    // 3DES and SHA-1 because `security import` can't read newer PKCS#12.
    run("openssl", [
      "pkcs12",
      "-export",
      "-inkey",
      key,
      "-in",
      cert,
      "-out",
      p12,
      "-name",
      IDENTITY,
      "-passout",
      `pass:${password}`,
      "-certpbe",
      "PBE-SHA1-3DES",
      "-keypbe",
      "PBE-SHA1-3DES",
      "-macalg",
      "sha1",
    ]);
    run("security", [
      "import",
      p12,
      "-k",
      keychain,
      "-P",
      password,
      "-T",
      "/usr/bin/codesign",
    ]);
  } finally {
    rmSync(dir, { force: true, recursive: true });
  }
  // Grants given to earlier ad hoc builds can't match the new signature.
  for (const service of ["Microphone", "Accessibility", "ListenEvent"]) {
    try {
      run("tccutil", ["reset", service, BUNDLE_ID]);
    } catch {
      // Nothing was granted yet.
    }
  }
}

if (!hasIdentity()) {
  createIdentity();
}

execFileSync("pnpm", ["tauri", "build", "--debug", "--bundles", "app"], {
  cwd: root,
  env: { ...process.env, APPLE_SIGNING_IDENTITY: IDENTITY },
  stdio: "inherit",
});

if (!process.argv.includes("--no-open")) {
  execFileSync("open", [
    join(root, "target/debug/bundle/macos/Tiny Wispr.app"),
  ]);
}
