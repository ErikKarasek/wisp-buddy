/**
 * The mascot for plain web pages: an SVG string, and a tiny animated mount that
 * needs nothing but a DOM. This is the half other projects (the portfolio, the
 * job tracker) use; the React apps render the primitives themselves.
 */
import {
  blendExpressions,
  EXPRESSIONS,
  mascotFrame,
  mascotPose,
  stillPose,
  type ExpressionName,
  type MascotCharacter,
  type MascotExpression,
  type MascotGeometry,
  type MascotPrimitive,
} from "./mascot";

function primitiveSvg(p: MascotPrimitive): string {
  if (p.kind === "ellipse") {
    return `<ellipse cx="${p.cx}" cy="${p.cy}" rx="${p.rx}" ry="${p.ry}" fill="${p.fill}"/>`;
  }
  if (p.kind === "path") return `<path d="${p.d}" fill="${p.fill}"/>`;
  return `<path d="${p.d}" fill="none" stroke="${p.stroke}" stroke-width="${p.width}" stroke-linecap="round" stroke-linejoin="round"/>`;
}

/** The inside of the <svg>: the whole mascot leaning about its base. */
export function geometryInnerSvg(g: MascotGeometry): string {
  return `<g transform="rotate(${g.tilt} ${g.pivot.x} ${g.pivot.y})">${g.primitives.map(primitiveSvg).join("")}</g>`;
}

/** A complete, static SVG of one expression. */
export function mascotSvg(
  character: Partial<MascotCharacter> = {},
  expression: MascotExpression | ExpressionName = "neutral",
  size = 120,
): string {
  const ex = typeof expression === "string" ? EXPRESSIONS[expression] : expression;
  const g = mascotFrame(character, stillPose(ex));
  return (
    `<svg xmlns="http://www.w3.org/2000/svg" width="${size}" height="${size}" viewBox="0 0 ${g.width} ${g.height}" role="img">` +
    geometryInnerSvg(g) +
    `</svg>`
  );
}

export type MountedMascot = {
  setExpression(next: MascotExpression | ExpressionName): void;
  /** A full roll over the top: the eyes leave upwards and come back from below. */
  roll(ms?: number): void;
  setCharacter(next: Partial<MascotCharacter>): void;
  destroy(): void;
};

const SVG_NS = "http://www.w3.org/2000/svg";

/** Every living mascot on the page, drawn from one shared animation frame. */
const living = new Set<(now: number) => void>();
let loop = 0;
function tick(now: number) {
  for (const draw of living) draw(now);
  loop = living.size ? requestAnimationFrame(tick) : 0;
}
function wake() {
  if (!loop && living.size) loop = requestAnimationFrame(tick);
}

/** Ease out: quick to start, soft to land. */
const settle = (k: number) => 1 - Math.pow(1 - Math.min(1, Math.max(0, k)), 3);

/**
 * Put a living mascot inside `el`. It breathes, blinks and looks around, eases
 * between expressions, and holds still for people who asked for reduced motion.
 *
 * Frames update the attributes of the shapes already on the page instead of
 * rewriting the SVG, and a mascot that is scrolled out of view isn't drawn.
 */
let mounted = 0;

