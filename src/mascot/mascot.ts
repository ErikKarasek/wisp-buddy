/**
 * A procedural blob mascot: a flat coloured body and two dark capsule eyes.
 * Nothing else — no mouth, no shading, no highlights. Every expression is made
 * from how each eye is sized and turned and where the face is looking, which is
 * exactly why it reads so clearly at any size.
 *
 * Returns plain primitives, like `companionTree`, so the desktop (<svg>), the
 * mobile app (react-native-svg) and any plain web page (`mascotSvg`) draw the
 * same character from one source. No dependencies on purpose: this file is
 * meant to be copied into other projects as-is.
 *
 * Coordinate space: 100 × 100 viewBox, body centred around (50, 52).
 *
 * The eyes live on an imagined sphere. Each one is a capsule laid on that
 * surface, and the whole face turns with the head, so looking away slides the
 * eyes towards the edge and slants and narrows them on their own — the
 * perspective comes out of the projection instead of being faked per pose.
 */

export type MascotPrimitive =
  | { kind: "ellipse"; cx: number; cy: number; rx: number; ry: number; fill: string }
  | { kind: "path"; d: string; fill: string }
  | { kind: "stroke"; d: string; stroke: string; width: number };

export type MascotShape =
  | "round" | "capsule" | "lemon" | "cube" | "cloud" | "ghost" | "dome" | "onigiri" | "blob" | "cat" | "bear" | "bunny"
  // Made of more than one piece, some of them moving: dots round a sun, moons round a planet.
  | "sun" | "flower" | "planet" | "star" | "octopus" | "sprout" | "crown" | "flame";

export type MascotCharacter = {
  shape: MascotShape;
  color: string;
  eyeColor: string;
  /** Wider than tall above 1. */
  aspect: number;
  /** Resting lean of the body, in degrees. */
  lean: number;
  /** Eye size multiplier; 1 is the default. */
  eyeSize: number;
  /** Eye spacing multiplier; 1 is the default. */
  eyeSpread: number;
};

/**
 * One eye, in the face's own terms. `x`/`y` are angles on the sphere (radians,
 * 0 is straight ahead) measured from where that eye normally sits; `length` and
 * `width` scale the default capsule; `angle` turns it (degrees, 0 is upright,
 * positive leans the top outwards, away from the other eye).
 */
export type EyeSpec = { x: number; y: number; length: number; width: number; angle: number };

/**
 * Every field is numeric or a colour so two expressions can be blended into
 * each other (`blendExpressions`) rather than snapping.
 */
export type MascotExpression = {
  left: EyeSpec;
  right: EyeSpec;
  /** Where the head is turned, −1 … 1. */
  lookX: number;
  lookY: number;
  /** Extra body lean, degrees. */
  tilt: number;
  /** Hop height, viewBox units. */
  bounce: number;
  /** How much the idle gaze wanders, 0 … 1. */
  wander: number;
  /** Replaces the body colour — anger goes red. Empty keeps the character's. */
  tint: string;
  /** 0 … 1, how much of `tint` shows. */
  tintAmount: number;
  /** 0 … 1: rising Zs. */
  zzz: number;
  /** Pill eyes blink; closed or squinting ones would look odd doing it. */
  blinks: boolean;
};

export type MascotPose = {
  expression: MascotExpression;
  /** Gaze added on top of the expression's own, −1 … 1. */
  lookX: number;
  lookY: number;
  /** 0 open … 1 shut. */
  blink: number;
  /** Vertical squash from breathing or landing, ~ −0.1 … 0.1. */
  squash: number;
  /** Upward offset from bouncing. */
  lift: number;
  /** Seconds, for things that drift on their own (the Zs). */
  time: number;
  /** Extra head turn over the top, radians: a roll sends the eyes up and over
   * and brings them back from below. Optional; 0 when missing. */
  spin?: number;
};

export const DEFAULT_CHARACTER: MascotCharacter = {
  shape: "round",
  color: "#6d7fe0",
  eyeColor: "#111216",
  aspect: 1,
  lean: 0,
  eyeSize: 1,
  eyeSpread: 1,
};

const EYE: EyeSpec = { x: 0, y: 0, length: 1, width: 1, angle: 0 };
const eye = (spec: Partial<EyeSpec>): EyeSpec => ({ ...EYE, ...spec });
/** The same eye on both sides; angles mirror through `angle`'s definition. */
const both = (spec: Partial<EyeSpec>) => ({ left: eye(spec), right: eye(spec) });

const NEUTRAL: MascotExpression = {
  ...both({}),
  lookX: 0,
  lookY: 0,
  tilt: 0,
  bounce: 0,
  wander: 1,
  tint: "",
  tintAmount: 0,
  zzz: 0,
  blinks: true,
};

