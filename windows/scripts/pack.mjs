// Copies Tauri bundles and the portable executables into windows/release/.
// It also writes and verifies SHA-256 checksums for every shipped file.

import { createHash } from "node:crypto";
import {
  copyFileSync,
  existsSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  statSync,
  writeFileSync,
} from "node:fs";
import { basename, dirname, isAbsolute, join, relative, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const bundleRoot = join(root, "target", "release", "bundle");
const outDir = join(root, "release");
const manifestPath = join(outDir, "SHA256SUMS.txt");
const { version } = JSON.parse(readFileSync(join(root, "src-tauri", "tauri.conf.json"), "utf8"));

const arch = process.arch === "arm64" ? "aarch64" : "x86_64";
const debArch = process.arch === "arm64" ? "arm64" : "amd64";
const PACKAGES = {
  win32: [
    {
      dir: "nsis",
      suffix: "-setup.exe",
      names: [`Coucou-Windows-${version}-setup.exe`, "Coucou-Windows-setup.exe"],
    },
    {
      dir: "msi",
      suffix: ".msi",
      names: [`Coucou-Windows-${version}.msi`, "Coucou-Windows.msi"],
    },
  ],
  linux: [
    {
      dir: "appimage",
      suffix: ".AppImage",
      names: [`Coucou-Linux-${version}-${arch}.AppImage`, `Coucou-Linux-${arch}.AppImage`],
    },
    { dir: "deb", suffix: ".deb", names: [`Coucou-Linux-${version}-${debArch}.deb`] },
    { dir: "rpm", suffix: ".rpm", names: [`Coucou-Linux-${version}-${arch}.rpm`] },
  ],
};

export function sha256(path) {
  return createHash("sha256").update(readFileSync(path)).digest("hex");
}

export function writeChecksumManifest(files, directory) {
  const base = resolve(directory);
  const entries = files
    .map((file) => {
      const absolute = resolve(file);
      const name = relative(base, absolute).split(sep).join("/");
      if (!name || name === ".." || name.startsWith("../") || isAbsolute(name)) {
        throw new Error(`artifact is outside checksum directory: ${file}`);
      }
      return { absolute, name };
    })
    .sort((a, b) => (a.name < b.name ? -1 : a.name > b.name ? 1 : 0));
  const path = join(base, "SHA256SUMS.txt");
  writeFileSync(path, entries.map(({ absolute, name }) => `${sha256(absolute)}  ${name}\n`).join(""));
  return path;
}

export function verifyChecksumManifest(path) {
  const directory = dirname(resolve(path));
  const lines = readFileSync(path, "utf8").split(/\r?\n/).filter(Boolean);
  if (lines.length === 0) throw new Error("checksum manifest is empty");
  const verified = [];
  for (const line of lines) {
    const match = line.match(/^([0-9a-f]{64})  (.+)$/);
    if (!match) throw new Error(`invalid checksum line: ${line}`);
    const [, expected, name] = match;
    const artifact = resolve(directory, name);
    const rel = relative(directory, artifact);
    if (!rel || rel === ".." || rel.startsWith(`..${sep}`) || isAbsolute(rel)) {
      throw new Error(`checksum path escapes release directory: ${name}`);
    }
    if (!existsSync(artifact)) throw new Error(`checksum artifact is missing: ${name}`);
    const actual = sha256(artifact);
    if (actual !== expected) throw new Error(`checksum mismatch for ${name}: expected ${expected}, got ${actual}`);
    verified.push(artifact);
  }
  return verified;
}

function newest(dir, suffix) {
  let files = [];
  try {
    files = readdirSync(dir).filter((file) => file.endsWith(suffix));
  } catch {
    return null;
  }
  if (files.length === 0) return null;
  return files
    .map((file) => join(dir, file))
    .sort((a, b) => statSync(b).mtimeMs - statSync(a).mtimeMs)[0];
}

function copy(source, destination, written) {
  if (!existsSync(source)) throw new Error(`Missing build output: ${source}`);
  mkdirSync(dirname(destination), { recursive: true });
  copyFileSync(source, destination);
  written.push(destination);
}

function pack() {
  const packages = PACKAGES[process.platform];
  if (!packages) throw new Error(`Nothing to pack on ${process.platform}.`);
  mkdirSync(outDir, { recursive: true });
  const written = [];

  for (const { dir, suffix, names } of packages) {
    const built = newest(join(bundleRoot, dir), suffix);
    if (!built) throw new Error(`No *${suffix} in ${join(bundleRoot, dir)} — run \`npm run tauri build\` first.`);
    for (const name of names) copy(built, join(outDir, name), written);
  }

  if (process.platform === "win32") {
    const portable = join(outDir, `Coucou-Windows-${version}-portable`);
    for (const name of ["coucou.exe", "coucou-hook.exe", "coucou-edge-worker.exe"]) {
      const destination = join(portable, name === "coucou.exe" ? "Coucou.exe" : basename(name));
      copy(join(root, "target", "release", name), destination, written);
    }
  }

  const checksum = writeChecksumManifest(written, outDir);
  const verified = verifyChecksumManifest(checksum);
  console.log("\n  Packages ready (SHA-256 verified)\n");
  for (const file of [...written, checksum]) {
    const size = statSync(file).size;
    console.log(`  ${file}  (${(size / 1024 / 1024).toFixed(2)} MB)`);
  }
  console.log(`\n  Verified ${verified.length} artifacts.\n`);
}

const isMain = process.argv[1] && resolve(process.argv[1]) === resolve(fileURLToPath(import.meta.url));
if (isMain) {
  try {
    if (process.argv.includes("--verify")) {
      const verified = verifyChecksumManifest(manifestPath);
      console.log(`Verified ${verified.length} artifacts from ${manifestPath}.`);
    } else {
      pack();
    }
  } catch (error) {
    console.error(error instanceof Error ? error.message : error);
    process.exitCode = 1;
  }
}
