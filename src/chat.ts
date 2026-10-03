// The chat bubble above the buddy. The conversation lives here; Rust holds the Gemini key and
// makes the call (src-tauri/src/chat.rs), so the key never reaches this page.

import "./chat.css";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { loadConfig, worn } from "./config";

type Message = { role: "me" | "buddy"; text: string };

const AI_STUDIO = "https://aistudio.google.com/apikey";

export async function startChat() {
  document.body.className = "chat";
  document.body.innerHTML = `
    <div class="bubble">
      <header><b class="who"></b><button class="x" title="Zavřít (Esc)">✕</button></header>
      <div class="log"></div>
      <form class="say"><input type="text" placeholder="Napiš mu…" spellcheck="false" autocomplete="off"><button>➤</button></form>
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
  const render = (thinking = false) => {
    log.innerHTML =
      (messages.length ? "" : `<p class="hint">Ahoj, já jsem ${esc(name)}. Co je nového?</p>`) +
      messages.map((m) => `<p class="${m.role}">${esc(m.text)}</p>`).join("") +
      (thinking ? `<p class="buddy dots"><i></i><i></i><i></i></p>` : "");
    log.scrollTop = log.scrollHeight;
  };

  const showKeyForm = (error = "") => {
    keyForm.hidden = false;
    say.hidden = true;
    $(".err").textContent = error;
    keyInput.focus();
  };

  const refresh = async () => {
    name = worn(await loadConfig())?.name ?? "Buddy";
    $(".who").textContent = name;
    render();
    if (!(await invoke<boolean>("gemini_key_present"))) showKeyForm();
    else {
      keyForm.hidden = true;
      say.hidden = false;
      input.focus();
    }
  };

  say.addEventListener("submit", async (e) => {
    e.preventDefault();
    const text = input.value.trim();
    if (!text || busy) return;
    input.value = "";
    messages.push({ role: "me", text });
    busy = true;
    render(true);
    try {
      const answer = await invoke<string>("chat_send", { name, messages });
      messages.push({ role: "buddy", text: answer });
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

  const close = () => void invoke("chat_close");
  $(".x").addEventListener("click", close);
  document.addEventListener("keydown", (e) => {
    if (e.key === "Escape") close();
  });
  void listen("chat-shown", () => void refresh());
  void listen("config-changed", () => void refresh());

  await refresh();
}
