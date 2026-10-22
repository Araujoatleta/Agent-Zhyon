// AXYON — the character's look, drawn in code (Canvas 2D). The body, its animations and its outfits are the
// engine's (engine.ts, outfits.ts); this is what AXYON is made of, shared by the engine, the greeting (greeting.ts)
// and the drop sequence (upload/canvas.ts):
//
//   - a pearl shell: the same squircle body, white with a cool sheen, a rim of light and a soft shade;
//   - a face screen of dark glass, wrapped on the front of the head, where the eyes are light (LEDs) in the colour
//     of what AXYON is doing, with a glow, a reflection, and now and then a line of light sweeping down;
//   - an antenna on top whose tip glows in the same colour and sways on the soft-part spring;
//   - two ear pods with a stripe of light, and the AXYON mark low on the front.
//
// Coordinates are the engine's body space: origin at the body centre, y down, R = W × 0.3, rx = 1.14 R, ry = 0.88 R.
// Nothing here keeps time or state: the engine says what to draw, so nothing runs while AXYON is still.

export type RGB = readonly [number, number, number]; // components 0…1

/** The light of each state: eyes, antenna, mark. Idle is the water's teal. */
export const LED: Record<string, RGB> = {
  idle: [0.369, 0.918, 0.831], // #5EEAD4
  working: [0.302, 0.639, 1], // #4DA3FF
  thinking: [0.655, 0.545, 0.98], // #A78BFA
  searching: [0.506, 0.549, 0.973], // #818CF8
  approval: [0.984, 0.749, 0.141], // #FBBF24
  question: [0.133, 0.827, 0.933], // #22D3EE
  error: [0.984, 0.443, 0.522], // #FB7185
  finished: [0.204, 0.827, 0.6], // #34D399
  ratelimit: [0.984, 0.573, 0.235], // #FB923C
  sleeping: [0.58, 0.639, 0.722], // #94A3B8
  dizzy: [0.957, 0.447, 0.714], // #F472B6
};

export const SHELL_TOP: RGB = [0.969, 0.98, 0.992]; // #F7FAFD
export const SHELL_BOTTOM: RGB = [0.706, 0.765, 0.812]; // #B4C3CF
const SCREEN_TOP = "#132430";
const SCREEN_BOTTOM = "#04080C";

/** The face screen around the eyes, in head angles: half its width (longitude) and half its height (latitude). */
export const SCREEN_LON = 0.84;
export const SCREEN_LAT = 0.4;
/** How square its corners are (a superellipse's exponent). */
const SCREEN_SQUARE = 3.4;
/** The mark's place, below the screen's centre, in head latitude. */
const MARK_LAT = -0.66;

export const rgba = (c: RGB, a = 1) =>
  `rgba(${Math.round(c[0] * 255)},${Math.round(c[1] * 255)},${Math.round(c[2] * 255)},${a})`;

export const mixRGB = (a: RGB, b: RGB, t: number): RGB => [
  a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t,
];

/** The current transform's scale: shadows are drawn in device pixels, whatever the transform. */
function deviceScale(x: CanvasRenderingContext2D): number {
  const m = x.getTransform();
  return Math.hypot(m.a, m.b) || 1;
}

/** A glow of `color` around what is drawn next, `blur` in the body's own units. */
export function glow(x: CanvasRenderingContext2D, color: RGB, blur: number, alpha = 0.9) {
  x.shadowColor = rgba(color, alpha);
  x.shadowBlur = blur * deviceScale(x);
  x.shadowOffsetX = 0;
  x.shadowOffsetY = 0;
}

export function noGlow(x: CanvasRenderingContext2D) {
  x.shadowBlur = 0;
  x.shadowColor = "rgba(0,0,0,0)";
}

/** The shell's gradient, for the hands and the pods too. */
export function pearl(x: CanvasRenderingContext2D, x0: number, y0: number, x1: number, y1: number, dim = 0): CanvasGradient {
  const g = x.createLinearGradient(x0, y0, x1, y1);
  g.addColorStop(0, rgba(mixRGB(SHELL_TOP, [0.55, 0.62, 0.68], dim)));
  g.addColorStop(1, rgba(mixRGB(SHELL_BOTTOM, [0.32, 0.38, 0.44], dim)));
  return g;
}

/**
 * The pearl shell over `body`: the gradient, a cool sheen low on the left and a warm one high on the right, the shade
 * toward the edge, the highlight, and a thin rim of light inside the top edge. `fill` false leaves the base colour
 * (a body already painted, a pumpkin's orange) and lays the rest over it.
 */
