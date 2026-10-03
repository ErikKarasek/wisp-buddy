// The studio: put together the buddy's look. Pick one from your gallery or from Wisp's,
// change its shape, colours, eyes and build, then save it and the buddy wears it at once.

import "./studio.css";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { full, loadConfig, newId, saveConfig, worn, type Config, type Saved } from "./config";
import { EXPRESSIONS, setShapeMotion, type ExpressionName, type MascotCharacter, type MascotShape } from "./mascot/mascot";
import { mascotSvg, mountMascot } from "./mascot/svg";

const SHAPES: [MascotShape, string][] = [
  ["round", "Kulička"],
  ["capsule", "Kapsle"],
  ["lemon", "Citron"],
  ["cube", "Kostka"],
  ["cloud", "Mráček"],
  ["ghost", "Duch"],
  ["dome", "Kopeček"],
  ["onigiri", "Onigiri"],
  ["blob", "Želé"],
  ["cat", "Kočka"],
  ["bear", "Méďa"],
  ["bunny", "Zajíc"],
  ["sun", "Sluníčko"],
  ["flower", "Kytička"],
  ["planet", "Planetka"],
  ["star", "Hvězdička"],
  ["octopus", "Chobotnička"],
  ["sprout", "Klíček"],
  ["crown", "Princátko"],
  ["flame", "Plamínek"],
];
const BODY_COLORS = [
  "#6d7fe0", "#8b9cff", "#4fb3d9", "#5fcfa8", "#7fc97a", "#e8d25a", "#f2a65a", "#f08a7e",
  "#e0605a", "#d980c9", "#b4a1f0", "#8e5bb5", "#c9a27e", "#dfe3ec", "#8a909b", "#3a3f4b",
];
const EYE_COLORS = ["#111216", "#2b2140", "#3a1f14", "#f4f5f8"];
const NAMES = ["Pepík", "Bublina", "Knedlík", "Rozinka", "Drobek", "Šiška", "Fazolka", "Oříšek", "Jahůdka", "Kamínek", "Brouček", "Mufin"];

type Slider = { key: "aspect" | "lean" | "eyeSize" | "eyeSpread"; label: string; min: number; max: number; step: number; ends: [string, string] };
const SLIDERS: Slider[] = [
  { key: "aspect", label: "Postava", min: 0.75, max: 1.35, step: 0.01, ends: ["vyšší", "širší"] },
  { key: "lean", label: "Náklon", min: -12, max: 12, step: 1, ends: ["doleva", "doprava"] },
  { key: "eyeSize", label: "Velikost očí", min: 0.6, max: 1.6, step: 0.05, ends: ["malé", "velké"] },
  { key: "eyeSpread", label: "Rozestup očí", min: 0.6, max: 1.5, step: 0.05, ends: ["u sebe", "od sebe"] },
];
/** How it looks doing what the buddy does on the desktop. */
const MOMENTS: [ExpressionName, string, number][] = [
  ["happy", "Pohoda", 0],
  ["happy", "Chůze", 0.55],
  ["neutral", "Sedí", 0],
  ["sleepy", "Spí", 0],
  ["love", "Šťouchnutí", 0],
  ["surprised", "Letí", 0],
  ["tired", "Omámený", 0],
];

type Sel = { kind: "saved"; id: string } | { kind: "new" } | { kind: "wisp"; index: number };

const esc = (s: string) => s.replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]!);
const pick = <T>(list: T[]) => list[Math.floor(Math.random() * list.length)];
const rnd = (lo: number, hi: number, step: number) => Math.round((lo + Math.random() * (hi - lo)) / step) * step;

