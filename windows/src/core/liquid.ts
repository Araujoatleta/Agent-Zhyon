// Liquid glass: glass that bends what is behind it, as a thick lens or a drop of water on a window does.
//
// A surface marked data-glass gets an SVG filter applied to its backdrop (backdrop-filter: url(#…)). The filter's
// displacement map is worked out from the surface's own size and corner radius: flat in the middle, bending more and
// more toward the rounded rim, along the rim's normal, with the slope of a convex "squircle" profile, so the rim reads
// as thick glass. Three kinds more than a plain pane:
//
//   capsule  a button, a field or a chip: the same, with a rim as deep as its radius;
//   lens     a selection that magnifies what it sits on, as the tab bars of iOS do;
//   drop     a whole lens: what is behind it shows upside down and magnified, as through a drop of water.
//
// Chromium (WebView2 on Windows) applies an SVG filter to a backdrop; other engines (WebKitGTK on Linux) don't, and
// get the same surfaces frosted instead (liquid.css). Nothing runs while nothing changes: a map is drawn when a
// surface appears or settles at a new size, and the light that follows the pointer moves only with the pointer.

export type GlassKind = "panel" | "capsule" | "lens" | "drop";

const SVG_NS = "http://www.w3.org/2000/svg";

/** Whether this engine bends a backdrop through an SVG filter: Chromium does, WebKit and Gecko don't. */
export const REFRACTS = typeof navigator !== "undefined" && /Chrome\/\d+/.test(navigator.userAgent);

/** How a surface bends: its kind, the depth of its rim (px), how far it moves what is behind (px), and its frost. */
export interface Optics {
  kind: GlassKind;
  rim: number;
  bend: number;
  frost: number;
}

/** The optics of a kind at a size; a surface may change them with data-glass-rim, -bend and -frost (px). */
export function opticsFor(kind: GlassKind, w: number, h: number, el?: HTMLElement): Optics {
  const short = Math.min(w, h);
  const base: Optics =
    kind === "drop"
      ? { kind, rim: short / 2, bend: short * 0.9, frost: 0 }
      : kind === "lens"
        ? { kind, rim: Math.min(short / 2, 16), bend: Math.min(10, short * 0.25), frost: 0 }
        : kind === "capsule"
          ? { kind, rim: Math.min(short / 2, 22), bend: Math.min(14, short * 0.36), frost: 0.6 }
          : { kind, rim: Math.min(26, short / 3), bend: 22, frost: 1.2 };
  const read = (name: string, fallback: number) => {
    const v = el ? Number.parseFloat(el.dataset[name] ?? "") : Number.NaN;
    return Number.isFinite(v) ? v : fallback;
  };
  return { kind, rim: read("glassRim", base.rim), bend: read("glassBend", base.bend), frost: read("glassFrost", base.frost) };
}

// ── The maps ──────────────────────────────────────────────────────────────────

/**
 * How far, as a fraction of the most it moves, a point `e` px inside the rim of depth `rim` is moved: the slope of a
 * squircle profile, (1 − (1 − s)⁴)^¼, steep at the rim and flat where the rim meets the face.
 */
export function rimSlope(e: number, rim: number): number {
  if (e <= 0 || e >= rim) return 0;
  const u = 1 - e / rim;
  const slope = u ** 3 / Math.max(1 - u ** 4, 1e-4) ** 0.75;
  return Math.min(slope / 3, 1);
}

/**
 * The displacement at (x, y) of a w × h surface with corners of radius r, in px (where the backdrop is read from,
 * relative to the point): toward the face's middle along the rim's normal, so the rim magnifies what is under it.
 */
export function displacement(o: Optics, w: number, h: number, r: number, x: number, y: number): [number, number] {
  const px = x - w / 2;
  const py = y - h / 2;
  if (o.kind === "drop") {
    // A ball lens: read from the other side of the middle, nearer it at the middle (magnified) than at the rim.
    const rx = w / 2;
    const ry = h / 2;
    const d = Math.hypot(px / rx, py / ry);
    if (d >= 1) return [0, 0];
    const k = 0.42 + 0.5 * d * d;
    return [-(1 + k) * px, -(1 + k) * py];
  }
  const qx = Math.abs(px) - (w / 2 - r);
  const qy = Math.abs(py) - (h / 2 - r);
  const ox = Math.max(qx, 0);
  const oy = Math.max(qy, 0);
  const out = Math.hypot(ox, oy);
  const inside = -(out + Math.min(Math.max(qx, qy), 0) - r);
  let dx = 0;
  let dy = 0;
  if (o.kind === "lens" && inside > 0) {
    // A selection lens magnifies its whole face a little.
    dx = -px * LENS_ZOOM;
    dy = -py * LENS_ZOOM;
  }
  const m = rimSlope(inside, o.rim);
  if (m > 0) {
    let nx: number;
    let ny: number;
    if (qx > 0 && qy > 0) {
      nx = ox / out;
      ny = oy / out;
    } else if (qx > qy) {
      nx = 1;
      ny = 0;
    } else {
      nx = 0;
      ny = 1;
    }
    dx -= Math.sign(px || 1) * nx * m * o.bend;
    dy -= Math.sign(py || 1) * ny * m * o.bend;
  }
  return [dx, dy];
}

