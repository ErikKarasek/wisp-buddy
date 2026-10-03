// The buddy's settings, kept by Rust in config.json (src-tauri/src/config.rs): the saved
// characters, which one it wears, and whether the shapes move.

import { invoke } from "@tauri-apps/api/core";
import { DEFAULT_CHARACTER, type MascotCharacter } from "./mascot/mascot";

export type Saved = { id: string; name: string; character: MascotCharacter };
/** Rust keeps more in the same file (reminders, bedtime); those ride along untouched. */
export type Config = { characters: Saved[]; wearing: string | null; shapeMotion?: boolean; [other: string]: unknown };

export const newId = () => Math.random().toString(36).slice(2, 10);
export const full = (c: Partial<MascotCharacter>): MascotCharacter => ({ ...DEFAULT_CHARACTER, ...c });

/** A few to start from, so the gallery is never empty. */
const STARTERS: [string, Partial<MascotCharacter>][] = [
  ["Borůvka", { shape: "round", color: "#6d7fe0" }],
  ["Sluníčko", { shape: "sun", color: "#e8d25a" }],
  ["Planetka", { shape: "planet", color: "#4fb3d9" }],
  ["Chobotnička", { shape: "octopus", color: "#8e5bb5", eyeColor: "#f4f5f8" }],
  ["Mourek", { shape: "cat", color: "#3a3f4b" }],
  ["Klíček", { shape: "sprout", color: "#7fc97a" }],
];

export async function loadConfig(): Promise<Config> {
  const raw = (await invoke<Partial<Config> | null>("config_load").catch(() => null)) ?? {};
  if (!Array.isArray(raw.characters) || raw.characters.length === 0) {
    // Saved straight away: ids made up again by the next window would not match these.
    const characters = STARTERS.map(([name, c]) => ({ id: newId(), name, character: full(c) }));
    const cfg: Config = { ...raw, characters, wearing: characters[0].id };
    await invoke("config_save", { config: cfg }).catch(() => {});
    return cfg;
  }
  return { ...raw, characters: raw.characters.map((c) => ({ ...c, character: full(c.character) })), wearing: raw.wearing ?? null };
}

/**
 * Change the characters or what is worn, on top of what is saved right now: the tray may have
 * flipped the motion switch since this window loaded, and that must not be undone.
 */
export async function saveConfig(change: (c: Config) => void): Promise<Config> {
  const cfg = await loadConfig();
  change(cfg);
  await invoke("config_save", { config: cfg });
  return cfg;
}

/** What the buddy wears: the chosen character, or the first one. */
export const worn = (cfg: Config): Saved | undefined => cfg.characters.find((c) => c.id === cfg.wearing) ?? cfg.characters[0];
