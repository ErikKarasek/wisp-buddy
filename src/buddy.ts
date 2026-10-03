// The buddy's face. Rust (src-tauri/src/pet.rs) owns the body: where it stands, walking,
// falling, being carried. This page only draws the character and changes its expression
// when Rust says what it is doing ("pet" events), and reports the mouse (grab, release).

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { loadConfig, worn } from "./config";
import { EXPRESSIONS, setShapeMotion, type ExpressionName, type MascotExpression } from "./mascot/mascot";
import { mountMascot } from "./mascot/svg";

type Mode = "idle" | "walk" | "sit" | "sleep" | "fall" | "held";
type PetState = { mode: Mode; facing: number };
type Reaction = "poke" | "land" | "dizzy" | "wake" | "think" | "talk" | "confused" | "remind" | "yawn" | "jump" | "celebrate" | "sad" | "curious";

const BASE: Record<Mode, ExpressionName> = {
  idle: "happy",
  walk: "happy",
  sit: "neutral",
  sleep: "sleepy",
  fall: "surprised",
  held: "surprised",
};

export async function startBuddy() {
  const el = document.getElementById("buddy")!;
  const mascot = mountMascot(el, { expression: "happy", transition: 260 });
  let state: PetState = { mode: "idle", facing: 1 };
  /** An agent in Wisp is working: the buddy looks keen too, now and then with a sparkle. */
  let wispBusy = false;
  let reactingUntil = 0;

  /** The face for what the body is doing, turned the way it walks. */
  const face = () => {
    if (Date.now() < reactingUntil) return;
    const busyFace = wispBusy && (state.mode === "idle" || state.mode === "walk");
    const ex: MascotExpression = EXPRESSIONS[busyFace ? "thriving" : BASE[state.mode]];
    const walking = state.mode === "walk";
    mascot.setExpression({ ...ex, lookX: walking ? state.facing * 0.55 : ex.lookX, wander: walking ? 0.15 : ex.wander });
  };

  /** A face for a moment, then back to what the body is doing. */
  const react = (name: ExpressionName, ms: number) => {
    reactingUntil = Date.now() + ms;
    mascot.setExpression(name);
    setTimeout(face, ms + 30);
  };

  /** Put on what the studio saved, and follow the motion switch. */
  const dress = async () => {
    const cfg = await loadConfig();
    const look = worn(cfg);
    if (look) mascot.setCharacter(look.character);
    setShapeMotion(cfg.shapeMotion !== false);
  };
  await dress();
  void listen("config-changed", async () => {
    await dress();
    react("happy", 600);
  });

  void listen<boolean>("wisp-busy", (e) => {
    wispBusy = e.payload;
    face();
  });

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
    // Talking in the bubble: it thinks while Gemini answers, then perks up, or looks lost.
    else if (e.payload === "think") react("curious", 40_000);
    else if (e.payload === "talk") {
      mascot.roll(500);
      react("happy", 1200);
    } else if (e.payload === "confused") react("sad", 1500);
    // A reminder: jump to attention. Bedtime: a sleepy look first.
    else if (e.payload === "remind") {
      mascot.roll(600);
      react("surprised", 1500);
    } else if (e.payload === "yawn") react("tired", 2500);
    // News from Wisp: an agent finished (a somersault), failed (sad) or waits for you (curious).
    else if (e.payload === "celebrate") {
      mascot.roll(800);
      react("proud", 1600);
    } else if (e.payload === "sad") react("sad", 3000);
    else if (e.payload === "curious") react("curious", 3000);
    // Off on a jump, up onto a window or down from one.
    else if (e.payload === "jump") react("thriving", 700);
  });

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
}