/** How much a selection lens magnifies what it sits on: enough to see, not enough to blur a word. */
const LENS_ZOOM = 0.06;

/** The most a surface moves anything, px: the scale its map is drawn against. */
function reach(o: Optics, w: number, h: number): number {
  if (o.kind === "drop") return 1.92 * Math.max(w, h) / 2 + 1;
  if (o.kind === "lens") return o.bend + (Math.max(w, h) / 2) * LENS_ZOOM + 1;
  return o.bend + 1;
}

/** Draws the map of a surface as an image: red carries the horizontal move, green the vertical, 128 none. */
function drawMap(o: Optics, w: number, h: number, r: number, scale: number): string {
  // Big panes are drawn at half size: the rim is still many pixels deep, and the image is stretched smoothly.
  const q = w * h > 60000 ? 0.5 : 1;
  const mw = Math.max(1, Math.round(w * q));
  const mh = Math.max(1, Math.round(h * q));
  const canvas = document.createElement("canvas");
  canvas.width = mw;
  canvas.height = mh;
  const ctx = canvas.getContext("2d");
  if (!ctx) return "";
  const image = ctx.createImageData(mw, mh);
  const px = image.data;
  const half = scale / 2;
  for (let my = 0; my < mh; my++) {
    for (let mx = 0; mx < mw; mx++) {
      const [dx, dy] = displacement(o, w, h, r, (mx + 0.5) / q, (my + 0.5) / q);
      const i = (my * mw + mx) * 4;
      px[i] = Math.max(0, Math.min(255, Math.round(128 + (dx / half) * 127)));
      px[i + 1] = Math.max(0, Math.min(255, Math.round(128 + (dy / half) * 127)));
      px[i + 2] = 128;
      px[i + 3] = 255;
    }
  }
  ctx.putImageData(image, 0, 0);
  return canvas.toDataURL("image/png");
}

// ── The filters ───────────────────────────────────────────────────────────────

interface Filter {
  id: string;
  image: SVGFEImageElement;
  node: SVGFilterElement;
  users: number;
  idleSince: number;
  /** Its map was stretched over a surface that changed size: no longer the map of its key. */
  stretched: boolean;
}

let defs: SVGSVGElement | null = null;
const filters = new Map<string, Filter>();
let serial = 0;

function svg(tag: string, attrs: Record<string, string | number>, ...children: Element[]): SVGElement {
  const el = document.createElementNS(SVG_NS, tag);
  for (const [k, v] of Object.entries(attrs)) el.setAttribute(k, String(v));
  el.append(...children);
  return el;
}

function holder(): SVGSVGElement {
  if (defs && defs.isConnected) return defs;
  defs = svg("svg", { width: 0, height: 0, "aria-hidden": "true", style: "position:absolute;width:0;height:0;overflow:hidden" }) as SVGSVGElement;
  document.body.append(defs);
  return defs;
}

