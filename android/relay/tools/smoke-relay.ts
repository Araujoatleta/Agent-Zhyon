// Public-path smoke test for an installed VPS. It reads one enrolled desktop
// key from the container's read-only mount, never prints it, and proves the
// real TLS/WebSocket route can authenticate two peers and forward a frame.

import { randomBytes } from "node:crypto";
import { readFileSync } from "node:fs";
import { WebSocket } from "ws";
import { B64URL_32 } from "../src/protocol.ts";

const base = process.env.RELAY_URL?.replace(/\/$/, "") ?? "";
const keyFile = process.argv[2] ?? "";
if (!/^wss:\/\/[A-Za-z0-9.-]+(?::\d{1,5})?$/.test(base)) throw new Error("RELAY_URL must be a bare wss:// address");
const access = readFileSync(keyFile, "utf8").trim();
if (!B64URL_32.test(access)) throw new Error("the enrolled access key is invalid");

const b64url = (bytes: Buffer) => bytes.toString("base64url");
const room = b64url(randomBytes(16));
const proof = b64url(randomBytes(32));
const protocols = ["coucou.v1", `coucou.join.${proof}`];

function connect(role: "pc" | "phone", key: string): Promise<WebSocket | number> {
  return new Promise((resolve, reject) => {
    const ws = new WebSocket(`${base}/v1/room/${room}?role=${role}`, protocols, { headers: { Authorization: `Bearer ${key}` } });
    ws.once("open", () => resolve(ws));
    ws.once("unexpected-response", (_request, response) => {
      resolve(response.statusCode ?? 0);
      ws.terminate();
    });
    ws.once("error", reject);
  });
}

function message(ws: WebSocket, binary: boolean, timeout = 5_000): Promise<Buffer> {
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error("timed out waiting for relay data")), timeout);
    ws.on("message", function handler(data, isBinary) {
      if (isBinary !== binary) return;
      clearTimeout(timer);
      ws.off("message", handler);
      resolve(Buffer.isBuffer(data) ? data : Buffer.concat(data as Buffer[]));
    });
  });
}

const pc = await connect("pc", access);
if (!(pc instanceof WebSocket)) throw new Error(`enrolled key was refused (${pc})`);
const onlineAtPc = message(pc, false);
const phone = await connect("phone", access);
if (!(phone instanceof WebSocket)) throw new Error(`enrolled phone key was refused (${phone})`);
await onlineAtPc;
const forwarded = message(phone, true);
pc.send(Buffer.from([1, 2, 3, 4]));
if (!Buffer.from(await forwarded).equals(Buffer.from([1, 2, 3, 4]))) throw new Error("binary forwarding changed the frame");
pc.close();
phone.close();

const refused = await connect("pc", b64url(randomBytes(32)));
if (refused !== 401) {
  if (refused instanceof WebSocket) refused.close();
  throw new Error(`wrong key should return 401, got ${refused instanceof WebSocket ? "connected" : refused}`);
}
process.stdout.write("Public WSS authentication and binary forwarding are OK.\n");

