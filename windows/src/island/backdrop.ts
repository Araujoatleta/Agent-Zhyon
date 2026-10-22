// What is behind the island, for its liquid glass (island_backdrop in src-tauri/src/lib.rs).
//
// The island's window can't see the desktop under it, so on Windows Coucou reads that piece of the screen (the
// island's own window left out of the picture) and draws it, blurred, on a canvas behind the island's glass: the
// glass bends it at its rim (core/liquid.ts), and the island reads as a drop of glass over whatever is under it.
// Read when the island shows, then every few seconds while it stays on screen — less often while nothing behind it
// changes; never while it is hidden or in the classic look. The picture is only drawn here, never kept or sent
// anywhere. Where it can never be read (Linux), nothing is asked again and the island keeps its own dark water.

import { Bridge, IS_TAURI } from "../core/bridge";
import { PANEL_H, PANEL_W } from "../core/layout";

/** How often what is behind is read again while it changes, and at most while it doesn't, ms. */
const EVERY = 2500;
const MOST = 8000;
/** How soon a reading that gave nothing for now is tried again, at first, ms. */
const RETRY = 400;
/** How much the picture is blurred, window px: enough for the island's words to read over anything. */
const BLUR = 5;
/** The side of a square of the window whose lightness is kept, window px. */
const CELL = 20;
const COLS = Math.ceil(PANEL_W / CELL);
const ROWS = Math.ceil(PANEL_H / CELL);

/** What the screen behind gave: a picture, nothing for now (a locked screen), or nothing ever (Linux). */
export type Behind =
  | { kind: "picture"; w: number; h: number; pixels: Uint8ClampedArray<ArrayBuffer> }
  | { kind: "not-now" }
  | { kind: "never" };

/** Reads island_backdrop's answer: nothing is never, a width and height of 0 not now, else the picture. */
export function readBehind(data: ArrayBuffer | null): Behind {
  if (!data) return { kind: "not-now" };
  if (data.byteLength === 0) return { kind: "never" };
  if (data.byteLength < 8) return { kind: "not-now" };
  const head = new DataView(data, 0, 8);
  const w = head.getUint32(0, true);
  const h = head.getUint32(4, true);
  if (!w || !h || data.byteLength < 8 + w * h * 4) return { kind: "not-now" };
  // A window still drawn as the wake strip, or caught as it grows back: not the island's panel.
  if (Math.abs(w / h - PANEL_W / PANEL_H) > 0.05) return { kind: "not-now" };
  return { kind: "picture", w, h, pixels: new Uint8ClampedArray(data, 8, w * h * 4) };
}

/** How light each square of the window is in a picture, 0 (black) to 1 (white). */
export function lightness(w: number, h: number, pixels: Uint8ClampedArray): Float32Array {
  const cells = new Float32Array(COLS * ROWS);
  const sx = w / PANEL_W;
  const sy = h / PANEL_H;
  for (let row = 0; row < ROWS; row++) {
    for (let col = 0; col < COLS; col++) {
      let sum = 0;
      let n = 0;
      // Nine points of each square.
      for (let j = 0; j < 3; j++) {
        for (let i = 0; i < 3; i++) {
          const x = Math.min(w - 1, Math.floor((col * CELL + (i + 0.5) * (CELL / 3)) * sx));
          const y = Math.min(h - 1, Math.floor((row * CELL + (j + 0.5) * (CELL / 3)) * sy));
          const p = (y * w + x) * 4;
          sum += 0.2126 * pixels[p] + 0.7152 * pixels[p + 1] + 0.0722 * pixels[p + 2];
          n++;
        }
      }
      cells[row * COLS + col] = sum / n / 255;
    }
  }
  return cells;
}

/** How light what is behind a rectangle of the window is (window px), 0 to 1. */
export function lightUnder(cells: Float32Array, x: number, y: number, w: number, h: number): number {
  const c0 = Math.max(0, Math.floor(x / CELL));
  const c1 = Math.min(COLS - 1, Math.floor((x + Math.max(w, 1) - 1) / CELL));
  const r0 = Math.max(0, Math.floor(y / CELL));
  const r1 = Math.min(ROWS - 1, Math.floor((y + Math.max(h, 1) - 1) / CELL));
  let sum = 0;
  let n = 0;
  for (let row = r0; row <= r1; row++) {
    for (let col = c0; col <= c1; col++) {
      sum += cells[row * COLS + col];
      n++;
    }
  }
  return n ? sum / n : 0;
}

/** Whether two pictures differ, square by square, more than a caret blinking or a clock turning would. */
export function differs(a: Float32Array | null, b: Float32Array): boolean {
  if (!a || a.length !== b.length) return true;
  for (let i = 0; i < a.length; i++) if (Math.abs(a[i] - b[i]) > 0.03) return true;
  return false;
}