export const EXPRESSIONS = {
  neutral: NEUTRAL,
  // Soft, a little squashed and looking up: contentment rather than a grin.
  happy: {
    ...NEUTRAL,
    ...both({ length: 0.62, width: 1.05, angle: -14, y: -0.02 }),
    lookY: -0.35,
    tilt: -4,
    wander: 0.6,
  },
  // Round, bigger, bouncing.
  thriving: {
    ...NEUTRAL,
    ...both({ length: 0.15, width: 1.35 }),
    lookY: -0.4,
    bounce: 5,
    wander: 0.4,
  },
  // Tops lean together and the face drops.
  sad: {
    ...NEUTRAL,
    ...both({ length: 0.85, angle: -26 }),
    lookY: 0.65,
    lookX: -0.2,
    tilt: 5,
    wander: 0.2,
  },
  // Flat dashes.
  sleepy: {
    ...NEUTRAL,
    ...both({ length: 0.9, width: 0.6, angle: 90, y: 0.05 }),
    lookY: 0.35,
    wander: 0,
    zzz: 1,
    blinks: false,
  },
  surprised: {
    ...NEUTRAL,
    ...both({ length: 0.2, width: 1.5 }),
    lookY: -0.15,
    wander: 0.15,
  },
  // A V: tops apart, bottoms together, and the body flushes red.
  angry: {
    ...NEUTRAL,
    ...both({ length: 0.95, width: 1.1, angle: 30 }),
    lookY: 0.1,
    wander: 0.25,
    tint: "#c0453e",
    tintAmount: 1,
  },
  // One eye bigger than the other, head turned.
  curious: {
    ...NEUTRAL,
    left: eye({ length: 0.7, width: 0.85 }),
    right: eye({ length: 1.25, width: 1.2 }),
    lookX: 0.55,
    lookY: -0.2,
    tilt: 7,
    wander: 0.2,
  },
  // Chin up, eyes leaning back.
  proud: {
    ...NEUTRAL,
    left: eye({ angle: -34, length: 1.05 }),
    right: eye({ angle: -12, length: 0.95 }),
    lookX: -0.45,
    lookY: 0.35,
    tilt: -5,
    wander: 0.2,
  },
  // One eye shut into a dash, the other open, head tipped: "nice".
  wink: {
    ...NEUTRAL,
    left: eye({ length: 0.9, width: 0.6, angle: 80, y: 0.03 }),
    right: eye({ length: 0.62, width: 1.05, angle: -14, y: -0.02 }),
    lookX: 0.25,
    lookY: -0.25,
    tilt: 9,
    wander: 0,
    blinks: false,
  },
  // Soft and glowing, looking up at you; the hearts come from the renderer.
  love: {
    ...NEUTRAL,
    ...both({ length: 0.5, width: 1.15, angle: -18, y: -0.03 }),
    lookY: -0.5,
    tilt: -6,
    bounce: 2,
    wander: 0,
    tint: "#ff7aa8",
    tintAmount: 0.35,
  },
  // Out of breath: heavy lids, head down.
  tired: {
    ...NEUTRAL,
    ...both({ length: 0.45, width: 1, angle: -8, y: 0.06 }),
    lookY: 0.4,
    tilt: 4,
    wander: 0.15,
  },
  // Two dots, far off to the side.
  shy: {
    ...NEUTRAL,
    ...both({ length: 0.1, width: 0.8 }),
    lookX: -0.8,
    lookY: 0.45,
    tilt: -6,
    wander: 0.15,
  },
} satisfies Record<string, MascotExpression>;

export type ExpressionName = keyof typeof EXPRESSIONS;

/**
 * The companion's mood, as the domain computes it, mapped to a face. Kept here
 * so every renderer agrees on it — the same kind of list the stage names were
 * before they drifted apart across four copies.
 */
export const MOOD_EXPRESSION: Record<string, ExpressionName> = {
  dormant: "sleepy",
  wilting: "sad",
  okay: "neutral",
  happy: "happy",
  thriving: "thriving",
};

export function expressionForMood(mood: string): MascotExpression {
  return EXPRESSIONS[MOOD_EXPRESSION[mood] ?? "neutral"];
}

// "worklet" marks the functions the mobile app runs on Reanimated's UI thread,
// every frame, so the mascot keeps moving while JavaScript is busy. Anywhere
// else the directive is an inert string, so desktop and web are unaffected.

function clamp(v: number, lo: number, hi: number) {
  "worklet";
  return Math.max(lo, Math.min(hi, v));
}
function mix(a: number, b: number, t: number) {
  "worklet";
  return a + (b - a) * t;
}
function r2(v: number) {
  "worklet";
  return Math.round(v * 100) / 100;
}

function parseHex(hex: string): [number, number, number] {
  "worklet";
  const h = hex.replace("#", "");
  const full = h.length === 3 ? [...h].map((c) => c + c).join("") : h.slice(0, 6);
  const n = parseInt(full, 16);
  return [(n >> 16) & 255, (n >> 8) & 255, n & 255];
}

function mixHex(a: string, b: string, t: number): string {
  "worklet";
  if (t <= 0) return a;
  if (t >= 1) return b;
  const pa = parseHex(a);
  const pb = parseHex(b);
  return `#${pa
    .map((v, i) =>
      Math.round(mix(v, pb[i], t))
        .toString(16)
        .padStart(2, "0"),
    )
    .join("")}`;
}