export async function startStudio() {
  document.body.className = "studio";
  document.body.innerHTML = `
    <aside class="gallery">
      <h3>Moje postavičky</h3><div class="tiles mine"></div>
      <h3 class="from-wisp" hidden>Z Wispu</h3><div class="tiles wisp"></div>
    </aside>
    <main class="editor">
      <section class="preview">
        <div class="m big"></div>
        <div class="moments">${MOMENTS.map(([, label], i) => `<button data-moment="${i}"${i === 0 ? ' class="on"' : ""}>${label}</button>`).join("")}</div>
        <p class="badge" hidden>Tuhle teď nosí Buddy</p>
      </section>
      <section class="form">
        <label class="row">Jméno<input type="text" data-f="name" maxlength="30" spellcheck="false"></label>
        <div class="row top"><span>Tvar</span><div class="shapes"></div></div>
        <div class="row"><span>Barva</span><div class="swatches" data-f="color"></div></div>
        <div class="row"><span>Oči</span><div class="swatches" data-f="eyeColor"></div></div>
        ${SLIDERS.map((s) => `<label class="row slider"><span>${s.label}</span><span class="range"><small>${s.ends[0]}</small>
          <input type="range" data-f="${s.key}" min="${s.min}" max="${s.max}" step="${s.step}"><small>${s.ends[1]}</small></span></label>`).join("")}
        <p class="note"></p>
        <div class="btns">
          <button data-act="random">Náhodně</button>
          <button data-act="delete">Smazat</button>
          <span class="spacer"></span>
          <button data-act="saveNew">Uložit jako novou</button>
          <button class="primary" data-act="apply"></button>
        </div>
      </section>
    </main>`;
  const q = <T extends HTMLElement>(s: string) => document.querySelector(s) as T;

  let cfg: Config = await loadConfig();
  const fromWisp = ((await invoke<Saved[]>("wisp_characters").catch(() => [])) ?? []).filter((c) => c?.character);
  setShapeMotion(cfg.shapeMotion !== false);

  const current = worn(cfg);
  let sel: Sel = current ? { kind: "saved", id: current.id } : { kind: "new" };
  let draft = { name: "", character: full({}) };
  let source = "";
  let moment = 0;

  const preview = mountMascot(q(".m.big"), { expression: "happy", seed: 9, transition: 220 });
  const savedChar = (id: string) => cfg.characters.find((c) => c.id === id);
  const dirty = () => JSON.stringify(draft) !== source;

  function load(next: Sel) {
    sel = next;
    if (next.kind === "saved") {
      const c = savedChar(next.id)!;
      draft = { name: c.name, character: { ...c.character } };
    } else if (next.kind === "wisp") {
      const c = fromWisp[next.index];
      draft = { name: c.name, character: full(c.character) };
    } else {
      draft = { name: pick(NAMES), character: randomCharacter() };
    }
    source = next.kind === "saved" ? JSON.stringify(draft) : "";
    q<HTMLInputElement>('[data-f="name"]').value = draft.name;
    for (const s of SLIDERS) q<HTMLInputElement>(`[data-f="${s.key}"]`).value = String(draft.character[s.key]);
    changed();
  }

  function randomCharacter(): MascotCharacter {
    const color = pick(BODY_COLORS);
    return full({
      shape: pick(SHAPES)[0],
      color,
      eyeColor: color === "#3a3f4b" || color === "#8e5bb5" ? "#f4f5f8" : "#111216",
      aspect: rnd(0.85, 1.2, 0.01),
      lean: rnd(-6, 6, 1),
      eyeSize: rnd(0.8, 1.3, 0.05),
      eyeSpread: rnd(0.85, 1.2, 0.05),
    });
  }

  function showMoment() {
    const [name, , lookX] = MOMENTS[moment];
    preview.setExpression(lookX ? { ...EXPRESSIONS[name], lookX, wander: 0.15 } : name);
  }

  function renderPickers() {
    const c = draft.character;
    q(".shapes").innerHTML = SHAPES.map(
      ([shape, label]) =>
        `<button class="shape${c.shape === shape ? " on" : ""}" data-shape="${shape}" title="${label}">${mascotSvg({ ...c, shape, lean: 0 }, "neutral", 40)}<small>${label}</small></button>`,
    ).join("");
    const swatches = (key: "color" | "eyeColor", list: string[]) =>
      list.map((col) => `<button class="sw${c[key].toLowerCase() === col ? " on" : ""}" data-col="${col}" style="background:${col}" title="${col}"></button>`).join("") +
      `<label class="sw custom" title="Vlastní barva" style="background:${c[key]}"><input type="color" value="${c[key]}"></label>`;
    q('[data-f="color"]').innerHTML = swatches("color", BODY_COLORS);
    q('[data-f="eyeColor"]').innerHTML = swatches("eyeColor", EYE_COLORS);
  }

  function renderGallery() {
    const tile = (key: string, name: string, ch: Partial<MascotCharacter>, on: boolean, extra = "") =>
      `<button class="tile${on ? " on" : ""}" data-key="${key}">${mascotSvg(ch, "happy", 56)}<span>${esc(name)}</span>${extra}</button>`;
    const mine = cfg.characters
      .map((c) => {
        const here = sel.kind === "saved" && sel.id === c.id;
        return tile(c.id, here ? draft.name || c.name : c.name, here ? draft.character : c.character, here, c.id === cfg.wearing ? "<i>nosí</i>" : "");
      })
      .join("");
    q(".tiles.mine").innerHTML =
      mine + (sel.kind === "new" ? tile("new", draft.name || "Nová", draft.character, true) : "") + `<button class="tile add" data-key="add"><b>+</b><span>Nová</span></button>`;
    q(".from-wisp").hidden = fromWisp.length === 0;
    q(".tiles.wisp").innerHTML = fromWisp
      .map((c, i) => tile(`wisp:${i}`, c.name, sel.kind === "wisp" && sel.index === i ? draft.character : c.character, sel.kind === "wisp" && sel.index === i))
      .join("");
  }

  function renderButtons() {
    const wearing = sel.kind === "saved" && sel.id === cfg.wearing;
    const apply = q<HTMLButtonElement>('[data-act="apply"]');
    apply.textContent = sel.kind !== "saved" ? "Uložit a nosit" : dirty() ? (wearing ? "Uložit" : "Uložit a nosit") : wearing ? "Nosí ji" : "Nosit";
    apply.disabled = sel.kind === "saved" && wearing && !dirty();
    q<HTMLButtonElement>('[data-act="saveNew"]').hidden = sel.kind !== "saved" || !dirty();
    q<HTMLButtonElement>('[data-act="delete"]').hidden = sel.kind !== "saved" || cfg.characters.length < 2;
    q(".badge").hidden = !wearing;
    q(".note").textContent =
      sel.kind === "wisp" ? "Z Wispu se zkopíruje k tvým. Ve Wispu zůstane, jak je." : sel.kind === "new" ? "" : wearing && dirty() ? "Buddy se převleče, jakmile uložíš." : "";
  }

  function changed() {
    preview.setCharacter(draft.character);
    showMoment();
    renderPickers();
    renderGallery();
    renderButtons();
  }

  // ----- form -----
  document.addEventListener("input", (e) => {
    const t = e.target as HTMLInputElement;
    const f = t.dataset.f;
    if (f === "name") {
      draft.name = t.value;
      renderGallery();
      renderButtons();
    } else if (f && SLIDERS.some((s) => s.key === f)) {
      draft.character = { ...draft.character, [f]: Number(t.value) };
      preview.setCharacter(draft.character);
      renderGallery();
      renderButtons();
    } else if (t.type === "color") {
      const key = t.closest('[data-f="eyeColor"]') ? "eyeColor" : "color";
      draft.character = { ...draft.character, [key]: t.value };
      (t.parentElement as HTMLElement).style.background = t.value;
      preview.setCharacter(draft.character);
      renderGallery();
      renderButtons();
    }
  });
  document.addEventListener("change", (e) => {
    // A colour from the picker settles: redraw the swatches so the chosen one is marked.
    if ((e.target as HTMLInputElement).type === "color") renderPickers();
  });

  document.addEventListener("click", async (e) => {
    const el = e.target as HTMLElement;
    const shape = el.closest<HTMLElement>("[data-shape]")?.dataset.shape;
    if (shape) {
      draft.character = { ...draft.character, shape: shape as MascotShape };
      return changed();
    }
    const col = el.closest<HTMLElement>("[data-col]");
    if (col) {
      const key = col.closest('[data-f="eyeColor"]') ? "eyeColor" : "color";
      draft.character = { ...draft.character, [key]: col.dataset.col! };
      return changed();
    }
    const m = el.closest<HTMLElement>("[data-moment]");
    if (m) {
      moment = Number(m.dataset.moment);
      document.querySelectorAll(".moments button").forEach((b) => b.classList.toggle("on", b === m));
      return showMoment();
    }
    const tile = el.closest<HTMLElement>(".tile");
    if (tile) {
      const key = tile.dataset.key!;
      if (key === "add") return load({ kind: "new" });
      if (key === "new") return;
      if (key.startsWith("wisp:")) return load({ kind: "wisp", index: Number(key.slice(5)) });
      return load({ kind: "saved", id: key });
    }
    const act = el.closest<HTMLElement>("[data-act]")?.dataset.act;
    if (act === "random") {
      draft.character = randomCharacter();
      if (sel.kind !== "saved") draft.name = pick(NAMES);
      q<HTMLInputElement>('[data-f="name"]').value = draft.name;
      for (const s of SLIDERS) q<HTMLInputElement>(`[data-f="${s.key}"]`).value = String(draft.character[s.key]);
      return changed();
    }
    if (act === "delete" && sel.kind === "saved") {
      const b = el as HTMLButtonElement;
      if (b.dataset.armed !== "1") {
        b.dataset.armed = "1";
        b.textContent = "Opravdu smazat?";
        setTimeout(() => {
          b.dataset.armed = "";
          b.textContent = "Smazat";
        }, 4000);
        return;
      }
      const id = sel.id;
      cfg = await saveConfig((c) => {
        c.characters = c.characters.filter((x) => x.id !== id);
        if (c.wearing === id) c.wearing = c.characters[0]?.id ?? null;
      });
      return load({ kind: "saved", id: worn(cfg)!.id });
    }
    if (act === "saveNew" || act === "apply") {
      const name = draft.name.trim() || "Postavička";
      const character = { ...draft.character };
      const editing = sel.kind === "saved" && act === "apply" ? sel.id : null;
      const id = editing ?? newId();
      cfg = await saveConfig((c) => {
        const there = c.characters.find((x) => x.id === id);
        if (there) {
          there.name = name;
          there.character = character;
        } else {
          c.characters.push({ id, name, character });
        }
        c.wearing = id;
      });
      return load({ kind: "saved", id });
    }
  });

  // The tray's motion switch, or another studio window, changed the file.
  void listen("config-changed", async () => {
    cfg = await loadConfig();
    setShapeMotion(cfg.shapeMotion !== false);
    renderGallery();
    renderButtons();
  });

  load(sel);
}
