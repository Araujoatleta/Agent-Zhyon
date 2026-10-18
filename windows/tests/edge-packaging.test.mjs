import { test } from "node:test";
import assert from "node:assert/strict";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const read = (path) => readFileSync(new URL(`../${path}`, import.meta.url), "utf8").replace(/\r\n/g, "\n");

test("prebuild compiles the relay and edge worker from the lockfile", () => {
  const pkg = JSON.parse(read("package.json"));
  assert.match(pkg.scripts.prebuild, /cargo build --release --locked/);
  assert.match(pkg.scripts.prebuild, /-p coucou-hook/);
  assert.match(pkg.scripts.prebuild, /-p coucou-edge-worker/);
  assert.equal(pkg.scripts.prepack, pkg.scripts.prebuild);
  assert.equal(pkg.scripts["verify:artifacts"], "node scripts/pack.mjs --verify");
});

test("Tauri bundles both native executables as installer resources", () => {
  const resources = JSON.parse(read("src-tauri/tauri.conf.json")).bundle.resources;
  assert.equal(resources["../target/release/coucou-hook.exe"], "coucou-hook.exe");
  assert.equal(resources["../target/release/coucou-edge-worker.exe"], "coucou-edge-worker.exe");
});

test("NSIS derives a real device name and starts the worker without manual PowerShell", () => {
  const hooks = read("src-tauri/nsis/hooks.nsh");
  assert.match(hooks, /ReadRegStr \$CoucouVpsUrl HKCU "Environment" "COUCOU_VPS_URL"/);
  assert.match(hooks, /ReadRegStr \$CoucouDeviceName HKCU "Environment" "COUCOU_DEVICE_NAME"/);
  assert.match(hooks, /ReadEnvStr \$CoucouDeviceName "COMPUTERNAME"/);
  assert.doesNotMatch(hooks, /StrCpy \$CoucouDeviceName "\$COMPUTERNAME"/);
  assert.match(hooks, /WriteRegExpandStr HKCU "Environment" "COUCOU_VPS_URL"/);
  assert.match(hooks, /WriteRegExpandStr HKCU "Environment" "COUCOU_DEVICE_NAME"/);
  assert.doesNotMatch(hooks, /schtasks\.exe.*\/Create/s);
  assert.doesNotMatch(hooks, /schtasks\.exe.*\/Run/s);
  assert.match(hooks, /WriteRegStr HKCU "Software\\Microsoft\\Windows\\CurrentVersion\\Run" "Coucou Edge Worker"/);
  assert.match(hooks, /Exec '\"\$INSTDIR\\coucou-edge-worker\.exe\"'/);
  assert.match(hooks, /NSIS_HOOK_PREUNINSTALL[\s\S]*schtasks\.exe.*\/Delete.*\/TN.*Coucou Edge Worker/s);
  assert.match(hooks, /NSIS_HOOK_PREUNINSTALL[\s\S]*DeleteRegValue HKCU "Software\\Microsoft\\Windows\\CurrentVersion\\Run" "Coucou Edge Worker"/s);
  assert.doesNotMatch(hooks, /\/RU SYSTEM|Session 0|New-Service|sc\.exe create/i);
});