/** Relative luminance, 0 black … 1 white. */
function luminance(hex: string): number {
  "worklet";
  const [r, g, b] = parseHex(hex).map((v) => {
    const c = v / 255;
    return c <= 0.03928 ? c / 12.92 : Math.pow((c + 0.055) / 1.055, 2.4);
  });
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

function mixEye(a: EyeSpec, b: EyeSpec, t: number): EyeSpec {
  "worklet";
  return {
    x: mix(a.x, b.x, t),
    y: mix(a.y, b.y, t),
    length: mix(a.length, b.length, t),
    width: mix(a.width, b.width, t),
    angle: mix(a.angle, b.angle, t),
  };
}

/**
 * Blend two expressions. Everything interpolates — the eyes turn and stretch
 * into their new shape, which is most of the charm — except the flags, which
 * switch halfway.
 */
export function blendExpressions(
  from: MascotExpression,
  to: MascotExpression,
  t: number,
): MascotExpression {
  "worklet";
  const k = clamp(t, 0, 1);
  if (k === 0) return from;
  if (k === 1) return to;
  // A tint fading in or out blends from/to "no tint" at the same strength.
  const tint = to.tint || from.tint;
  const fromAmount = from.tint ? from.tintAmount : 0;
  const toAmount = to.tint ? to.tintAmount : 0;
  return {
    left: mixEye(from.left, to.left, k),
    right: mixEye(from.right, to.right, k),
    lookX: mix(from.lookX, to.lookX, k),
    lookY: mix(from.lookY, to.lookY, k),
    tilt: mix(from.tilt, to.tilt, k),
    bounce: mix(from.bounce, to.bounce, k),
    wander: mix(from.wander, to.wander, k),
    tint,
    tintAmount: mix(fromAmount, toAmount, k),
    zzz: mix(from.zzz, to.zzz, k),
    blinks: (k >= 0.5 ? to : from).blinks,
  };
}

/** A cheap deterministic hash in 0 … 1, so blink timing is stable per seed. */
function hash01(n: number): number {
  "worklet";
  const x = Math.sin(n * 127.1 + 311.7) * 43758.5453;
  return x - Math.floor(x);
}

/**
 * Where the idle animation is at `timeMs`. Pure: the same time and seed always
 * give the same pose, so renderers only need a clock, and two mascots with
 * different seeds do not blink in unison.
 */
export function mascotPose(expression: MascotExpression, timeMs: number, seed = 0): MascotPose {
  "worklet";
  const s = timeMs / 1000;
  const sleeping = expression.zzz > 0.5;

  // Breathing: slower and deeper while asleep.
  const breathPeriod = sleeping ? 4.2 : 3.4;
  const breath = Math.sin((s / breathPeriod) * Math.PI * 2);

  // Blinks fall into 3.6 s slots, once per slot at a hashed moment, now and
  // then doubled.
  let blink = 0;
  if (expression.blinks) {
    const slotLen = 3.6;
    const slot = Math.floor(s / slotLen);
    const at = slot * slotLen + hash01(slot + seed * 91) * (slotLen - 0.5);
    const d = s - at;
    if (d >= 0 && d < 0.18) blink = Math.sin((d / 0.18) * Math.PI);
    const d2 = d - 0.28;
    if (hash01(slot * 7 + seed) > 0.8 && d2 >= 0 && d2 < 0.18) {
      blink = Math.sin((d2 / 0.18) * Math.PI);
    }
  }

  // The head drifts on a sum of sines, which never visibly repeats, and holds
  // each glance a moment instead of sliding constantly.
  const w = expression.wander;
  const drift = (f: number, p: number) => {
    const v = Math.sin(s * f + p);
    return Math.sign(v) * Math.pow(Math.abs(v), 0.6);
  };
  const lookX = w * (0.38 * drift(0.41, seed) + 0.14 * Math.sin(s * 1.13 + seed * 2));
  const lookY = w * 0.22 * drift(0.29, seed * 3 + 1);

  // Bouncing: a hop with a squash on landing.
  let lift = 0;
  let land = 0;
  if (expression.bounce > 0) {
    const phase = (s * 1.6) % 1;
    lift = expression.bounce * Math.sin(phase * Math.PI);
    land = phase > 0.9 || phase < 0.06 ? 0.06 : 0;
  }

  return {
    expression,
    lookX,
    lookY,
    blink,
    squash: breath * (sleeping ? 0.025 : 0.014) + land,
    lift,
    time: s,
  };
}

/** Half-width of the body at a given height, as a fraction of its radius. */
function widthAt(shape: MascotShape, v: number): number {
  "worklet";
  // v: −1 at the top … 1 at the bottom.
  if (shape === "lemon") return 0.8 + 0.2 * v;
  return 1;
}

/** A closed outline through points given in polar form around the centre. */
function polar(cx: number, cy: number, rx: number, ry: number, r: (a: number) => number, clampBottom = Infinity): string {
  "worklet";
  const pts: string[] = [];
  for (let i = 0; i < 72; i += 1) {
    const a = (i / 72) * Math.PI * 2;
    const k = r(a);
    pts.push(`${r2(cx + rx * k * Math.cos(a))} ${r2(Math.min(cy + ry * k * Math.sin(a), clampBottom))}`);
  }
  return `M${pts.join(" L")} Z`;
}

function bodyPath(shape: MascotShape, cx: number, cy: number, rx: number, ry: number, t = 0): string {
  "worklet";
  if (shape === "dome") {
    // A mochi: a soft superellipse with its bottom pressed flat, gently squishing.
    return polar(cx, cy - ry * 0.08, rx, ry * 1.06, (a) => {
      const c = Math.abs(Math.cos(a));
      const s = Math.abs(Math.sin(a));
      return (1 / Math.pow(Math.pow(c, 2.6) + Math.pow(s, 2.6), 1 / 2.6)) * (1 + 0.025 * Math.sin(2 * a + t * 1.6));
    }, cy + ry * 0.86);
  }
  if (shape === "blob") {
    // Jelly: a wobbly, uneven outline whose bumps keep flowing round.
    return polar(cx, cy, rx, ry, (a) => 1 + 0.07 * Math.sin(3 * a + 0.6 + t * 0.9) + 0.045 * Math.cos(5 * a - t * 1.3));
  }
  if (shape === "onigiri") {
    // A rounded triangle, point up, its corners jiggling a little.
    const j = (i: number) => Math.sin(t * 1.5 + i * 2.1) * 0.035;
    const v = [
      [cx + j(0) * rx, cy - ry * (1.05 + j(1))],
      [cx + rx * (1.08 + j(2)), cy + ry * 0.92],
      [cx - rx * (1.08 + j(3)), cy + ry * 0.92],
    ];
    const lerp = (a: number[], b: number[], t: number) => [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t];
    let d = "";
    for (let i = 0; i < 3; i += 1) {
      const prev = v[(i + 2) % 3];
      const cur = v[i];
      const next = v[(i + 1) % 3];
      const a = lerp(cur, prev, 0.3);
      const b = lerp(cur, next, 0.3);
      d += `${i === 0 ? "M" : "L"}${r2(a[0])} ${r2(a[1])} Q${r2(cur[0])} ${r2(cur[1])} ${r2(b[0])} ${r2(b[1])} `;
    }
    return `${d}Z`;
  }
  if (shape === "ghost") {
    // A dome on top, straight sides, three soft scallops along the bottom that ripple.
    const pts: string[] = [];
    const top = cy - ry * 0.05;
    for (let i = 0; i <= 32; i += 1) {
      const a = Math.PI + (i / 32) * Math.PI;
      pts.push(`${r2(cx + rx * Math.cos(a))} ${r2(top + ry * 0.95 * Math.sin(a))}`);
    }
    const hem = cy + ry * 0.82;
    pts.push(`${r2(cx + rx)} ${r2(hem)}`);
    for (let k = 0; k < 3; k += 1) {
      for (let j = 1; j <= 8; j += 1) {
        const u = j / 8;
        const x = cx + rx - (2 * rx * (k + u)) / 3;
        pts.push(`${r2(x)} ${r2(hem + Math.sin(u * Math.PI) * ry * 0.2 * (0.7 + 0.3 * Math.sin(t * 2.4 + k * 2.1)))}`);
      }
    }
    return `M${pts.join(" L")} Z`;
  }
  // Superellipse exponent: higher is boxier. The outline wobbles like jelly, a cube less.
  const n = shape === "cube" ? 5 : shape === "capsule" ? 3 : 2;
  const wobble = shape === "cube" ? 0.012 : 0.02;
  const steps = 64;
  const pts: string[] = [];
  for (let i = 0; i < steps; i += 1) {
    const a = (i / steps) * Math.PI * 2;
    const c = Math.cos(a);
    const s = Math.sin(a);
    const ex = Math.sign(c) * Math.pow(Math.abs(c), 2 / n);
    const ey = Math.sign(s) * Math.pow(Math.abs(s), 2 / n);
    const w = 1 + wobble * Math.sin(3 * a + t * 1.7) * Math.sin(t * 0.9);
    pts.push(`${r2(cx + rx * ex * widthAt(shape, ey) * w)} ${r2(cy + ry * ey * w)}`);
  }
  return `M${pts.join(" L")} Z`;
}

/** A cloud: overlapping puffs of the body colour around a solid middle, drifting a little. */
function cloudPuffs(cx: number, cy: number, rx: number, ry: number, fill: string, t = 0) {
  "worklet";
  const puffs: Array<[number, number, number]> = [
    [0, 0.18, 0.78],
    [-0.52, 0.28, 0.5],
    [0.52, 0.28, 0.5],
    [-0.3, -0.3, 0.52],
    [0.28, -0.36, 0.56],
    [0, 0.5, 0.5],
  ];
  return puffs.map(
    ([x, y, r], i): MascotPrimitive => ({
      kind: "ellipse",
      cx: r2(cx + x * rx + Math.sin(t * 0.7 + i * 1.3) * rx * 0.04),
      cy: r2(cy + y * ry + Math.cos(t * 0.9 + i) * ry * 0.03),
      rx: r2(r * rx),
      ry: r2(r * ry),
      fill,
    }),
  );
}

/**
 * Whether the shapes move on their own (a sun's dots turn, ears twitch, jelly wobbles). Off, they
 * stand still in their resting form; breathing, blinking and the expressions stay either way.
 * Per page, so each window that draws mascots sets it from the config.
 */
let shapeMotion = true;
export function setShapeMotion(on: boolean) {
  shapeMotion = on;
}

/** An oval as a path, turned by `angle` radians about its centre: a bunny's ear that sways. */
function oval(cx: number, cy: number, rx: number, ry: number, angle: number): string {
  "worklet";
  const pts: string[] = [];
  for (let i = 0; i < 36; i += 1) {
    const a = (i / 36) * Math.PI * 2;
    const x = Math.cos(a) * rx;
    const y = Math.sin(a) * ry;
    pts.push(`${r2(cx + x * Math.cos(angle) - y * Math.sin(angle))} ${r2(cy + x * Math.sin(angle) + y * Math.cos(angle))}`);
  }
  return `M${pts.join(" L")} Z`;
}

/** How big the body is drawn: smaller where ears, petals, rays or a ring need the room. */
function bodyScale(shape: MascotShape): number {
  "worklet";
  if (shape === "bunny") return 0.74;
  if (shape === "sun" || shape === "flower") return 0.68;
  if (shape === "planet") return 0.74;
  if (shape === "cat" || shape === "bear") return 0.84;
  if (shape === "sprout" || shape === "crown" || shape === "flame") return 0.82;
  return 1;
}

/** Shapes that hang in the middle of the picture instead of standing on the ground. */
function floats(shape: MascotShape): boolean {
  "worklet";
  return shape === "sun" || shape === "flower" || shape === "planet";
}

const GOLD = "#f5c84c";
const LEAF = "#6cc070";

const dot = (x: number, y: number, r: number, fill: string): MascotPrimitive => ({ kind: "ellipse", cx: r2(x), cy: r2(y), rx: r2(r), ry: r2(r), fill });

/** A colour that stands out against the body: lighter on a dark one, darker on a light one. */
function contrast(fill: string, amount: number): string {
  "worklet";
  return luminance(fill) > 0.45 ? mixHex(fill, "#000000", amount * 0.6) : mixHex(fill, "#ffffff", amount);
}

/** A planet's ring: a flat ellipse, tilted, as points for angles a0…a1 (back half is sin < 0). */
function ringPoint(cx: number, cy: number, rx: number, ry: number, a: number): [number, number] {
  "worklet";
  const tilt = -0.28;
  const x = Math.cos(a) * rx * 1.62;
  const y = Math.sin(a) * ry * 0.36;
  return [cx + x * Math.cos(tilt) - y * Math.sin(tilt), cy + x * Math.sin(tilt) + y * Math.cos(tilt)];
}
function ringArc(cx: number, cy: number, rx: number, ry: number, a0: number, a1: number): string {
  "worklet";
  const pts: string[] = [];
  for (let i = 0; i <= 24; i += 1) {
    const [x, y] = ringPoint(cx, cy, rx, ry, a0 + ((a1 - a0) * i) / 24);
    pts.push(`${r2(x)} ${r2(y)}`);
  }
  return `M${pts.join(" L")}`;
}

/** What is drawn over the body but under the eyes: a ring's near half, moons passing in front, a crown. */
function frontPrimitives(shape: MascotShape, cx: number, cy: number, rx: number, ry: number, fill: string, time: number): MascotPrimitive[] {
  "worklet";
  if (shape === "planet") {
    const ring = contrast(fill, 0.5);
    const out: MascotPrimitive[] = [{ kind: "stroke", d: ringArc(cx, cy, rx, ry, 0, Math.PI), stroke: ring, width: 3.4 }];
    for (const m of moons(time)) if (Math.sin(m) >= 0) out.push(dot(...ringPoint(cx, cy, rx, ry, m), 4.4, ring));
    return out;
  }
  if (shape === "crown") {
    // Three points with a ball on each, sitting on top of the head.
    const base = cy - ry * 0.72;
    const w = rx * 0.62;
    const h = ry * 0.55;
    const xs = [-1, -0.5, 0, 0.5, 1].map((t) => cx + t * w);
    const d =
      `M${r2(xs[0])} ${r2(base)} L${r2(xs[0])} ${r2(base - h)} L${r2(xs[1])} ${r2(base - h * 0.45)} L${r2(xs[2])} ${r2(base - h * 1.15)} ` +
      `L${r2(xs[3])} ${r2(base - h * 0.45)} L${r2(xs[4])} ${r2(base - h)} L${r2(xs[4])} ${r2(base)} Q${r2(cx)} ${r2(base + ry * 0.12)} ${r2(xs[0])} ${r2(base)} Z`;
    return [{ kind: "path", d, fill: GOLD }, dot(xs[0], base - h, 2.4, GOLD), dot(xs[2], base - h * 1.15, 2.6, GOLD), dot(xs[4], base - h, 2.4, GOLD)];
  }
  return [];
}

/** Where a planet's two moons are on the ring, going round slowly. */
function moons(time: number): number[] {
  "worklet";
  const a = time * 0.55;
  return [a, a + Math.PI * 0.9];
}

/** The body as one or more shapes of its colour: puffs for a cloud, ears for the animals. */
function bodyPrimitives(shape: MascotShape, cx: number, cy: number, rx: number, ry: number, fill: string, time = 0): MascotPrimitive[] {
  "worklet";
  if (shape === "cloud") return cloudPuffs(cx, cy, rx, ry, fill, time);
  const round: MascotPrimitive = { kind: "path", d: bodyPath("round", cx, cy, rx, ry, time), fill };
  if (shape === "sun") {
    // Eight dots around it, turning slowly.
    const turn = time * 0.25;
    const out: MascotPrimitive[] = [];
    for (let i = 0; i < 8; i += 1) {
      const a = turn + (i * Math.PI) / 4;
      out.push(dot(cx + Math.cos(a) * rx * 1.42, cy + Math.sin(a) * ry * 1.42, Math.min(rx, ry) * 0.17, fill));
    }
    return [...out, round];
  }
  if (shape === "flower") {
    // Six petals of a lighter shade behind a round middle, swaying a little.
    const petal = contrast(fill, 0.45);
    const sway = Math.sin(time * 0.8) * 0.12;
    const out: MascotPrimitive[] = [];
    for (let i = 0; i < 6; i += 1) {
      const a = sway + (i * Math.PI) / 3 - Math.PI / 2;
      out.push(dot(cx + Math.cos(a) * rx * 0.98, cy + Math.sin(a) * ry * 0.98, Math.min(rx, ry) * 0.52, petal));
    }
    return [...out, round];
  }
  if (shape === "planet") {
    const ring = contrast(fill, 0.5);
    const out: MascotPrimitive[] = [{ kind: "stroke", d: ringArc(cx, cy, rx, ry, Math.PI, Math.PI * 2), stroke: ring, width: 3.4 }];
    for (const m of moons(time)) if (Math.sin(m) < 0) out.push(dot(...ringPoint(cx, cy, rx, ry, m), 4.4, ring));
    return [...out, round];
  }
  if (shape === "star") {
    // Five soft points, one straight up.
    return [{ kind: "path", d: polar(cx, cy, rx * 1.05, ry * 1.05, (a) => 0.8 + 0.3 * Math.pow((1 + Math.cos(5 * (a + Math.PI / 2))) / 2, 1.6)), fill }];
  }
  if (shape === "octopus") {
    // A dome with five legs along the bottom that keep wiggling.
    const pts: string[] = [];
    const top = cy - ry * 0.12;
    for (let i = 0; i <= 32; i += 1) {
      const a = Math.PI + (i / 32) * Math.PI;
      pts.push(`${r2(cx + rx * Math.cos(a))} ${r2(top + ry * 0.92 * Math.sin(a))}`);
    }
    const hem = cy + ry * 0.55;
    for (let j = 0; j <= 60; j += 1) {
      const u = j / 60;
      const lobe = Math.pow(Math.abs(Math.sin(u * 5 * Math.PI)), 0.55);
      const wiggle = Math.sin(time * 3 + u * 9) * ry * 0.05;
      pts.push(`${r2(cx + rx - 2 * rx * u + wiggle * 0.6)} ${r2(hem + ry * 0.42 * lobe + wiggle)}`);
    }
    return [{ kind: "path", d: `M${pts.join(" L")} Z`, fill }];
  }
  if (shape === "sprout") {
    // A stem with two leaves out of the top of the head, swaying.
    const sway = Math.sin(time * 1.4) * 0.18;
    const bx = cx;
    const by = cy - ry * 0.9;
    const tx = bx + Math.sin(sway) * ry * 0.42;
    const ty = by - Math.cos(sway) * ry * 0.42;
    const leaf = (side: number): MascotPrimitive => {
      const ex = tx + side * rx * 0.42;
      const ey = ty - ry * 0.12 + side * sway * 4;
      return {
        kind: "path",
        d: `M${r2(tx)} ${r2(ty)} Q${r2((tx + ex) / 2)} ${r2(ty - ry * 0.3)} ${r2(ex)} ${r2(ey)} Q${r2((tx + ex) / 2)} ${r2(ty + ry * 0.12)} ${r2(tx)} ${r2(ty)} Z`,
        fill: LEAF,
      };
    };
    return [{ kind: "stroke", d: `M${r2(bx)} ${r2(cy - ry * 0.6)} L${r2(tx)} ${r2(ty)}`, stroke: LEAF, width: 2.6 }, leaf(-1), leaf(1), round];
  }
  if (shape === "flame") {
    // A drop pointing up whose tip flickers and sways.
    const sway = Math.sin(time * 2.3) * 0.5 + Math.sin(time * 5.1) * 0.15;
    const pts: string[] = [];
    for (let i = 0; i < 72; i += 1) {
      const a = (i / 72) * Math.PI * 2;
      const up = Math.max(0, -Math.sin(a));
      const x = cx + rx * Math.cos(a) * (1 - 0.3 * up) + sway * rx * 0.28 * Math.pow(up, 3);
      const y = cy + ry * Math.sin(a) * (1 + 0.6 * up * up);
      pts.push(`${r2(x)} ${r2(y)}`);
    }
    return [{ kind: "path", d: `M${pts.join(" L")} Z`, fill }];
  }
  if (shape === "cat") {
    // Now and then one ear twitches outwards, the other a moment later.
    const twitch = (side: number) => Math.pow(Math.max(0, Math.sin(time * 0.9 + (side > 0 ? 0.5 : 0))), 14) * rx * 0.14;
    const ear = (side: number) => {
      const k = twitch(side);
      return (
        `M${r2(cx + side * rx * 0.86)} ${r2(cy - ry * 0.32)} L${r2(cx + side * (rx * 0.7 + k))} ${r2(cy - ry * 1.28 + k * 0.4)} ` +
        `Q${r2(cx + side * (rx * 0.62 + k))} ${r2(cy - ry * 1.36 + k * 0.4)} ${r2(cx + side * (rx * 0.52 + k))} ${r2(cy - ry * 1.26 + k * 0.4)} L${r2(cx + side * rx * 0.1)} ${r2(cy - ry * 0.8)} Z`
      );
    };
    return [{ kind: "path", d: ear(-1), fill }, { kind: "path", d: ear(1), fill }, round];
  }
  if (shape === "bear") {
    // Round ears that bob, each in its own time.
    const ear = (side: number): MascotPrimitive => ({ kind: "ellipse", cx: r2(cx + side * rx * 0.66), cy: r2(cy - ry * 0.78 + Math.sin(time * 1.6 + side) * ry * 0.035), rx: r2(rx * 0.3), ry: r2(ry * 0.3), fill });
    return [ear(-1), ear(1), round];
  }
  if (shape === "bunny") {
    // Long ears that sway, leaning out a little from their roots.
    const ear = (side: number): MascotPrimitive => {
      const angle = side * 0.1 + Math.sin(time * 1.3 + side * 0.8) * 0.12;
      const root = [cx + side * rx * 0.34, cy - ry * 0.62];
      const len = ry * 0.5;
      return { kind: "path", d: oval(root[0] + Math.sin(angle) * len, root[1] - Math.cos(angle) * len, rx * 0.17, ry * 0.52, angle), fill };
    };
    return [ear(-1), ear(1), round];
  }
  if (shape === "lemon") {
    // A leaf on top, swaying.
    const sway = Math.sin(time * 1.2) * 0.16;
    const tx = cx + rx * 0.06;
    const ty = cy - ry * 0.98;
    const ex = tx + Math.cos(-0.5 + sway) * rx * 0.5;
    const ey = ty + Math.sin(-0.5 + sway) * rx * 0.5;
    const leaf: MascotPrimitive = {
      kind: "path",
      d: `M${r2(tx)} ${r2(ty)} Q${r2((tx + ex) / 2 - 2)} ${r2((ty + ey) / 2 - 6)} ${r2(ex)} ${r2(ey)} Q${r2((tx + ex) / 2 + 2)} ${r2((ty + ey) / 2 + 3)} ${r2(tx)} ${r2(ty)} Z`,
      fill: LEAF,
    };
    return [leaf, { kind: "path", d: bodyPath(shape, cx, cy, rx, ry, time), fill }];
  }
  return [{ kind: "path", d: bodyPath(shape, cx, cy, rx, ry, time), fill }];
}

export type MascotGeometry = {
  width: number;
  height: number;
  primitives: MascotPrimitive[];
  /** Lean, applied by the renderer as a rotation about `pivot`. */
  tilt: number;
  pivot: { x: number; y: number };
  /** The body's centre and radii, for renderers that shade it (a glossy highlight). */
  body: { cx: number; cy: number; rx: number; ry: number; count: number };
};

/**
 * One frame of the mascot. Pass a `MascotPose` from `mascotPose` for motion,
 * or `stillPose(expression)` for a static picture.
 */
export function mascotFrame(character: Partial<MascotCharacter>, pose: MascotPose): MascotGeometry {
  "worklet";
  const ch = { ...DEFAULT_CHARACTER, ...character };
  const ex = pose.expression;
  const groundY = 92;

  // Animals leave headroom for their ears.
  const baseR = 38 * bodyScale(ch.shape);
  const cloud = ch.shape === "cloud";
  const rx = baseR * Math.sqrt(ch.aspect) * (1 + pose.squash * 0.5) * (cloud ? 0.92 : 1);
  const ry = (baseR / Math.sqrt(ch.aspect)) * (1 - pose.squash) * (cloud ? 0.72 : 1);
  const cx = 50;
  const cy = floats(ch.shape) ? 51 - pose.lift * 0.6 : groundY - ry - pose.lift - (cloud ? 8 : 0);

  const tinted = ex.tint && ex.tintAmount > 0;
  const color = tinted ? mixHex(ch.color, ex.tint, ex.tintAmount) : ch.color;
  // On a tinted body the eyes darken towards that tint rather than staying
  // black, which keeps them part of the same face.
  // Dark eyes vanish on a dark body, so a near-black character gets light ones
  // unless it chose its own.
  const baseEye =
    ch.eyeColor === DEFAULT_CHARACTER.eyeColor && luminance(ch.color) < 0.12
      ? "#f2f2f5"
      : ch.eyeColor;
  const eyeColor = tinted
    ? mixHex(baseEye, mixHex(ex.tint, "#000000", 0.6), ex.tintAmount)
    : baseEye;

  const shapeTime = shapeMotion ? pose.time : 0;
  const primitives: MascotPrimitive[] = bodyPrimitives(ch.shape, cx, cy, rx, ry, color, shapeTime);
  const bodyCount = primitives.length;
  primitives.push(...frontPrimitives(ch.shape, cx, cy, rx, ry, color, shapeTime));

  // ── The head's rotation ─────────────────────────────────────────────────
  const yaw = clamp(ex.lookX + pose.lookX, -1, 1) * 0.62;
  const pitch = clamp(ex.lookY + pose.lookY, -1, 1) * 0.42 - (pose.spin ?? 0);
  const cosY = Math.cos(yaw);
  const sinY = Math.sin(yaw);
  const cosP = Math.cos(pitch);
  const sinP = Math.sin(pitch);

  /** A point on the face (azimuth, elevation) → screen, with its depth. */
  const project = (az: number, el: number) => {
    // Unit sphere, y down, z towards the viewer.
    let x = Math.cos(el) * Math.sin(az);
    let y = Math.sin(el);
    let z = Math.cos(el) * Math.cos(az);
    // Yaw about the vertical axis, then pitch about the horizontal one.
    [x, z] = [x * cosY + z * sinY, -x * sinY + z * cosY];
    [y, z] = [y * cosP + z * sinP, -y * sinP + z * cosP];
    const v = clamp(y, -1, 1);
    return { x: cx + x * rx * 0.94 * widthAt(ch.shape, v), y: cy + y * ry * 0.94, z };
  };

  const spread = 0.25 * ch.eyeSpread;
  const halfLen = 0.13 * ch.eyeSize;
  const thickness = 9.2 * ch.eyeSize;

  for (const side of [-1, 1] as const) {
    const spec = side < 0 ? ex.left : ex.right;
    const az = side * spread + side * spec.x;
    const el = -0.06 + spec.y;
    // Positive angle leans the top outwards, so it mirrors between the eyes.
    const a = (spec.angle * Math.PI) / 180;
    const h = halfLen * spec.length;
    const dAz = side * Math.sin(a) * h;
    const dEl = Math.cos(a) * h;
    const top = project(az + dAz, el - dEl);
    const bottom = project(az - dAz, el + dEl);
    const mid = project(az, el);
    // Turned past the silhouette: hidden behind the head.
    if (mid.z < 0.15) continue;

    // Foreshortening thins the stroke as the eye turns away.
    let width = thickness * spec.width * (0.45 + 0.55 * mid.z);
    let x1 = top.x;
    let y1 = top.y;
    let x2 = bottom.x;
    let y2 = bottom.y;

    // Blink: flatten vertically about the middle and widen into a dash, so an
    // upright capsule closes into a line instead of shrinking to a dot.
    if (pose.blink > 0) {
      const b = pose.blink;
      y1 = mid.y + (y1 - mid.y) * (1 - b);
      y2 = mid.y + (y2 - mid.y) * (1 - b);
      const spreadX = b * Math.min(width * 0.3, 3);
      const leftFirst = x1 <= x2;
      x1 += leftFirst ? -spreadX : spreadX;
      x2 += leftFirst ? spreadX : -spreadX;
      width *= 1 - b * 0.45;
    }

    primitives.push({
      kind: "stroke",
      d: `M${r2(x1)} ${r2(y1)} L${r2(x2)} ${r2(y2)}`,
      stroke: eyeColor,
      width: r2(width),
    });
  }

  // Zs rise and fade from the top right, one after another.
  if (ex.zzz > 0.01) {
    for (let i = 0; i < 3; i += 1) {
      const p = (pose.time * 0.32 + i / 3) % 1;
      const size = 3.5 + p * 4;
      const zx = cx + rx * 0.72 + p * 12;
      const zy = cy - ry * 0.75 - p * 20;
      const alpha = Math.round(Math.sin(p * Math.PI) * 170 * clamp(ex.zzz, 0, 1))
        .toString(16)
        .padStart(2, "0");
      primitives.push({
        kind: "stroke",
        d:
          `M${r2(zx - size / 2)} ${r2(zy - size / 2)} L${r2(zx + size / 2)} ${r2(zy - size / 2)} ` +
          `L${r2(zx - size / 2)} ${r2(zy + size / 2)} L${r2(zx + size / 2)} ${r2(zy + size / 2)}`,
        stroke: `${eyeColor}${alpha}`,
        width: r2(1.2 + size * 0.14),
      });
    }
  }

  return {
    width: 100,
    height: 100,
    primitives,
    tilt: ch.lean + ex.tilt,
    pivot: { x: cx, y: groundY },
    body: { cx, cy, rx, ry, count: bodyCount },
  };
}

/** A pose with no motion, for a static picture of an expression. */
export function stillPose(expression: MascotExpression): MascotPose {
  "worklet";
  return { expression, lookX: 0, lookY: 0, blink: 0, squash: 0, lift: 0, time: 0 };
}