export function paintShell(x: CanvasRenderingContext2D, body: Path2D, rx: number, ry: number, R: number, fill = true) {
  if (fill) {
    x.fillStyle = pearl(x, rx * 0.7, -ry * 0.85, -rx * 0.8, ry * 0.9);
    x.fill(body);
  }
  x.save();
  x.clip(body);
  const cool = x.createRadialGradient(-rx * 0.6, ry * 0.55, 0, -rx * 0.6, ry * 0.55, R * 1.1);
  cool.addColorStop(0, "rgba(94,234,212,0.16)");
  cool.addColorStop(1, "rgba(94,234,212,0)");
  x.fillStyle = cool;
  x.fill(body);
  const warm = x.createRadialGradient(rx * 0.75, -ry * 0.2, 0, rx * 0.75, -ry * 0.2, R * 0.75);
  warm.addColorStop(0, "rgba(255,170,120,0.10)");
  warm.addColorStop(1, "rgba(255,170,120,0)");
  x.fillStyle = warm;
  x.fill(body);

  const shade = x.createRadialGradient(0, 0, R * 0.2, 0, 0, R * 1.25);
  shade.addColorStop(0, "rgba(10,20,30,0)");
  shade.addColorStop(0.62, "rgba(10,20,30,0)");
  shade.addColorStop(1, "rgba(10,20,30,0.24)");
  x.fillStyle = shade;
  x.fill(body);

  const hl = x.createRadialGradient(rx * 0.36, -ry * 0.5, 0, rx * 0.36, -ry * 0.5, R * 0.46);
  hl.addColorStop(0, "rgba(255,255,255,0.75)");
  hl.addColorStop(1, "rgba(255,255,255,0)");
  x.fillStyle = hl;
  x.fill(body);

  if (R > 10) {
    const rim = x.createLinearGradient(0, -ry, 0, ry * 0.2);
    rim.addColorStop(0, "rgba(255,255,255,0.9)");
    rim.addColorStop(1, "rgba(255,255,255,0)");
    x.strokeStyle = rim;
    x.lineWidth = R * 0.07;
    x.stroke(body);
  }
  x.restore();
}

// ── The face screen ──────────────────────────────────────────────────────────

export interface Screen {
  path: Path2D;
  /** How much of it faces the viewer (its centre's depth): 0 hidden … 1 straight on. */
  facing: number;
  top: number;
  bottom: number;
  left: number;
  right: number;
}

/**
 * The face screen: a patch with rounded corners on the front of the head around (lon, lat) — the eyes' own mapping,
 * so it turns and rolls with them — projected to the body. Where it wraps behind the head it follows the silhouette.
 * `dy` moves it down (the mailbox morph lowers the face).
 */
export function screenPath(rx: number, ry: number, lon: number, lat: number, dy = 0, scale = 1): Screen {
  const path = new Path2D();
  const n = 56;
  const e = 2 / SCREEN_SQUARE;
  let top = Infinity;
  let bottom = -Infinity;
  let left = Infinity;
  let right = -Infinity;
  for (let i = 0; i <= n; i++) {
    const a = (i / n) * Math.PI * 2;
    const ca = Math.cos(a);
    const sa = Math.sin(a);
    const u = Math.sign(ca) * Math.pow(Math.abs(ca), e);
    const v = Math.sign(sa) * Math.pow(Math.abs(sa), e);
    const lo = lon + u * SCREEN_LON * scale;
    const la = lat + v * SCREEN_LAT * scale;
    let px = Math.sin(lo) * Math.cos(la);
    let py = -Math.sin(la);
    if (Math.cos(lo) * Math.cos(la) < 0) {
      const k = Math.hypot(px, py) || 1;
      px /= k;
      py /= k;
    }
    const X = px * rx;
    const Y = py * ry + dy;
    if (i === 0) path.moveTo(X, Y);
    else path.lineTo(X, Y);
    top = Math.min(top, Y);
    bottom = Math.max(bottom, Y);
    left = Math.min(left, X);
    right = Math.max(right, X);
  }
  path.closePath();
  return { path, facing: Math.cos(lon) * Math.cos(lat), top, bottom, left, right };
}

/**
 * Paints the screen: dark glass, the light of the eyes inside it, a reflection, a bright upper edge, and the line of
 * light sweeping down (`scan`, 0…1, 0 = none) or a flash (`flash`, 0…1).
 */
