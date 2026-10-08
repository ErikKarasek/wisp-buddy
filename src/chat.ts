// The chat bubble above the buddy. The conversation lives here; Rust holds the Gemini key and
// makes the call (src-tauri/src/chat.rs), so the key never reaches this page. The microphone
// button records (src-tauri/src/voice.rs) and what was said is sent like typed text. What the
// buddy wants to do but cannot undo (work for Claude Code, switching the PC off) waits here
// for a yes.

import "./chat.css";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { loadConfig, worn } from "./config";

/** A reminder that fired, or a question, shows its buttons until one is pressed. */
type Ask = { id: string; text: string };
type Message = { role: "me" | "buddy"; text: string; reminder?: string; asks?: Ask[]; answered?: boolean };
type Reminder = { id: string; at: number; text: string; repeat: string | null; when: string };
type Fired = { text: string; late: number; bedtime: boolean };
type Reply = { text: string; asks: Ask[] };
type Fact = { id: string; text: string; at: number };

const AI_STUDIO = "https://aistudio.google.com/apikey";

export async function startChat() {
  document.body.className = "chat";
  document.body.innerHTML = `
    <div class="bubble">
      <header><b class="who"></b><button class="clock" title="Připomínky" hidden></button><button class="brain" title="Co si pamatuje" hidden></button><button class="x" title="Zavřít (Esc)">✕</button></header>
      <div class="list" hidden></div>
      <div class="list facts" hidden></div>
      <div class="log"></div>
      <form class="say"><button type="button" class="mic" title="Říct to nahlas (klikni znovu, až domluvíš)">🎙</button><input type="text" placeholder="Napiš mu…" spellcheck="false" autocomplete="off"><button>➤</button></form>
      <form class="key" hidden>
        <p>Abych mohl odpovídat, potřebuju klíč ke Gemini. Je zdarma: přihlas se na <a href="${AI_STUDIO}" target="_blank">aistudio.google.com/apikey</a>, dej <b>Create API key</b> a vlož ho sem. Uložím ho do Klíčenky.</p>
        <input type="password" placeholder="Klíč ke Gemini" spellcheck="false" autocomplete="off">
        <button>Uložit</button>
        <small class="err"></small>
      </form>
    </div>`;
  const $ = <T extends HTMLElement>(s: string) => document.querySelector(s) as T;
  const log = $(".log");
  const say = $<HTMLFormElement>(".say");
  const input = say.querySelector("input")!;
  const keyForm = $<HTMLFormElement>(".key");
  const keyInput = keyForm.querySelector("input")!;

  let name = "Buddy";
  const messages: Message[] = [];
  let busy = false;

  const esc = (s: string) => s.replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]!);
  /** Escaped, with web links (a PR to look at) clickable. */
  const rich = (s: string) => esc(s).replace(/https:\/\/[^\s<]+[^\s<.,;:!?)]/g, (url) => `<a href="${url}">${url.length > 48 ? url.slice(0, 46) + "…" : url}</a>`);
  const render = (thinking = false) => {
    log.innerHTML =
      (messages.length ? "" : `<p class="hint">Ahoj, já jsem ${esc(name)}. Co je nového? Můžu ti něco připomenout, zapamatovat si, zadat práci v projektu, zapnout počítač nebo televizi. Klidně mi to řekni přes 🎙.</p>`) +
      messages
        .map((m, i) => {
          const buttons =
            m.reminder !== undefined && !m.answered
              ? `<span class="acts"><button data-done="${i}">Hotovo</button><button data-snooze="${i}" data-min="10">Za 10 min</button><button data-snooze="${i}" data-min="60">Za hodinu</button></span>`
              : "";
          const asks = (m.asks ?? [])
            .map((a) => `<span class="ask"><span>${esc(a.text)}</span><span class="acts"><button data-ask="${esc(a.id)}" data-yes="1" class="yes">Ano</button><button data-ask="${esc(a.id)}">Ne</button></span></span>`)
            .join("");
          return `<p class="${m.role}${m.reminder !== undefined ? " remind" : ""}">${m.role === "buddy" ? rich(m.text) : esc(m.text)}${buttons}${asks}</p>`;
        })
        .join("") +
      (thinking ? `<p class="buddy dots"><i></i><i></i><i></i></p>` : "");
    log.scrollTop = log.scrollHeight;
  };

  // ----- reminders: the clock in the header, and the ones that fire -----
  const clock = $<HTMLButtonElement>(".clock");
  const list = $(".list");
  const loadReminders = async () => {
    const all = await invoke<Reminder[]>("reminder_list").catch(() => []);
    clock.hidden = all.length === 0;
    clock.textContent = `⏰ ${all.length}`;
    list.innerHTML = all
      .map((r) => `<div class="item"><span><b>${esc(r.when)}</b>${r.repeat ? ` <small>${r.repeat === "daily" ? "každý den" : "všední dny"}</small>` : ""}<br>${esc(r.text)}</span><button data-cancel="${esc(r.id)}" title="Zrušit">✕</button></div>`)
      .join("");
    if (all.length === 0) list.hidden = true;
  };
  clock.addEventListener("click", () => {
    list.hidden = !list.hidden;
    $(".facts").hidden = true;
  });
  list.addEventListener("click", async (e) => {
    const id = (e.target as HTMLElement).closest<HTMLElement>("[data-cancel]")?.dataset.cancel;
    if (!id) return;
    await invoke("reminder_remove", { id });
    await loadReminders();
  });
  log.addEventListener("click", async (e) => {
    const t = e.target as HTMLElement;
    // Ano / Ne under a question: once, then the buttons go.
    const askId = t.dataset.ask;
    if (askId !== undefined) {
      const m = messages.find((m) => m.asks?.some((a) => a.id === askId));
      if (m) m.asks = m.asks!.filter((a) => a.id !== askId);
      render(true);
      const text = await invoke<string>("chat_confirm", { id: askId, yes: t.dataset.yes === "1" }).catch((err) => String(err));
      messages.push({ role: "buddy", text });
      render();
      return;
    }
    const done = t.dataset.done;
    const snooze = t.dataset.snooze;
    const i = Number(done ?? snooze);
    const m = messages[i];
    if (!m || m.reminder === undefined) return;
    m.answered = true;
    if (snooze !== undefined) {
      const minutes = Number(t.dataset.min);
      await invoke("reminder_snooze", { text: m.reminder, minutes });
      messages.push({ role: "buddy", text: minutes >= 60 ? "Dobře, ozvu se za hodinu." : `Dobře, ozvu se za ${minutes} minut.` });
      await loadReminders();
    } else {
      messages.push({ role: "buddy", text: Math.random() < 0.5 ? "Super, odškrtnuto." : "Hotovo, výborně." });
    }
    render();
  });

  // ----- what it remembers: the brain in the header -----
  const brain = $<HTMLButtonElement>(".brain");
  const facts = $(".facts");
  const loadFacts = async () => {
    const all = await invoke<Fact[]>("memory_list").catch(() => []);
    brain.hidden = all.length === 0;
    brain.textContent = `🧠 ${all.length}`;
    facts.innerHTML = all.map((f) => `<div class="item"><span>${esc(f.text)}</span><button data-forget="${esc(f.id)}" title="Zapomenout">✕</button></div>`).join("");
    if (all.length === 0) facts.hidden = true;
  };
  brain.addEventListener("click", () => {
    facts.hidden = !facts.hidden;
    list.hidden = true;
  });
  facts.addEventListener("click", async (e) => {
    const id = (e.target as HTMLElement).closest<HTMLElement>("[data-forget]")?.dataset.forget;
    if (!id) return;
    await invoke("memory_remove", { id });
    await loadFacts();
  });

  const showKeyForm = (error = "") => {
    keyForm.hidden = false;
    say.hidden = true;
    $(".err").textContent = error;
    keyInput.focus();
  };

  const refresh = async (focus = true) => {
    name = worn(await loadConfig())?.name ?? "Buddy";
    $(".who").textContent = name;
    render();
    if (!(await invoke<boolean>("gemini_key_present"))) showKeyForm();
    else {
      keyForm.hidden = true;
      say.hidden = false;
      if (focus) input.focus();
    }
    await loadReminders();
    await loadFacts();
  };

  const send = async (text: string) => {
    messages.push({ role: "me", text });
    busy = true;
    render(true);
    try {
      const answer = await invoke<Reply>("chat_send", { name, messages });
      messages.push({ role: "buddy", text: answer.text, asks: answer.asks.length ? answer.asks : undefined });
    } catch (err) {
      const why = String(err);
      if (why === "nokey" || why === "badkey") {
        messages.pop();
        input.value = text;
        showKeyForm(why === "badkey" ? "Tenhle klíč Gemini nebere. Zkontroluj ho, nebo vytvoř nový." : "");
      } else {
        messages.push({ role: "buddy", text: "Teď se mi nějak nedaří odpovědět. Zkusíš to za chvilku znovu?" });
        console.warn(why);
      }
    } finally {
      busy = false;
      render();
      input.focus();
    }
  };

  say.addEventListener("submit", (e) => {
    e.preventDefault();
    const text = input.value.trim();
    if (!text || busy) return;
    input.value = "";
    void send(text);
  });

  // ----- talking instead of typing -----
  const mic = $<HTMLButtonElement>(".mic");
  let listening = false;
  const stopListening = async (keep: boolean) => {
    if (!listening) return;
    listening = false;
    mic.classList.remove("on");
    input.placeholder = "Napiš mu…";
    if (!keep) {
      await invoke("listen_cancel");
      return;
    }
    busy = true;
    input.placeholder = "Poslouchám, co jsi řekl…";
    try {
      const heard = (await invoke<string>("listen_stop")).trim();
      busy = false;
      input.placeholder = "Napiš mu…";
      if (heard) await send(heard);
      else {
        messages.push({ role: "buddy", text: "Nic jsem neslyšel. Zkusíš to znovu? Mac se možná ptá, jestli smím k mikrofonu." });
        render();
      }
    } catch (err) {
      busy = false;
      input.placeholder = "Napiš mu…";
      const why = String(err);
      if (why === "nokey" || why === "badkey") showKeyForm(why === "badkey" ? "Tenhle klíč Gemini nebere. Zkontroluj ho, nebo vytvoř nový." : "");
      else messages.push({ role: "buddy", text: why.includes("ffmpeg") ? why : "Nerozuměl jsem, zkusíš to ještě jednou?" });
      render();
    }
  };
  mic.addEventListener("click", async () => {
    if (busy) return;
    if (listening) return void stopListening(true);
    try {
      await invoke("listen_start");
      listening = true;
      mic.classList.add("on");
      input.placeholder = "Mluv, pak klikni znovu…";
    } catch (err) {
      messages.push({ role: "buddy", text: String(err) });
      render();
    }
  });

  keyForm.addEventListener("submit", async (e) => {
    e.preventDefault();
    try {
      await invoke("gemini_key_set", { key: keyInput.value });
      keyInput.value = "";
      await refresh();
    } catch (err) {
      $(".err").textContent = String(err);
    }
  });

  // The link opens in the browser: the bubble is no place for a web page.
  document.addEventListener("click", (e) => {
    const a = (e.target as HTMLElement).closest("a");
    if (!a) return;
    e.preventDefault();
    void invoke("open_url", { url: a.href });
  });

  const close = () => {
    void stopListening(false);
    void invoke("voice_hush");
    void invoke("chat_close");
  };
  $(".x").addEventListener("click", close);
  document.addEventListener("keydown", (e) => {
    if (e.key === "Escape") close();
  });
  void listen<boolean>("chat-shown", (e) => void refresh(e.payload !== false));
  void listen("config-changed", () => void refresh(false));
  void listen("reminders-changed", () => void loadReminders());
  void listen("memory-changed", () => void loadFacts());
  // News from Wisp, finished work, the morning overview: in the buddy's own words.
  void listen<string>("say", (e) => {
    messages.push({ role: "buddy", text: e.payload });
    render();
  });
  void listen<Fired>("reminder", (e) => {
    const { text, late, bedtime } = e.payload;
    const lateNote = late >= 2 ? ` (měl jsem se ozvat před ${late} min, Mac spal)` : "";
    messages.push(bedtime ? { role: "buddy", text } : { role: "buddy", text: `⏰ ${text}${lateNote}`, reminder: text });
    render();
    void loadReminders();
  });

  await refresh();
}
