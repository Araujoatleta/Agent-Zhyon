// Production entry point for the self-hosted Coucou relay.
//
// The forwarding rules stay in dev-relay.ts and protocol.ts so the Cloudflare
// and VPS builds run the same behaviour suite. This file only owns production
// configuration and graceful shutdown. It never prints request data or keys.

import { readFileSync } from "node:fs";
import { pathToFileURL } from "node:url";
import { B64URL_32 } from "../src/protocol.ts";
import { startRelay, type Relay } from "./dev-relay.ts";

export interface VpsEnvironment {
  ACCESS_KEY?: string;
  ACCESS_KEY_FILE?: string;
  HOST?: string;
  PORT?: string;
  CONNECTS_PER_MINUTE?: string;
}

export function readAccessKey(env: VpsEnvironment): string {
  const inline = env.ACCESS_KEY?.trim() ?? "";
  const file = env.ACCESS_KEY_FILE?.trim() ?? "";
  if (inline && file) throw new Error("set ACCESS_KEY or ACCESS_KEY_FILE, not both");
  const key = file ? readFileSync(file, "utf8").trim() : inline;
  if (!B64URL_32.test(key)) throw new Error("the relay access key must be 43 base64url characters");
  return key;
}

export function positiveInteger(value: string | undefined, fallback: number, name: string, maximum: number): number {
  if (value == null || value.trim() === "") return fallback;
  if (!/^\d+$/.test(value)) throw new Error(`${name} must be a positive integer`);
  const parsed = Number(value);
  if (!Number.isSafeInteger(parsed) || parsed < 1 || parsed > maximum) {
    throw new Error(`${name} must be between 1 and ${maximum}`);
  }
  return parsed;
}

export async function runVpsRelay(env: VpsEnvironment = process.env): Promise<Relay> {
  return startRelay({
    host: env.HOST?.trim() || "0.0.0.0",
    port: positiveInteger(env.PORT, 8787, "PORT", 65_535),
    accessKey: readAccessKey(env),
    connectsPerMinute: positiveInteger(env.CONNECTS_PER_MINUTE, 30, "CONNECTS_PER_MINUTE", 10_000),
  });
}

async function main() {
  const relay = await runVpsRelay();
  let stopping = false;
  const stop = async () => {
    if (stopping) return;
    stopping = true;
    await relay.close();
  };
  process.once("SIGTERM", () => void stop());
  process.once("SIGINT", () => void stop());
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  main().catch((error: unknown) => {
    const message = error instanceof Error ? error.message : "startup failed";
    process.stderr.write(`Coucou relay: ${message}\n`);
    process.exitCode = 1;
  });
}