export function paintScreen(
  x: CanvasRenderingContext2D, s: Screen, R: number, led: RGB, alpha: number, scan = 0, flash = 0,
) {
  if (alpha <= 0.01 || s.facing <= 0.02) return;
  x.save();
  x.globalAlpha *= alpha;
  const g = x.createLinearGradient(0, s.top, 0, s.bottom);
  g.addColorStop(0, SCREEN_TOP);
  g.addColorStop(1, SCREEN_BOTTOM);
  x.fillStyle = g;
  x.fill(s.path);

  x.save();
  x.clip(s.path);
  const cx = (s.left + s.right) / 2;
  const cy = (s.top + s.bottom) / 2;
  const w = s.right - s.left;
  const h = s.bottom - s.top;
  const inner = x.createRadialGradient(cx, cy, 0, cx, cy, Math.max(w, h) * 0.62);
  inner.addColorStop(0, rgba(led, 0.16 + 0.5 * flash));
  inner.addColorStop(1, rgba(led, 0));
  x.fillStyle = inner;
  x.fillRect(s.left, s.top, w, h);

  // The reflection: a soft band across the upper left, as on glass.
  if (R > 9) {
    x.save();
    x.translate(cx - w * 0.18, s.top + h * 0.12);
    x.rotate(-0.42);
    const band = x.createLinearGradient(0, -h * 0.3, 0, h * 0.3);
    band.addColorStop(0, "rgba(255,255,255,0)");
    band.addColorStop(0.5, "rgba(255,255,255,0.11)");
    band.addColorStop(1, "rgba(255,255,255,0)");
    x.fillStyle = band;
    x.fillRect(-w, -h * 0.3, w * 2, h * 0.6);
    x.restore();
  }

  if (scan > 0 && scan < 1) {
    const y = s.top + h * scan;
    const a = Math.sin(Math.PI * scan);
    const line = x.createLinearGradient(0, y - h * 0.16, 0, y + h * 0.04);
    line.addColorStop(0, rgba(led, 0));
    line.addColorStop(0.8, rgba(led, 0.28 * a));
    line.addColorStop(1, rgba(mixRGB(led, [1, 1, 1], 0.6), 0.6 * a));
    x.fillStyle = line;
    x.fillRect(s.left, y - h * 0.16, w, h * 0.2);
  }
  x.restore();

  // A bright upper edge, and the bezel's dark line around it.
  if (R > 8) {
    x.save();
    x.clip(s.path);
    const edge = x.createLinearGradient(0, s.top, 0, s.top + h * 0.35);
    edge.addColorStop(0, "rgba(255,255,255,0.32)");
    edge.addColorStop(1, "rgba(255,255,255,0)");
    x.strokeStyle = edge;
    x.lineWidth = R * 0.05;
    x.stroke(s.path);
    x.restore();
    x.strokeStyle = "rgba(6,14,22,0.55)";
    x.lineWidth = R * 0.03;
    x.stroke(s.path);
  }
  x.restore();
}

// ── Antenna, pods, mark ──────────────────────────────────────────────────────

/**
 * The antenna from the top of the head, behind the body (its base is hidden by it). `dx`, `dy` are the soft-part
 * spring (−1…1); `level` the tip's brightness (0…1); `ping` a ring of light leaving the tip (0…1, 0 = none).
 */
