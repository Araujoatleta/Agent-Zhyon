import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, describe, expect, it } from "vitest";
import { positiveInteger, readAccessKey, readAccessKeys } from "../tools/vps-relay.ts";

const KEY = "A".repeat(43);
const temporary: string[] = [];

afterEach(() => {
  for (const path of temporary.splice(0)) rmSync(path, { recursive: true, force: true });
});

describe("VPS configuration", () => {
  it("reads the relay key from a Docker secret without exposing it in config", () => {
    const dir = mkdtempSync(join(tmpdir(), "coucou-relay-"));
    temporary.push(dir);
    const file = join(dir, "access-key");
    writeFileSync(file, `${KEY}\n`, { mode: 0o600 });
    expect(readAccessKey({ ACCESS_KEY_FILE: file })).toBe(KEY);
  });

  it("fails closed for missing, ambiguous or malformed keys", () => {
    expect(() => readAccessKey({})).toThrow(/43 base64url/);
    expect(() => readAccessKey({ ACCESS_KEY: KEY, ACCESS_KEY_FILE: "secret" })).toThrow(/not both/);
    expect(() => readAccessKey({ ACCESS_KEY: "too-short" })).toThrow(/43 base64url/);
  });

  it("reads one revocable key per desktop in stable order", () => {
    const dir = mkdtempSync(join(tmpdir(), "coucou-clients-"));
    temporary.push(dir);
    writeFileSync(join(dir, "zeta.key"), `${"Z".repeat(43)}\n`, { mode: 0o400 });
    writeFileSync(join(dir, "alpha.key"), `${"A".repeat(43)}\n`, { mode: 0o400 });
    writeFileSync(join(dir, "README.txt"), "ignored");
    expect(readAccessKeys({ ACCESS_KEYS_DIR: dir })).toEqual(["A".repeat(43), "Z".repeat(43)]);
  });

  it("rejects empty, duplicate and mixed multi-key configuration", () => {
    const empty = mkdtempSync(join(tmpdir(), "coucou-empty-"));
    temporary.push(empty);
    expect(() => readAccessKeys({ ACCESS_KEYS_DIR: empty })).toThrow(/no valid/);
    const duplicate = mkdtempSync(join(tmpdir(), "coucou-duplicate-"));
    temporary.push(duplicate);
    writeFileSync(join(duplicate, "one.key"), KEY);
    writeFileSync(join(duplicate, "two.key"), KEY);
    expect(() => readAccessKeys({ ACCESS_KEYS_DIR: duplicate })).toThrow(/duplicate/);
    expect(() => readAccessKeys({ ACCESS_KEYS_DIR: duplicate, ACCESS_KEY: KEY })).toThrow(/not both/);
  });

  it("bounds numeric environment settings", () => {
    expect(positiveInteger(undefined, 30, "RATE", 100)).toBe(30);
    expect(positiveInteger("42", 30, "RATE", 100)).toBe(42);
    expect(() => positiveInteger("0", 30, "RATE", 100)).toThrow(/between/);
    expect(() => positiveInteger("101", 30, "RATE", 100)).toThrow(/between/);
  });

  it("keeps the container private and the key out of compose", () => {
    const compose = readFileSync(new URL("../compose.vps.yml", import.meta.url), "utf8");
    const dockerfile = readFileSync(new URL("../Dockerfile", import.meta.url), "utf8");
    expect(compose).toContain('"127.0.0.1:${COUCO_RELAY_PORT:-8787}:8787"');
    expect(compose).toMatch(/read_only:\s*true/);
    expect(compose).toMatch(/cap_drop:\s*\n\s*- ALL/);
    expect(compose).toContain("ACCESS_KEYS_DIR");
    expect(compose).toContain("./state/clients:/run/coucou/clients:ro");
    expect(compose).not.toMatch(/ACCESS_KEY:\s*[A-Za-z0-9_-]{16}/);
    expect(dockerfile).toContain("USER node");
  });

  it("keeps portable TLS and all mutable state inside the relay folder", () => {
    const compose = readFileSync(new URL("../compose.portable.yml", import.meta.url), "utf8");
    const caddy = readFileSync(new URL("../deploy/Caddyfile", import.meta.url), "utf8");
    expect(compose).toContain('"80:80"');
    expect(compose).toContain('"443:443"');
    expect(compose).toContain("./state/caddy/data:/data");
    expect(compose).toContain("./state/clients:/run/coucou/clients:ro");
    expect(caddy).toContain("reverse_proxy coucou-relay:8787");
    expect(caddy).toContain("output discard");
  });

  it("ships one-click enrollment without embedding a repository secret", () => {
    const installer = readFileSync(new URL("../deploy/windows/Install-CoucouManaged.ps1", import.meta.url), "utf8");
    const control = readFileSync(new URL("../vpsctl", import.meta.url), "utf8");
    expect(installer).toContain("phone-relay-access.fr.louisraille.coucou");
    expect(installer).toContain("phoneRelayUrl");
    expect(installer).toContain("Marshal.ZeroFreeCoTaskMemUnicode");
    expect(installer).not.toMatch(/"accessKey"\s*:\s*"[A-Za-z0-9_-]{43}"/);
    expect(control).toContain("client-package");
    expect(control).toContain("revoke)");
    expect(control).toContain("backup)");
  });
});
