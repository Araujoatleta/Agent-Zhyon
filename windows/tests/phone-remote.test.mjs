// Settings → Phone → Away from home, directly: what is said about the router's answer (src/settings/phone.ts,
// src-tauri/src/phone_link/remote.rs).

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { installFakeDom } from "./fakedom.mjs";

installFakeDom();
const { remoteLines, tailnetLine } = await import("../src/settings/phone.ts");

const base = {
  enabled: true,
  linkOn: true,
  checking: false,
  router: "Archer C6",
  v4: "198.51.100.7:47821",
  v4Problem: null,
  v6: ["[2804:14c:1:2::10]:47821"],
  v6Note: "The router lets these addresses in.",
  v6Confirmed: true,
  v6Blocked: false,
  v6Error: null,
  lastOutside: 0,
};

test("switched off it says nothing, and while the router has not answered it says so", () => {
  assert.deepEqual(remoteLines({ ...base, enabled: false }), []);
  assert.deepEqual(remoteLines({ ...base, checking: true }), ["Asking your router…"]);
});

test("every way in is listed, IPv4 and IPv6, with what the router said", () => {
  const lines = remoteLines(base);
  assert.ok(lines.includes("Router: Archer C6"));
  assert.ok(lines.includes("Over IPv4: 198.51.100.7:47821"));
  assert.ok(lines.includes("Over IPv6: [2804:14c:1:2::10]:47821"));
  assert.ok(lines.includes("The router lets these addresses in."));
  assert.ok(lines.some((l) => l.startsWith("Your iPhone learns these addresses")), "how the phone gets them");
  assert.ok(lines.includes("No phone has connected from outside yet."));
});

test("what cannot work is said plainly, never as if it worked", () => {
  const lines = remoteLines({ ...base, v4: null, v4Problem: "CGNAT here.", v6: [], v6Note: null });
  assert.ok(lines.includes("Over IPv4: no. CGNAT here."));
  assert.ok(lines.includes("Over IPv6: no. This computer has no internet IPv6 address."));
  assert.ok(lines.includes("Not reachable from outside for now: see what your router answered above."));
  assert.ok(!lines.some((l) => l.startsWith("Your iPhone learns")));
  const closed = remoteLines({ ...base, v4: null, v4Problem: "CGNAT here.", v6Blocked: true, v6Confirmed: false, v6Note: "Closed." });
  assert.ok(closed.includes("Not reachable from outside for now: see what your router answered above."), "a closed IPv6 is no way in");
  const v6Broken = remoteLines({ ...base, v6: [], v6Error: "address in use" });
  assert.ok(v6Broken.includes("Over IPv6: no. address in use"));
});

test("a connection from outside is shown with its time", () => {
  const lines = remoteLines({ ...base, lastOutside: Date.UTC(2026, 9, 13, 18, 30) });
  assert.ok(lines.some((l) => l.startsWith("Last connection from outside: ")));
});

test("only the settings window asks for the home's addresses, and they never reach the log", () => {
  const mod = readFileSync(new URL("../src-tauri/src/phone_link/mod.rs", import.meta.url), "utf8");
  const status = mod.slice(mod.indexOf("pub fn phone_remote_status"), mod.indexOf("pub fn phone_remote_set"));
  assert.match(status, /only_settings\(&window\)\?/);
  const sync = mod.slice(mod.indexOf("fn sync_remote"), mod.indexOf("fn stop_remote_now"));
  for (const line of sync.split("\n").filter((l) => l.includes("log::line"))) {
    assert.ok(!/endpoints\)|reach\.v4|reach\.v6|\{endpoints/.test(line), `no address in: ${line.trim()}`);
  }
  const remote = readFileSync(new URL("../src-tauri/src/phone_link/remote.rs", import.meta.url), "utf8");
  assert.ok(!/ipify|ifconfig\.me|stun:|icanhazip|cloudflare|dyndns|noip/i.test(remote), "nobody but the router is asked");
});

test("Settings says how the iPhone comes through Tailscale, or how to set it up", () => {
  const link = { enabled: true, running: true, port: 47821, host: "192.168.0.10", name: "PC", clients: 1, error: null, relayState: "off", tailnet: [] };
  assert.match(tailnetLine(link), /install Tailscale \(free\) on this computer and on the iPhone/);
  assert.match(tailnetLine({ ...link, tailnet: ["100.101.102.103"] }), /Tailscale is on here \(100\.101\.102\.103\)/);
});
