// The buddy's face. Rust (src-tauri/src/pet.rs) owns the body: where it stands, walking,
// falling, being carried. This page only draws the character and changes its expression
// when Rust says what it is doing ("pet" events), and reports the mouse (grab, release).

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { EXPRESSIONS, setShapeMotion, type ExpressionName, type MascotCharacter, type MascotExpression } from "./mascot/mascot";
import { mountMascot } from "./mascot/svg";

type Mode = "idle" | "walk" | "sit" | "sleep" | "fall" | "held";
type PetState = { mode: Mode; facing: number };
type Reaction = "poke" | "land" | "dizzy" | "wake";

const el = document.getElementById("buddy")!;
// A starter look until the studio exists; the same blue as Wisp's first character.
const character: Partial<MascotCharacter> = { shape: "round", color: "#6d7fe0" };
const mascot = mountMascot(el, { character, expression: "happy", transition: 260 });

let state: PetState = { mode: "idle", facing: 1 };
let reactingUntil = 0;

/** The face for what the body is doing, turned the way it walks. */
function face() {
  if (Date.now() < reactingUntil) return;
  const base: Record<Mode, ExpressionName> = {
    idle: "happy",
    walk: "happy",
    sit: "neutral",
    sleep: "sleepy",
    fall: "surprised",
    held: "surprised",
  };
  const ex: MascotExpression = EXPRESSIONS[base[state.mode]];
  const look = state.mode === "walk" ? state.facing * 0.55 : ex.lookX;
  mascot.setExpression({ ...ex, lookX: look, wander: state.mode === "walk" ? 0.15 : ex.wander });
}

/** A face for a moment, then back to what the body is doing. */
function react(name: ExpressionName, ms: number) {
  reactingUntil = Date.now() + ms;
  mascot.setExpression(name);
  setTimeout(face, ms + 30);
}

void listen<PetState>("pet", (e) => {
  state = e.payload;
  el.classList.toggle("held", state.mode === "held");
  face();
});

void listen<Reaction>("pet-react", (e) => {
  if (e.payload === "poke") react(Math.random() < 0.5 ? "love" : "wink", 900);
  else if (e.payload === "dizzy") {
    mascot.roll(700);
    react("tired", 1400);
  } else if (e.payload === "land") react("proud", 500);
  else if (e.payload === "wake") react("surprised", 800);
});

void listen<boolean>("shape-motion", (e) => setShapeMotion(e.payload));

// The mouse: press to pick it up, release to let go (a short click is a poke, Rust decides).
el.addEventListener("mousedown", (e) => {
  if (e.button !== 0) return;
  e.preventDefault();
  void invoke("grab");
});
window.addEventListener("mouseup", (e) => {
  if (e.button !== 0) return;
  void invoke("release");
});

face();