export function mountMascot(
  el: HTMLElement,
  options: {
    character?: Partial<MascotCharacter>;
    expression?: MascotExpression | ExpressionName;
    seed?: number;
    /** Milliseconds to ease from one expression to the next. */
    transition?: number;
  } = {},
): MountedMascot {
  const resolve = (e: MascotExpression | ExpressionName) =>
    typeof e === "string" ? EXPRESSIONS[e] : e;
  let character = options.character ?? {};
  let from = resolve(options.expression ?? "neutral");
  let to = from;
  let changedAt = 0;
  let rollAt = 0;
  let rollMs = 0;
  const transition = options.transition ?? 360;
  const seed = options.seed ?? Math.random() * 100;
  const still =
    typeof matchMedia === "function" && matchMedia("(prefers-reduced-motion: reduce)").matches;

  const svg = document.createElementNS(SVG_NS, "svg");
  svg.setAttribute("viewBox", "0 0 100 100");
  svg.setAttribute("width", "100%");
  svg.setAttribute("height", "100%");
  svg.setAttribute("role", "img");
  const group = document.createElementNS(SVG_NS, "g");
  svg.appendChild(group);
  el.appendChild(svg);

  // Gloss, like a lit ball: a radial highlight top-left fading to a soft shade at
  // the bottom, clipped to the body. Ids are unique per mascot on the page.
  const uid = `m${++mounted}`;
  svg.insertAdjacentHTML(
    "afterbegin",
    `<defs><radialGradient id="${uid}g" gradientUnits="userSpaceOnUse">` +
      `<stop offset="0" stop-color="#fff" stop-opacity=".62"/><stop offset=".38" stop-color="#fff" stop-opacity=".1"/>` +
      `<stop offset=".7" stop-color="#fff" stop-opacity="0"/><stop offset="1" stop-color="#000" stop-opacity=".3"/>` +
      `</radialGradient><clipPath id="${uid}c"></clipPath></defs>`,
  );
  const grad = svg.querySelector("radialGradient") as SVGElement;
  const clip = svg.querySelector("clipPath") as SVGElement;
  const gloss = document.createElementNS(SVG_NS, "rect");
  gloss.setAttribute("x", "0");
  gloss.setAttribute("y", "0");
  gloss.setAttribute("width", "100");
  gloss.setAttribute("height", "100");
  gloss.setAttribute("fill", `url(#${uid}g)`);
  gloss.setAttribute("clip-path", `url(#${uid}c)`);
  gloss.setAttribute("pointer-events", "none");

  let shapes: SVGElement[] = [];
  let kinds = "";
  const set = (node: Element, name: string, value: string | number) => {
    const v = String(value);
    if (node.getAttribute(name) !== v) node.setAttribute(name, v);
  };

  const paint = (now: number) => {
    const k = transition > 0 ? settle((now - changedAt) / transition) : 1;
    const ex = k >= 1 ? to : blendExpressions(from, to, k);
    const pose = still ? stillPose(ex) : mascotPose(ex, now, seed);
    if (rollMs && now - rollAt < rollMs) {
      // Ease in and out, so it winds up, flips, and settles.
      const t = (now - rollAt) / rollMs;
      pose.spin = Math.PI * 2 * (t < 0.5 ? 4 * t * t * t : 1 - Math.pow(-2 * t + 2, 3) / 2);
    }
    const g = mascotFrame(character, pose);
    set(group, "transform", `rotate(${g.tilt} ${g.pivot.x} ${g.pivot.y})`);
    const nextKinds = g.primitives.map((p) => p.kind).join(",");
    if (nextKinds !== kinds) {
      // The set of shapes changed (a blink, the Zs): build them afresh.
      group.replaceChildren();
      shapes = g.primitives.map((p) => {
        const node = document.createElementNS(SVG_NS, p.kind === "ellipse" ? "ellipse" : "path");
        if (p.kind === "stroke") {
          node.setAttribute("fill", "none");
          node.setAttribute("stroke-linecap", "round");
          node.setAttribute("stroke-linejoin", "round");
        }
        group.appendChild(node);
        return node;
      });
      kinds = nextKinds;
    }
    // The highlight sits between the body and the eyes.
    const b = g.body;
    set(grad, "cx", b.cx - b.rx * 0.38);
    set(grad, "cy", b.cy - b.ry * 0.5);
    set(grad, "r", Math.max(b.rx, b.ry) * 1.55);
    const bodyShapes = g.primitives.slice(0, b.count);
    if (clip.childNodes.length !== bodyShapes.length) {
      clip.replaceChildren(...bodyShapes.map((p) => document.createElementNS(SVG_NS, p.kind === "ellipse" ? "ellipse" : "path")));
    }
    bodyShapes.forEach((p, i) => {
      const node = clip.childNodes[i] as Element;
      if (p.kind === "ellipse") {
        set(node, "cx", p.cx);
        set(node, "cy", p.cy);
        set(node, "rx", p.rx);
        set(node, "ry", p.ry);
      } else if (p.kind === "path") set(node, "d", p.d);
    });
    const firstEye = shapes[b.count] ?? null;
    if (gloss.parentNode !== group || gloss.nextSibling !== firstEye) group.insertBefore(gloss, firstEye);
    g.primitives.forEach((p, i) => {
      const node = shapes[i];
      if (p.kind === "ellipse") {
        set(node, "cx", p.cx);
        set(node, "cy", p.cy);
        set(node, "rx", p.rx);
        set(node, "ry", p.ry);
        set(node, "fill", p.fill);
      } else if (p.kind === "path") {
        set(node, "d", p.d);
        set(node, "fill", p.fill);
      } else {
        set(node, "d", p.d);
        set(node, "stroke", p.stroke);
        set(node, "stroke-width", p.width);
      }
    });
  };

  let visible = true;
  const draw = (now: number) => {
    if (visible) paint(now);
  };
  const seen =
    typeof IntersectionObserver === "function"
      ? new IntersectionObserver((entries) => {
          visible = entries.some((e) => e.isIntersecting);
        })
      : null;
  seen?.observe(el);

  paint(performance.now());
  if (!still) {
    living.add(draw);
    wake();
  }

  return {
    setExpression(next) {
      const now = performance.now();
      const k = transition > 0 ? settle((now - changedAt) / transition) : 1;
      // Start from wherever the face is right now, not from the last target.
      from = k >= 1 ? to : blendExpressions(from, to, k);
      to = resolve(next);
      changedAt = now;
      if (still) paint(now);
    },
    roll(ms = 950) {
      if (still) return;
      rollAt = performance.now();
      rollMs = ms;
    },
    setCharacter(next) {
      character = { ...character, ...next };
      if (still) paint(performance.now());
    },
    destroy() {
      living.delete(draw);
      seen?.disconnect();
      svg.remove();
    },
  };
}