test("installer validates edge URL, keeps secrets out of files, and uninstall removes edge state", () => {
  const hooks = read("src-tauri/nsis/hooks.nsh");
  assert.match(hooks, /StrCpy \$CoucouVpsUrl "https:\/\/coucou-relay\.navellux\.com\.br\/control"/);
  assert.match(hooks, /coucou-edge-worker\.exe" --validate-config/);
  assert.match(hooks, /ExecWait '\"\$INSTDIR\\coucou-edge-worker\.exe\" --remove-identity(?:[^']*)'/);
  assert.match(hooks, /schtasks\.exe.*\/End.*\/TN.*Coucou Edge Worker/s);
  assert.match(hooks, /NSIS_HOOK_PREINSTALL[\s\S]*DeleteRegValue HKCU .*CurrentVersion\\Run[\s\S]*taskkill\.exe.*\/IM "coucou-edge-worker\.exe".*\/F[\s\S]*!macroend/is);
  assert.match(hooks, /NSIS_HOOK_PREUNINSTALL[\s\S]*DeleteRegValue HKCU .*CurrentVersion\\Run[\s\S]*taskkill\.exe.*\/IM "coucou-edge-worker\.exe".*\/F[\s\S]*--remove-identity/is);
  assert.match(hooks, /DeleteRegValue HKCU "Environment" "COUCOU_VPS_URL"/);
  assert.match(hooks, /DeleteRegValue HKCU "Environment" "COUCOU_DEVICE_NAME"/);
  assert.match(hooks, /RMDir \/r "\$LOCALAPPDATA\\Coucou\\edge-worker"/);
  for (const line of hooks.split("\n").filter((line) => /FileWrite/i.test(line))) {
    assert.doesNotMatch(line, /VPS|DEVICE|credential|identity/i);
  }
});

test("checksum manifest generation and verification detect tampering", async () => {
  const { writeChecksumManifest, verifyChecksumManifest } = await import("../scripts/pack.mjs");
  const dir = mkdtempSync(join(tmpdir(), "coucou-pack-"));
  try {
    const first = join(dir, "Coucou.exe");
    const second = join(dir, "coucou-edge-worker.exe");
    writeFileSync(first, "app");
    writeFileSync(second, "worker");
    const manifest = writeChecksumManifest([first, second], dir);
    assert.equal(verifyChecksumManifest(manifest).length, 2);
    assert.match(readFileSync(manifest, "utf8"), /^[0-9a-f]{64}  Coucou\.exe\n[0-9a-f]{64}  coucou-edge-worker\.exe\n$/);
    writeFileSync(second, "tampered");
    assert.throws(() => verifyChecksumManifest(manifest), /checksum mismatch.*coucou-edge-worker\.exe/i);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test("portable packaging names the app, relay, worker, and checksum manifest", () => {
  const source = read("scripts/pack.mjs");
  for (const name of ["coucou.exe", "coucou-hook.exe", "coucou-edge-worker.exe", "SHA256SUMS.txt"]) {
    assert.ok(source.includes(name), `pack.mjs includes ${name}`);
  }
});

test("VPS control CI installs, checks, tests, audits, and builds its container", () => {
  const workflow = read("../.github/workflows/vps-control.yml");
  for (const command of ["npm ci", "npm run check", "npm test", "npm audit --audit-level=high", "docker build"]) {
    assert.ok(workflow.includes(command), `vps-control workflow runs ${command}`);
  }
  assert.match(workflow, /working-directory: vps-control/);
});

test("Windows CI has explicit locked worker quality gates", () => {
  const workflow = read("../.github/workflows/windows-ci.yml");
  assert.match(workflow, /cargo fmt --package coucou-edge-worker -- --check/);
  assert.match(workflow, /cargo test --locked -p coucou-edge-worker/);
  assert.match(workflow, /cargo build --release --locked -p coucou-edge-worker/);
});

test("Windows artifacts upload the worker and generated checksums", () => {
  for (const path of ["../.github/workflows/windows-installer.yml", "../.github/workflows/windows.yml"]) {
    const workflow = read(path);
    assert.ok(workflow.includes("coucou-edge-worker.exe"), `${path} uploads worker`);
    assert.ok(workflow.includes("SHA256SUMS.txt"), `${path} uploads checksums`);
    assert.ok(workflow.includes("npm run verify:artifacts"), `${path} verifies checksums`);
  }
});

test("Portuguese operations guide matches automatic installed startup and PIN approval", () => {
  const guide = read("../docs/VPS_EDGE_CONTROL_PT.md");
  assert.match(guide, /HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Run/);
  assert.match(guide, /inicia.*automaticamente.*PIN/is);
  assert.match(guide, /\/v1\/pair\/start/);
  assert.match(guide, /\/v1\/admin\/pairings\/approve/);
  assert.doesNotMatch(guide, /Lacuna operacional atual/);
  assert.match(guide, /SHA256SUMS\.txt/);
});