export function drawAntenna(
  x: CanvasRenderingContext2D, R: number, ry: number, dx: number, dy: number, led: RGB, level: number, ping = 0,
  alpha = 1,
) {
  if (alpha <= 0.01) return;
  const len = R * 0.42;
  const baseY = -ry * 0.88;
  const tipX = dx * R * 0.34;
  const tipY = -ry - len + Math.max(-1, Math.min(1, dy)) * R * 0.1;
  x.save();
  x.globalAlpha *= alpha;
  x.lineCap = "round";
  if (R > 7) {
    x.strokeStyle = pearl(x, R * 0.05, tipY, -R * 0.05, baseY, 0.25);
    x.lineWidth = Math.max(1, R * 0.07);
    x.beginPath();
    x.moveTo(0, baseY);
    x.quadraticCurveTo(tipX * 0.15, (baseY + tipY) / 2, tipX, tipY);
    x.stroke();
  }
  const r = Math.max(1.2, R * 0.115);
  const lit = Math.max(0, Math.min(1, level));
  if (ping > 0 && ping < 1) {
    x.strokeStyle = rgba(led, 0.7 * (1 - ping));
    x.lineWidth = Math.max(0.8, R * 0.04);
    x.beginPath();
    x.arc(tipX, tipY, r * (1.2 + ping * 2.6), 0, Math.PI * 2);
    x.stroke();
  }
  if (R > 6) glow(x, led, R * 0.45 * lit, 0.85 * lit);
  const tip = x.createRadialGradient(tipX - r * 0.3, tipY - r * 0.35, 0, tipX, tipY, r);
  tip.addColorStop(0, rgba(mixRGB(led, [1, 1, 1], 0.55 + 0.3 * lit)));
  tip.addColorStop(1, rgba(mixRGB(led, [0.1, 0.16, 0.2], 0.55 * (1 - lit))));
  x.fillStyle = tip;
  x.beginPath();
  x.arc(tipX, tipY, r, 0, Math.PI * 2);
  x.fill();
  noGlow(x);
  x.restore();
}

/** The ear pods, behind the body on either side, each with a stripe of light. They lean as the head turns. */
export function drawPods(x: CanvasRenderingContext2D, R: number, rx: number, ry: number, yaw: number, led: RGB, level = 1) {
  if (R <= 8) return;
  const w = R * 0.22;
  const h = R * 0.46;
  for (const sd of [-1, 1]) {
    const px = sd * rx * 0.97 - Math.sin(yaw) * rx * 0.09;
    const py = -ry * 0.04;
    x.save();
    x.translate(px, py);
    x.fillStyle = pearl(x, w * 0.5, -h / 2, -w * 0.5, h / 2, 0.18);
    roundRect(x, -w / 2, -h / 2, w, h, w / 2);
    x.fill();
    x.strokeStyle = "rgba(10,20,30,0.18)";
    x.lineWidth = Math.max(0.6, R * 0.02);
    x.stroke();
    glow(x, led, R * 0.25 * level, 0.8 * level);
    x.strokeStyle = rgba(mixRGB(led, [1, 1, 1], 0.25), 0.55 + 0.4 * level);
    x.lineWidth = Math.max(1, R * 0.05);
    x.lineCap = "round";
    x.beginPath();
    x.moveTo(sd * w * 0.24, -h * 0.22);
    x.lineTo(sd * w * 0.24, h * 0.22);
    x.stroke();
    noGlow(x);
    x.restore();
  }
}

/** The AXYON mark: an X of light, `s` its half size, foreshortened by (fx, fy) as it turns with the head. */
export function drawMark(
  x: CanvasRenderingContext2D, cx: number, cy: number, s: number, fx: number, fy: number, led: RGB, alpha: number,
) {
  if (alpha <= 0.02 || s < 1.2) return;
  x.save();
  x.translate(cx, cy);
  x.scale(Math.max(0.15, fx), Math.max(0.15, fy));
  x.globalAlpha *= alpha;
  glow(x, led, s * 1.6, 0.9);
  x.strokeStyle = rgba(mixRGB(led, [1, 1, 1], 0.3));
  x.lineWidth = s * 0.42;
  x.lineCap = "round";
  x.beginPath();
  x.moveTo(-s, -s);
  x.lineTo(s, s);
  x.moveTo(s, -s);
  x.lineTo(-s, s);
  x.stroke();
  noGlow(x);
  x.restore();
}

/** Where the mark sits for a head turned by (yaw, lat): its centre, foreshortening and how much it faces us. */
export function markFrame(rx: number, ry: number, yaw: number, lat: number, dy = 0) {
  const la = lat + MARK_LAT;
  return {
    x: Math.sin(yaw) * Math.cos(la) * rx,
    y: -Math.sin(la) * ry + dy,
    fx: Math.cos(yaw),
    fy: Math.cos(la),
    facing: Math.cos(yaw) * Math.cos(la),
  };
}

function roundRect(x: CanvasRenderingContext2D, X: number, Y: number, W: number, H: number, R: number) {
  const r = Math.max(0, Math.min(R, W / 2, H / 2));
  x.beginPath();
  x.moveTo(X + r, Y);
  x.arcTo(X + W, Y, X + W, Y + H, r);
  x.arcTo(X + W, Y + H, X, Y + H, r);
  x.arcTo(X, Y + H, X, Y, r);
  x.arcTo(X, Y, X + W, Y, r);
  x.closePath();
}
