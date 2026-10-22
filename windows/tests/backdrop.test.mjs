// The island's see-through glass (src/island/backdrop.ts): what the screen behind gave, how light it is under the
// island, and whether it changed enough to draw it again.

import { test } from "node:test";
import assert from "node:assert/strict";
import { differs, lightness, lightUnder, readBehind } from "../src/island/backdrop.ts";

/** A picture of the island's 720×320 window, as island_backdrop sends it, every pixel `rgb`. */
function picture(w, h, rgb) {
  const out = new ArrayBuffer(8 + w * h * 4);
  const head = new DataView(out, 0, 8);
  head.setUint32(0, w, true);
  head.setUint32(4, h, true);
  const px = new Uint8Array(out, 8);
  for (let i = 0; i < w * h; i++) px.set([...rgb, 255], i * 4);
  return out;
}

test("nothing means never, a size of 0 means not now", () => {
  assert.equal(readBehind(new ArrayBuffer(0)).kind, "never");
  assert.equal(readBehind(new ArrayBuffer(8)).kind, "not-now");
  // An error on the way (the bridge gives null) is tried again, not given up on.
  assert.equal(readBehind(null).kind, "not-now");
});

test("a picture of the panel is read, a cut one or the wake strip is not", () => {
  const read = readBehind(picture(1080, 480, [10, 20, 30]));
  assert.equal(read.kind, "picture");
  assert.equal(read.w, 1080);
  assert.equal(read.h, 480);
  assert.deepEqual([...read.pixels.subarray(0, 4)], [10, 20, 30, 255]);
  assert.equal(readBehind(picture(1080, 480, [0, 0, 0]).slice(0, 100)).kind, "not-now");
  assert.equal(readBehind(picture(360, 9, [0, 0, 0])).kind, "not-now");
});

test("how light it is behind the island, whatever the screen's scale", () => {
  const white = readBehind(picture(1080, 480, [255, 255, 255]));
  const black = readBehind(picture(720, 320, [0, 0, 0]));
  const light = lightness(white.w, white.h, white.pixels);
  const dark = lightness(black.w, black.h, black.pixels);
  assert.ok(Math.abs(lightUnder(light, 40, 0, 640, 300) - 1) < 1e-6);
  assert.equal(lightUnder(dark, 216, 0, 288, 32), 0);
});

test("only a real change draws the picture again", () => {
  const a = readBehind(picture(720, 320, [200, 200, 200]));
  const b = readBehind(picture(720, 320, [201, 200, 199]));
  const c = readBehind(picture(720, 320, [40, 40, 40]));
  const la = lightness(a.w, a.h, a.pixels);
  assert.equal(differs(null, la), true);
  assert.equal(differs(la, lightness(b.w, b.h, b.pixels)), false);
  assert.equal(differs(la, lightness(c.w, c.h, c.pixels)), true);
});
