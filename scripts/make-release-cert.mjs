// Creates the self-signed "Tiny Wispr" code-signing certificate that every
// release is signed with: `node scripts/make-release-cert.mjs [dir]`.
//
// Run it once. macOS ties the Microphone, Accessibility and Input Monitoring
// grants to the certificate, so a release signed with a new one makes every
// user grant them again. Keep the .p12 and its password in a password
// manager; docs/operations/building-and-releasing.md says where they go.
import { execFileSync } from "node:child_process";
import { randomBytes } from "node:crypto";
import {
  chmodSync,
  existsSync,
  mkdirSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";

const IDENTITY = "Tiny Wispr";
const YEARS = 20;
const dir = process.argv[2] ?? join(homedir(), ".tiny-wispr-release-keys");
const p12 = join(dir, "release-certificate.p12");
const passwordFile = join(dir, "release-certificate.password");

if (existsSync(p12)) {
  console.error(`${p12} already exists. Releases must keep using it.`);
  process.exit(1);
}
mkdirSync(dir, { mode: 0o700, recursive: true });

const config = join(dir, "cert.cnf");
const key = join(dir, "key.pem");
const cert = join(dir, "cert.pem");
const password = randomBytes(24).toString("base64url");
try {
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
  const quiet = { stdio: ["ignore", "ignore", "inherit"] };
  execFileSync(
    "openssl",
    [
      "req",
      "-x509",
      "-newkey",
      "rsa:3072",
      "-nodes",
      "-days",
      String(YEARS * 365),
      "-config",
      config,
      "-keyout",
      key,
      "-out",
      cert,
    ],
    quiet
  );
  // 3DES and SHA-1 because `security import` can't read newer PKCS#12.
  execFileSync(
    "openssl",
    [
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
      "env:P12_PASSWORD",
      "-certpbe",
      "PBE-SHA1-3DES",
      "-keypbe",
      "PBE-SHA1-3DES",
      "-macalg",
      "sha1",
    ],
    { ...quiet, env: { ...process.env, P12_PASSWORD: password } }
  );
  writeFileSync(passwordFile, password);
  chmodSync(p12, 0o600);
  chmodSync(passwordFile, 0o600);
  const fingerprint = execFileSync(
    "openssl",
    ["x509", "-in", cert, "-noout", "-fingerprint", "-sha1"],
    { encoding: "utf8" }
  ).trim();
  console.log(`Created ${p12}\n${fingerprint}`);
} finally {
  for (const file of [config, key, cert]) {
    rmSync(file, { force: true });
  }
}