export class Backdrop {
  readonly canvas = document.createElement("canvas");
  /** The picture as read, its edges stretched outward, for the blur to read past them. */
  private padded = document.createElement("canvas");
  private timer: number | null = null;
  private running = false;
  private reading = false;
  private delay = EVERY;
  private retry = RETRY;
  private readAt = 0;
  private cells: Float32Array | null = null;
  /** The screen behind can never be read here: never asked again. */
  private unavailable = !IS_TAURI;
  /** A picture of what is behind is drawn (`seen`), or it was taken away. */
  private readonly changed: (seen: boolean) => void;

  constructor(changed: (seen: boolean) => void) {
    this.changed = changed;
    this.canvas.id = "island-backdrop";
    this.canvas.setAttribute("aria-hidden", "true");
  }

  /**
   * The island is on screen: read now (once the window has grown back from its wake strip), then now and then. Asked
   * again as it opens, it reads again soon, as what is behind may have changed meanwhile.
   */
  start() {
    if (this.unavailable) return;
    if (this.running) {
      if (!this.reading && performance.now() - this.readAt > 1000) {
        this.delay = EVERY;
        this.schedule(60);
      }
      return;
    }
    this.running = true;
    this.delay = EVERY;
    this.schedule(160);
  }

  /** The island is hidden, or the look is classic: nothing is read until it shows again. */
  stop() {
    this.running = false;
    if (this.timer != null) {
      window.clearTimeout(this.timer);
      this.timer = null;
    }
  }

  /** Forgets the picture: the island goes back to its own water. */
  clear() {
    this.stop();
    this.cells = null;
    this.changed(false);
  }

  /** How light what is behind a rectangle of the window is (window px), 0 to 1; null before a picture. */
  lightUnder(x: number, y: number, w: number, h: number): number | null {
    return this.cells ? lightUnder(this.cells, x, y, w, h) : null;
  }

  private schedule(ms: number) {
    if (this.timer != null) window.clearTimeout(this.timer);
    this.timer = window.setTimeout(() => {
      this.timer = null;
      void this.read();
    }, ms);
  }

  private async read() {
    if (!this.running || this.reading) return;
    this.reading = true;
    let behind: Behind;
    try {
      behind = readBehind(await Bridge.islandBackdrop());
    } finally {
      this.reading = false;
      this.readAt = performance.now();
    }
    if (behind.kind === "never") {
      this.unavailable = true;
      this.clear();
      return;
    }
    if (!this.running) return;
    if (behind.kind === "not-now") {
      // The window caught as it grows back, or a locked screen: tried again soon, then less and less often. The last
      // picture stays until the next one.
      this.schedule(this.retry);
      this.retry = Math.min(MOST, this.retry * 2);
      return;
    }
    this.retry = RETRY;
    const cells = lightness(behind.w, behind.h, behind.pixels);
    const moved = differs(this.cells, cells);
    if (moved) this.draw(behind.w, behind.h, behind.pixels);
    this.cells = cells;
    if (moved) this.changed(true);
    // Read less often while nothing behind moves: each reading leaves the island out of captures for a moment.
    this.delay = moved ? EVERY : Math.min(MOST, Math.round(this.delay * 1.6));
    this.schedule(this.delay);
  }

  /** Draws the picture blurred, with no dark seam where it ends: the blur reads the edges' own colours past them. */
  private draw(w: number, h: number, pixels: Uint8ClampedArray<ArrayBuffer>) {
    const blur = BLUR * (w / PANEL_W);
    const pad = Math.ceil(blur * 3);
    const pw = w + 2 * pad;
    const ph = h + 2 * pad;
    if (this.padded.width !== pw) this.padded.width = pw;
    if (this.padded.height !== ph) this.padded.height = ph;
    const p = this.padded.getContext("2d");
    if (!p) return;
    p.putImageData(new ImageData(pixels, w, h), pad, pad);
    p.imageSmoothingEnabled = false;
    p.drawImage(this.padded, pad, pad, w, 1, pad, 0, w, pad);
    p.drawImage(this.padded, pad, pad + h - 1, w, 1, pad, pad + h, w, pad);
    p.drawImage(this.padded, pad, 0, 1, ph, 0, 0, pad, ph);
    p.drawImage(this.padded, pad + w - 1, 0, 1, ph, pad + w, 0, pad, ph);

    if (this.canvas.width !== w) this.canvas.width = w;
    if (this.canvas.height !== h) this.canvas.height = h;
    const ctx = this.canvas.getContext("2d");
    if (!ctx) return;
    ctx.filter = `blur(${blur.toFixed(1)}px)`;
    ctx.drawImage(this.padded, -pad, -pad);
    ctx.filter = "none";
  }
}