/** The filter of a surface: read its backdrop through the map, a little frosted, colours split at the rim. */
function build(o: Optics, w: number, h: number, r: number): Filter {
  const id = `lg${++serial}`;
  const scale = 2 * reach(o, w, h);
  const image = svg("feImage", { x: 0, y: 0, width: w, height: h, preserveAspectRatio: "none", result: "map" }) as SVGFEImageElement;
  image.setAttribute("href", drawMap(o, w, h, r, scale));
  const parts: Element[] = [image];
  const source = o.frost > 0 ? "soft" : "SourceGraphic";
  if (o.frost > 0) parts.push(svg("feGaussianBlur", { in: "SourceGraphic", stdDeviation: o.frost, result: "soft" }));
  // Light of each colour bends a little differently (dispersion): a faint coloured fringe along the rim, as in
  // real glass and water. Capsules and drops only: it costs three passes, and a lens sits on words.
  if ((o.kind === "capsule" || o.kind === "drop") && w * h < 60000) {
    const pass = (k: number, keep: string, name: string) => [
      svg("feDisplacementMap", { in: source, in2: "map", scale: scale * k, xChannelSelector: "R", yChannelSelector: "G", result: `${name}d` }),
      svg("feColorMatrix", { in: `${name}d`, type: "matrix", values: keep, result: name }),
    ];
    parts.push(
      ...pass(1.05, "1 0 0 0 0  0 0 0 0 0  0 0 0 0 0  0 0 0 1 0", "r"),
      ...pass(1, "0 0 0 0 0  0 1 0 0 0  0 0 0 0 0  0 0 0 1 0", "g"),
      ...pass(0.95, "0 0 0 0 0  0 0 0 0 0  0 0 1 0 0  0 0 0 1 0", "b"),
      svg("feBlend", { in: "r", in2: "g", mode: "screen", result: "rg" }),
      svg("feBlend", { in: "rg", in2: "b", mode: "screen" }),
    );
  } else {
    parts.push(svg("feDisplacementMap", { in: source, in2: "map", scale, xChannelSelector: "R", yChannelSelector: "G" }));
  }
  const node = svg(
    "filter",
    { id, x: 0, y: 0, width: "100%", height: "100%", "color-interpolation-filters": "sRGB" },
    ...parts,
  ) as SVGFilterElement;
  holder().append(node);
  return { id, image, node, users: 0, idleSince: 0, stretched: false };
}

/** The radius a surface's corners are drawn with, px, from its computed style. */
function radiusOf(el: HTMLElement, w: number, h: number): number {
  const raw = getComputedStyle(el).borderTopLeftRadius;
  const v = Number.parseFloat(raw);
  if (!Number.isFinite(v)) return 0;
  const r = raw.trim().endsWith("%") ? (v / 100) * Math.min(w, h) : v;
  return Math.max(0, Math.min(r, w / 2, h / 2));
}

// ── The surfaces ──────────────────────────────────────────────────────────────

/** The filter each surface uses now, by its key. */
const using = new WeakMap<HTMLElement, string>();
/** Surfaces whose size is settling: their map is drawn again once it holds still. */
const settling = new WeakMap<HTMLElement, number>();
let sizes: ResizeObserver | null = null;

function keyOf(o: Optics, w: number, h: number, r: number): string {
  return `${o.kind}|${w}|${h}|${Math.round(r)}|${o.rim.toFixed(1)}|${o.bend.toFixed(1)}|${o.frost}`;
}

function release(key: string | undefined) {
  if (!key) return;
  const f = filters.get(key);
  if (!f) return;
  f.users = Math.max(0, f.users - 1);
  if (f.users === 0) f.idleSince = performance.now();
  if (f.users === 0 && f.stretched) {
    f.node.remove();
    filters.delete(key);
    return;
  }
  // Forgotten once many are idle: a filter no surface uses costs nothing but memory.
  if (filters.size > 48) {
    for (const [k, other] of filters) {
      if (other.users === 0 && performance.now() - other.idleSince > 2000) {
        other.node.remove();
        filters.delete(k);
      }
    }
  }
}

/** Gives a surface the filter of its size now (drawing its map if none has it yet). */
function fit(el: HTMLElement) {
  const kind = el.dataset.glass as GlassKind;
  const w = Math.round(el.offsetWidth);
  const h = Math.round(el.offsetHeight);
  if (w < 6 || h < 6 || !el.isConnected) return;
  const o = opticsFor(kind, w, h, el);
  const r = kind === "drop" ? Math.min(w, h) / 2 : radiusOf(el, w, h);
  const key = keyOf(o, w, h, r);
  if (using.get(el) === key) {
    el.classList.add("glass-bends");
    return;
  }
  let f = filters.get(key);
  if (!f) {
    f = build(o, w, h, r);
    filters.set(key, f);
  }
  release(using.get(el));
  f.users += 1;
  using.set(el, key);
  el.style.setProperty("--glass-filter", `url(#${f.id})`);
  el.classList.add("glass-bends");
}

/**
 * A size that changed: the surface's own map is stretched over the new size at once (one shared with others is not:
 * the surface is frosted meanwhile), and drawn anew once the size holds.
 */
function resized(el: HTMLElement) {
  const key = using.get(el);
  const f = key ? filters.get(key) : undefined;
  if (f && f.users === 1) {
    f.image.setAttribute("width", String(el.offsetWidth));
    f.image.setAttribute("height", String(el.offsetHeight));
    f.stretched = true;
  } else {
    el.classList.remove("glass-bends");
  }
  const pending = settling.get(el);
  if (pending) window.clearTimeout(pending);
  settling.set(el, window.setTimeout(() => {
    settling.delete(el);
    fit(el);
  }, 140));
}

