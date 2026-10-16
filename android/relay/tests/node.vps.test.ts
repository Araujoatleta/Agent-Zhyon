import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, describe, expect, it } from "vitest";
import { positiveInteger, readAccessKey } from "../tools/vps-relay.ts";

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
    expect(compose).toContain("ACCESS_KEY_FILE");
    expect(compose).not.toMatch(/ACCESS_KEY:\s*[A-Za-z0-9_-]{16}/);
    expect(dockerfile).toContain("USER node");
  });
});