function attach(el: HTMLElement) {
  if (!REFRACTS || !el.dataset.glass) return;
  if (!sizes) {
    sizes = new ResizeObserver((entries) => {
      for (const entry of entries) {
        const el = entry.target as HTMLElement;
        if (using.has(el)) resized(el);
        else fit(el);
      }
    });
  }
  sizes.observe(el);
}

function detach(el: HTMLElement) {
  sizes?.unobserve(el);
  release(using.get(el));
  using.delete(el);
  if (!el.dataset.glass) {
    el.style.removeProperty("--glass-filter");
    el.classList.remove("glass-bends");
  }
}

function each(node: Node, fn: (el: HTMLElement) => void) {
  if (!(node instanceof HTMLElement)) return;
  if (node.dataset.glass) fn(node);
  for (const el of node.querySelectorAll<HTMLElement>("[data-glass]")) fn(el);
}

// ── The light ─────────────────────────────────────────────────────────────────

/** The light catches the glass where the pointer is: each surface's highlight leans toward it. */
function follow(event: PointerEvent) {
  const el = (event.target as Element | null)?.closest?.<HTMLElement>("[data-glass]");
  if (!el) return;
  const box = el.getBoundingClientRect();
  if (box.width < 1 || box.height < 1) return;
  el.style.setProperty("--glass-x", `${(((event.clientX - box.left) / box.width) * 100).toFixed(1)}%`);
  el.style.setProperty("--glass-y", `${(((event.clientY - box.top) / box.height) * 100).toFixed(1)}%`);
}

/**
 * A filter of a fixed size for surfaces the engine can't find, such as a pseudo-element (a switch's drop): it is
 * named on the root as --{name}-filter, for a stylesheet to use.
 */
export function namedGlass(name: string, kind: GlassKind, w: number, h: number, root: HTMLElement = document.documentElement) {
  if (!REFRACTS) return;
  const o = opticsFor(kind, w, h);
  const r = Math.min(w, h) / 2;
  const key = keyOf(o, w, h, r);
  let f = filters.get(key);
  if (!f) {
    f = build(o, w, h, r);
    filters.set(key, f);
  }
  f.users += 1;
  root.style.setProperty(`--${name}-filter`, `url(#${f.id})`);
}

let marking: MutationObserver | null = null;

/**
 * Marks as glass, while `on`, the elements matching each rule's selector, now and as they appear (the island's glass
 * look: its views are built without knowing the look). Off, the marks are taken away and the surfaces are plain again.
 */
export function markGlass(rules: ReadonlyArray<readonly [string, GlassKind]>, on: boolean) {
  const apply = (scope: ParentNode) => {
    for (const [selector, kind] of rules) {
      if (scope instanceof HTMLElement && scope.matches(selector)) scope.dataset.glass = kind;
      for (const el of scope.querySelectorAll<HTMLElement>(selector)) el.dataset.glass = kind;
    }
  };
  if (on) {
    apply(document);
    if (!marking) {
      marking = new MutationObserver((changes) => {
        for (const change of changes) for (const node of change.addedNodes) if (node instanceof HTMLElement) apply(node);
      });
      marking.observe(document.body, { childList: true, subtree: true });
    }
    return;
  }
  marking?.disconnect();
  marking = null;
  for (const [selector] of rules) for (const el of document.querySelectorAll<HTMLElement>(selector)) delete el.dataset.glass;
}

let started = false;

/**
 * Starts the glass in this document: every element marked data-glass, now or later, bends its backdrop (where the
 * engine can) and catches the light. `root` gets the class glass-refracts or glass-frosted for liquid.css.
 */
export function startGlass(root: HTMLElement = document.documentElement) {
  root.classList.toggle("glass-refracts", REFRACTS);
  root.classList.toggle("glass-frosted", !REFRACTS);
  if (started) return;
  started = true;
  document.addEventListener("pointermove", follow, { passive: true });
  if (!REFRACTS) return;
  each(document.body, attach);
  new MutationObserver((changes) => {
    for (const change of changes) {
      if (change.type === "attributes") {
        const el = change.target as HTMLElement;
        detach(el);
        if (el.dataset.glass) attach(el);
        continue;
      }
      for (const node of change.removedNodes) each(node, detach);
      for (const node of change.addedNodes) each(node, attach);
    }
  }).observe(document.body, { childList: true, subtree: true, attributes: true, attributeFilter: ["data-glass"] });
}
