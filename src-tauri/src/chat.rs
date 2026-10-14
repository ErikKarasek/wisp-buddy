//! Talking to the buddy: the chat bubble's window, the Gemini key in the Keychain, and the
//! call to Gemini. The key never reaches a page; the bubble only sends the conversation and
//! gets the answer back. While talking it can use tools: reminders, its memory, work for
//! Claude Code, the PC and the TV. What cannot be undone waits for a button in the bubble.

use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, WebviewUrl, WebviewWindowBuilder};

const KEYCHAIN_SERVICE: &str = "cz.erikkarasek.wispbuddy";
const KEYCHAIN_ACCOUNT: &str = "gemini";
const GEMINI_URL: &str = "https://generativelanguage.googleapis.com/v1beta/openai/chat/completions";
/// Newest first; each has its own free-tier limit, and the newest is often busy (503).
/// The 2.5 models are closed to new keys (checked 2026-10-03).
const MODELS: [&str; 3] = ["gemini-3.8-flash", "gemini-3.6-flash", "gemini-3.5-flash-lite"];
/// The bubble above the buddy, in logical points.
pub const BUBBLE_W: f64 = 300.0;
pub const BUBBLE_H: f64 = 340.0;

fn entry() -> Result<keyring::Entry, String> {
    keyring::Entry::new(KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT).map_err(|e| e.to_string())
}

fn key() -> Option<String> {
    entry().ok()?.get_password().ok().filter(|k| !k.trim().is_empty())
}

#[tauri::command]
pub fn gemini_key_set(key: String) -> Result<(), String> {
    let key = key.trim();
    if key.len() < 20 {
        return Err("Tohle nevypadá jako klíč.".into());
    }
    entry()?.set_password(key).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn gemini_key_present() -> bool {
    // Recording a video: no Keychain prompt in the middle of the clip (a new build asks again).
    if std::env::var("WISP_BUDDY_DEMO").is_ok_and(|v| v == "1") {
        return true;
    }
    key().is_some()
}

#[tauri::command]
pub fn gemini_key_forget() -> Result<(), String> {
    match entry()?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}

#[derive(Deserialize)]
pub struct Message {
    role: String,
    text: String,
}

/// Local time as "sobota 3. 10., 17:42", so it can say good night at the right hour.
fn now_text() -> String {
    let now = unsafe { libc::time(std::ptr::null_mut()) };
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    unsafe { libc::localtime_r(&now, &mut tm) };
    let days = ["neděle", "pondělí", "úterý", "středa", "čtvrtek", "pátek", "sobota"];
    format!("{} {}. {}., {}:{:02}", days[tm.tm_wday as usize % 7], tm.tm_mday, tm.tm_mon + 1, tm.tm_hour, tm.tm_min)
}

/// The name it goes by: the character it wears.
fn worn_name(app: &AppHandle) -> String {
    let cfg = crate::config::config_load(app.clone());
    let id = cfg["wearing"].as_str().unwrap_or("");
    cfg["characters"]
        .as_array()
        .and_then(|all| all.iter().find(|c| c["id"].as_str() == Some(id)).or(all.first()))
        .and_then(|c| c["name"].as_str())
        .map(String::from)
        .unwrap_or_else(|| "Buddy".into())
}

/// Who it is and what it can do, for every call.
pub fn persona(app: &AppHandle) -> String {
    system_prompt(app, &worn_name(app))
}

fn system_prompt(app: &AppHandle, name: &str) -> String {
    format!(
        "Jsi {name}, parťák z rodiny Wisp, který žije na ploše Macu a dělá osobního asistenta. \
Pamatuješ si, co ti člověk řekne, připomínáš mu, zadáváš za něj práci, otevíráš mu stránky a aplikace, \
zapínáš počítač a ovládáš televizi.\n\
Jak mluvíš:\n\
- Česky (anglicky jen když on píše anglicky), krátce, jednou až třemi větami.\n\
- Jdi hned na věc. Žádné vítání, žádné úvody, žádné \"jen tak se tu rozhlížím\".\n\
- Nekomentuj sám sebe, svoje tělo ani to, že chodíš po obrazovce. O sobě mluv jen když se na to zeptá.\n\
- Mluv jako normální člověk. Žádné odrážky, nadpisy, emoji ani zdrobněliny, nepřehrávej to a nebuď přeslazený.\n\
- Když něco udělá nástroj, řekni jednou větou, jak to dopadlo, a dost.\n\
- Odpovědi se čtou nahlas, tak piš, aby se daly vyslovit.\n\
Když nevíš, řekni to. Nevymýšlej si, co nevidíš: nevidíš obrazovku, soubory ani co člověk dělá. \
Když nástroj vrátí chybu, řekni po lidsku, co chybí nebo co má člověk udělat.\n\
Co umíš nástroji:\n\
- Stránky a aplikace: open_url otevře odkaz v prohlížeči, open_app spustí aplikaci na Macu. Když chce \
\"otevřít záložky\" nebo víc stránek, zavolej open_url pro každou z nich. Když nevíš přesnou adresu, \
použij hledání na Googlu. Co ti neřekl, si nedomýšlej.\n\
- Připomínky: set_reminder, list_reminders, cancel_reminder. Čas počítej od teď. Když neřekne přesný čas \
(\"odpoledne\", \"večer\"), zeptej se.\n\
- Paměť: když řekne něco, co se bude hodit i příště (kdo je, na čem pracuje, co plánuje, co má rád, jak se co jmenuje), \
ulož to nástrojem remember jednou krátkou větou. Když řekne, ať něco zapomeneš, nebo to už neplatí, použij forget. \
Drobnosti z rozhovoru si nepiš.\n\
- Práce: start_work zadá úkol Claude Codu v jeho projektu v ~/Developer (dělá ho noční směna Wispu ve vlastní větvi \
a výsledek je PR ke kontrole). Úkol napiš celý a srozumitelně, jako by ho zadával on. Spustí se, až ho potvrdí tlačítkem \
pod tvou odpovědí, tak mu to řekni. Hned (now) jen když chce hned, jinak se to udělá v noci nebo až bude pryč. \
work_status ukáže frontu a poslední výsledky.\n\
- Počítač s Windows: pc (wake = zapnout, status). Vypnout, restartovat nebo uspat přes pc_power, to taky potvrdí tlačítkem.\n\
- Televize Samsung: tv (on, off, status, volume, key, app jako YouTube nebo Netflix).\n\
- Nastavení domácnosti: find_devices najde zařízení v síti, home_setup uloží MAC a IP adresu nebo SSH přihlášení k PC.\n\
- briefing: přehled dne (dnešní připomínky, co udělala noční směna, co chtějí agenti). Použij ho, když se ptá, co ho dnes čeká.\n\
Teď je {} ({}).{}{}",
        now_text(),
        crate::reminders::now_iso(),
        crate::memory::for_chat(app).map(|m| format!("\n{m}")).unwrap_or_default(),
        crate::wisp::summary_for_chat().map(|w| format!("\n{w} Když se zeptá na agenty, odpověz z tohohle.")).unwrap_or_default()
    )
}

fn tool(name: &str, description: &str, parameters: Value) -> Value {
    json!({ "type": "function", "function": { "name": name, "description": description, "parameters": parameters } })
}

fn no_params() -> Value {
    json!({ "type": "object", "properties": {} })
}

/// What the buddy can do while talking.
fn tools() -> Value {
    let keys: Vec<&str> = crate::home::KEYS.iter().map(|(k, _)| *k).collect();
    let apps: Vec<&str> = crate::home::APPS.iter().map(|(a, _)| *a).collect();
    json!([
        tool("open_url", "Open a web page in the user's browser, in a new tab. Call it once per page to open several.", json!({
            "type": "object",
            "properties": { "url": { "type": "string", "description": "Full http(s) address, e.g. https://github.com/notifications" } },
            "required": ["url"]
        })),
        tool("open_app", "Start an app on the Mac by its name, e.g. Spotify, Terminal, Visual Studio Code.", json!({
            "type": "object",
            "properties": { "name": { "type": "string", "description": "The app's name as it is in Applications" } },
            "required": ["name"]
        })),
        tool("set_reminder", "Save a reminder. The buddy wakes up at that time and shows the text in its bubble.", json!({
            "type": "object",
            "properties": {
                "when": { "type": "string", "description": "Local date and time, YYYY-MM-DDTHH:MM. Work it out from the current time in the instructions." },
                "text": { "type": "string", "description": "What to remind of, short, in the user's words and language" },
                "repeat": { "type": "string", "enum": ["none", "daily", "weekdays"] }
            },
            "required": ["when", "text"]
        })),
        tool("list_reminders", "The reminders that are set, with their ids and times.", no_params()),
        tool("cancel_reminder", "Delete a reminder by its id (from list_reminders).", json!({ "type": "object", "properties": { "id": { "type": "string" } }, "required": ["id"] })),
        tool("remember", "Keep a fact about the user for all future conversations.", json!({
            "type": "object",
            "properties": { "text": { "type": "string", "description": "One short sentence in Czech, e.g. 'Hledá práci jako frontend vývojář.'" } },
            "required": ["text"]
        })),
        tool("forget", "Drop a remembered fact by its id (shown in brackets in the instructions).", json!({ "type": "object", "properties": { "id": { "type": "string" } }, "required": ["id"] })),
        tool("start_work", "Ask Claude Code to do a coding task in one of the user's projects (through Wisp's night shift: own git branch, draft PR). Needs the user's confirmation button.", json!({
            "type": "object",
            "properties": {
                "project": { "type": "string", "description": "Project folder name in ~/Developer, or close to it" },
                "task": { "type": "string", "description": "The whole task, clear enough to do without asking" },
                "now": { "type": "boolean", "description": "Start at once. Otherwise it starts at night or when the user is away." }
            },
            "required": ["project", "task"]
        })),
        tool("work_status", "The night shift's queue, what runs, recent results with PR links, and the project list.", no_params()),
        tool("pc", "The Windows PC: wake it up (Wake-on-LAN) or check whether it is on.", json!({
            "type": "object", "properties": { "action": { "type": "string", "enum": ["wake", "status"] } }, "required": ["action"]
        })),
        tool("pc_power", "Shut down, restart or put the PC to sleep (over SSH). Needs the user's confirmation button.", json!({
            "type": "object", "properties": { "action": { "type": "string", "enum": ["shutdown", "restart", "sleep"] } }, "required": ["action"]
        })),
        tool("tv", "The Samsung TV on the home network.", json!({
            "type": "object",
            "properties": {
                "action": { "type": "string", "enum": ["on", "off", "status", "volume", "key", "app"] },
                "times": { "type": "integer", "description": "volume: steps, negative is quieter (default 3); key: how many presses" },
                "key": { "type": "string", "enum": keys },
                "app": { "type": "string", "enum": apps }
            },
            "required": ["action"]
        })),
        tool("find_devices", "Devices on the home network the Mac knows of, with a guess at what each is (Samsung TV, Windows).", no_params()),
        tool("home_setup", "Save the PC's or the TV's address, or the SSH login for the PC (user@address).", json!({
            "type": "object",
            "properties": {
                "device": { "type": "string", "enum": ["pc", "tv"] },
                "mac": { "type": "string" },
                "ip": { "type": "string" },
                "ssh": { "type": "string" }
            },
            "required": ["device"]
        })),
        tool("briefing", "Facts for an overview of the day: today's reminders, night shift results, Wisp's agents.", no_params())
    ])
}

// ---------- things that wait for a yes ----------

#[derive(Clone)]
enum Action {
    Work { project: String, task: String, now: bool },
    PcPower(String),
}

/// A question with Ano/Ne buttons under the buddy's answer.
#[derive(Serialize, Clone)]
pub struct Ask {
    id: String,
    text: String,
}

static PENDING: Mutex<Vec<(String, Action)>> = Mutex::new(Vec::new());

fn ask(action: Action, text: String) -> Ask {
    let id = crate::reminders::new_id();
    let mut p = PENDING.lock().unwrap_or_else(|e| e.into_inner());
    // A handful is plenty; old unanswered ones go.
    if p.len() >= 10 {
        p.remove(0);
    }
    p.push((id.clone(), action));
    Ask { id, text }
}

/// The answer to a button: does it, or lets it be.
#[tauri::command]
pub async fn chat_confirm(app: AppHandle, id: String, yes: bool) -> Result<String, String> {
    let action = {
        let mut p = PENDING.lock().unwrap_or_else(|e| e.into_inner());
        let i = p.iter().position(|(pid, _)| *pid == id).ok_or("Tohle už neplatí.")?;
        p.remove(i).1
    };
    if !yes {
        return Ok("Dobře, nechám to být.".into());
    }
    let out = tauri::async_runtime::spawn_blocking(move || match action {
        Action::Work { project, task, now } => crate::work::add(&project, &task, now).map(|_| {
            if now {
                format!("Zadáno, v {project} se do minuty pustí do práce. Ozvu se, až to bude hotové.")
            } else {
                format!("Zadáno pro {project}. Udělá se to v noci nebo až budeš pryč, ráno ti řeknu, jak to dopadlo.")
            }
        }),
        Action::PcPower(what) => crate::home::pc_power(&app, &what),
    })
    .await
    .map_err(|e| e.to_string())?;
    if let Ok(line) = &out {
        crate::voice::speak(line);
    }
    out
}

/// Runs one tool the model asked for and says how it went, for the model to read. A tool that
/// needs a yes leaves a question for the bubble instead of acting.
fn run_tool(app: &AppHandle, name: &str, args: &Value, asks: &mut Vec<Ask>) -> Value {
    let reminders = crate::reminders::reminder_list;
    let s = |k: &str| args[k].as_str().map(str::trim).filter(|v| !v.is_empty());
    let waiting = json!({ "waiting_for_confirmation": true, "note": "Buttons Ano/Ne are shown under your answer. Say in a few words what will happen and that they confirm with the button." });
    match name {
        "set_reminder" => {
            let when = args["when"].as_str().unwrap_or("");
            let text = args["text"].as_str().unwrap_or("").trim();
            let Some(at) = crate::reminders::parse_local(when) else { return json!({ "error": "when must be YYYY-MM-DDTHH:MM" }) };
            if text.is_empty() {
                return json!({ "error": "text is empty" });
            }
            if at < crate::reminders::now_ms() - 60_000 {
                return json!({ "error": "that time has already passed" });
            }
            let repeat = args["repeat"].as_str().filter(|r| *r != "none").map(String::from);
            let r = crate::reminders::add(app, at, text, repeat);
            let _ = app.emit("reminders-changed", ());
            json!({ "ok": true, "id": r.id, "when": crate::reminders::describe(r.at), "repeat": r.repeat })
        }
        "open_url" => match s("url") {
            Some(url) => crate::open_in_browser(url).map_or_else(|e| json!({ "error": e }), |()| json!({ "ok": true, "opened": url })),
            None => json!({ "error": "url is empty" }),
        },
        "open_app" => match s("name") {
            Some(name) => crate::open_mac_app(name).map_or_else(|e| json!({ "error": e }), |()| json!({ "ok": true, "opened": name })),
            None => json!({ "error": "name is empty" }),
        },
        "list_reminders" => json!({ "reminders": reminders(app.clone()) }),
        "cancel_reminder" => {
            let gone = crate::reminders::remove(app, args["id"].as_str().unwrap_or(""));
            let _ = app.emit("reminders-changed", ());
            json!({ "ok": gone })
        }
        "remember" => match s("text") {
            Some(text) => {
                let f = crate::memory::add(app, text);
                let _ = app.emit("memory-changed", ());
                json!({ "ok": true, "id": f.id })
            }
            None => json!({ "error": "text is empty" }),
        },
        "forget" => {
            let gone = crate::memory::remove(app, s("id").unwrap_or(""));
            let _ = app.emit("memory-changed", ());
            json!({ "ok": gone })
        }
        "start_work" => {
            let Some(task) = s("task").filter(|t| t.chars().count() >= 5) else { return json!({ "error": "say what to do" }) };
            let Some(project) = s("project").and_then(crate::work::find_project) else {
                return json!({ "error": "no such project (or more than one fits)", "projects": crate::work::projects() });
            };
            let now = args["now"].as_bool().unwrap_or(false);
            let when = if now { " hned" } else { " v noci nebo až budeš pryč" };
            asks.push(ask(Action::Work { project: project.clone(), task: task.to_string(), now }, format!("Pustit Claude Code do {project}{when}: „{task}“?")));
            waiting
        }
        "work_status" => crate::work::status(),
        "pc" => crate::home::pc(app, s("action").unwrap_or("")),
        "pc_power" => {
            let what = s("action").unwrap_or("");
            let label = match what {
                "shutdown" => "Vypnout počítač",
                "restart" => "Restartovat počítač",
                "sleep" => "Uspat počítač",
                _ => return json!({ "error": "action is shutdown, restart or sleep" }),
            };
            if let Err(e) = crate::home::pc_power_ready(app) {
                return json!({ "error": e });
            }
            asks.push(ask(Action::PcPower(what.into()), format!("{label}?")));
            waiting
        }
        "tv" => crate::home::tv(app, s("action").unwrap_or(""), s("key"), s("app"), args["times"].as_i64()),
        "find_devices" => crate::home::find_devices(app),
        "home_setup" => crate::home::setup(app, s("device").unwrap_or(""), s("mac"), s("ip"), s("ssh")),
        "briefing" => crate::briefing::facts(app),
        _ => json!({ "error": "no such tool" }),
    }
}

/// The lightest model, for the small jobs (writing down a recording) where the big one only
/// costs time.
const LIGHTEST: usize = MODELS.len() - 1;
/// Which model last answered. Tried first next time, so a newest model that is busy today is
/// not paid for with a failed round trip on every single question.
static LAST_GOOD: Mutex<Option<usize>> = Mutex::new(None);

fn models_from(first: usize) -> Vec<usize> {
    let mut all: Vec<usize> = (0..MODELS.len()).collect();
    all.sort_by_key(|i| (*i != first, *i));
    all
}

/// Folds one streamed piece into the message being rebuilt: text and tool arguments grow, tool
/// calls line up by their index, and everything else the model sends is kept as it came. Gemini 3
/// wants its own thought signatures handed back, and they do not all sit on the tool calls.
fn merge_delta(key: &str, acc: &mut Value, delta: &Value) {
    // Only these two arrive in pieces; an id or a name repeated in a later piece is the same
    // id, not more of it.
    let grows = matches!(key, "content" | "arguments" | "reasoning_content");
    match (acc, delta) {
        (Value::String(have), Value::String(piece)) if grows => have.push_str(piece),
        (Value::Object(have), Value::Object(piece)) => {
            for (k, v) in piece {
                if v.is_null() {
                    continue;
                }
                match have.get_mut(k) {
                    Some(slot) => merge_delta(k, slot, v),
                    None => {
                        have.insert(k.clone(), v.clone());
                    }
                }
            }
        }
        (Value::Array(have), Value::Array(pieces)) => {
            for piece in pieces {
                let at = piece.get("index").and_then(Value::as_u64).map_or(have.len(), |i| i as usize);
                while have.len() <= at {
                    have.push(json!({}));
                }
                merge_delta(key, &mut have[at], piece);
            }
        }
        (slot, piece) => *slot = piece.clone(),
    }
}

/// The whole lines waiting in `raw`, taken out of it. A chunk from the network can end in the
/// middle of a č, so the bytes are kept until the newline that completes the line: turning each
/// chunk into text on its own would mangle every Czech letter that straddles two of them.
fn whole_lines(raw: &mut Vec<u8>) -> Vec<String> {
    let mut lines = vec![];
    while let Some(nl) = raw.iter().position(|b| *b == b'\n') {
        let line: Vec<u8> = raw.drain(..=nl).collect();
        lines.push(String::from_utf8_lossy(&line[..line.len() - 1]).into_owned());
    }
    lines
}

/// Why one model did not answer.
enum Fail {
    /// The key itself: no other model will help.
    Key,
    /// This model: the next one is worth a try.
    Model,
    /// Nothing came out of the stream, whatever the reason. Worth one plain, unstreamed try
    /// before giving up on the model.
    Stream,
}

/// Asks one model.
async fn ask_model<'sink>(
    client: &reqwest::Client,
    key: &str,
    model: &str,
    history: &[Value],
    tools: Option<&Value>,
    on_delta: Option<&mut (dyn FnMut(&str) + Send + 'sink)>,
) -> Result<Value, (Fail, String)> {
    let streaming = on_delta.is_some();
    let mut body = json!({ "model": model, "messages": history, "reasoning_effort": "low" });
    if let Some(t) = tools {
        body["tools"] = t.clone();
    }
    if streaming {
        body["stream"] = json!(true);
    }
    let soft = |e: String| (Fail::Model, format!("{model}: {e}"));
    let mut r = client.post(GEMINI_URL).bearer_auth(key).json(&body).send().await.map_err(|e| soft(e.to_string()))?;
    if !r.status().is_success() {
        let status = r.status().as_u16();
        let detail = r.text().await.unwrap_or_default();
        // A wrong key will not get better with another model.
        if (status == 400 && detail.to_lowercase().contains("key")) || status == 401 || status == 403 {
            return Err((Fail::Key, "badkey".into()));
        }
        return Err(soft(format!("HTTP {status}")));
    }
    let message = match on_delta {
        None => {
            let v: Value = r.json().await.map_err(|e| soft(e.to_string()))?;
            v["choices"][0]["message"].clone()
        }
        Some(on_delta) => {
            let mut message = json!({});
            let mut raw: Vec<u8> = Vec::new();
            while let Some(chunk) = r.chunk().await.map_err(|e| soft(e.to_string()))? {
                raw.extend_from_slice(&chunk);
                for line in whole_lines(&mut raw) {
                    let Some(data) = line.trim().strip_prefix("data:") else { continue };
                    let data = data.trim();
                    if data.is_empty() || data == "[DONE]" {
                        continue;
                    }
                    let Ok(v) = serde_json::from_str::<Value>(data) else { continue };
                    let delta = &v["choices"][0]["delta"];
                    if !delta.is_object() {
                        continue;
                    }
                    if let Some(piece) = delta["content"].as_str().filter(|p| !p.is_empty()) {
                        on_delta(piece);
                    }
                    merge_delta("", &mut message, delta);
                }
            }
            message
        }
    };
    let nothing = message["content"].as_str().is_none_or(|t| t.trim().is_empty()) && message["tool_calls"].as_array().is_none_or(|c| c.is_empty());
    if nothing {
        let why = if streaming { Fail::Stream } else { Fail::Model };
        return Err((why, format!("{model}: prázdná odpověď")));
    }
    Ok(message)
}

/// One reply from Gemini to `history`: the model's message, which may ask for tools. With
/// `on_delta` the answer is streamed and each new piece of its text goes there the moment it
/// lands, so the bubble and the voice do not wait for the last word. Tries the next model when
/// one is busy. "nokey" and "badkey" mean the key, not the model.
async fn gemini<'sink>(history: &[Value], tools: Option<&Value>, first: usize, mut on_delta: Option<&mut (dyn FnMut(&str) + Send + 'sink)>) -> Result<Value, String> {
    let key = key().ok_or("nokey")?;
    let client = reqwest::Client::builder().timeout(Duration::from_secs(40)).build().map_err(|e| e.to_string())?;
    let mut last_err = String::new();
    let good = |i: usize| *LAST_GOOD.lock().unwrap_or_else(|e| e.into_inner()) = Some(i);
    for i in models_from(first) {
        match ask_model(&client, &key, MODELS[i], history, tools, on_delta.as_deref_mut()).await {
            Ok(message) => {
                good(i);
                return Ok(message);
            }
            Err((Fail::Key, fatal)) => return Err(fatal),
            Err((Fail::Model, e)) => last_err = e,
            // The stream gave nothing. Before writing the model off, ask it the plain way:
            // a streamed answer is nicer, an answer is the point.
            Err((Fail::Stream, _)) => {
                match ask_model(&client, &key, MODELS[i], history, tools, None).await {
                    Ok(message) => {
                        good(i);
                        return Ok(message);
                    }
                    Err((Fail::Key, fatal)) => return Err(fatal),
                    Err((_, e)) => last_err = e,
                }
            }
        }
    }
    Err(last_err)
}

/// The model to try first.
fn preferred() -> usize {
    LAST_GOOD.lock().unwrap_or_else(|e| e.into_inner()).unwrap_or(0)
}

/// One reply, waited for whole.
pub async fn complete(history: &[Value], tools: Option<&Value>) -> Result<Value, String> {
    gemini(history, tools, preferred(), None).await
}

/// The buddy's answer, and any questions with buttons that came up on the way.
#[derive(Serialize)]
pub struct Reply {
    text: String,
    asks: Vec<Ask>,
}

/// One answer from Gemini to the conversation so far, after any tools it used along the way.
#[tauri::command]
pub async fn chat_send(app: AppHandle, name: String, messages: Vec<Message>) -> Result<Reply, String> {
    if key().is_none() {
        return Err("nokey".into());
    }
    let mut turn = crate::voice::begin();
    let mut history = vec![json!({ "role": "system", "content": system_prompt(&app, &name) })];
    // The last twenty turns are plenty for a chat bubble and keep each call small.
    let start = messages.len().saturating_sub(20);
    for m in &messages[start..] {
        let role = if m.role == "buddy" { "assistant" } else { "user" };
        history.push(json!({ "role": role, "content": m.text }));
    }
    let _ = app.emit_to("pet", "pet-react", "think");
    let tools = tools();
    let mut asks = vec![];
    // A few rounds: the model may use a tool, read the result, then answer.
    for _ in 0..5 {
        // The answer as it is being written: shown in the bubble and said out loud sentence by
        // sentence, so neither waits for the last word.
        let mut written = String::new();
        let mut unsaid = String::new();
        let message = {
            let mut on_delta = |piece: &str| {
                if written.is_empty() {
                    let _ = app.emit_to("pet", "pet-react", "talk");
                }
                written.push_str(piece);
                let _ = app.emit_to("chat", "chat-delta", piece);
                unsaid.push_str(piece);
                while let Some(sentence) = crate::voice::next_sentence(&mut unsaid, false) {
                    crate::voice::speak_part(turn, &sentence);
                }
            };
            match gemini(&history, Some(&tools), preferred(), Some(&mut on_delta)).await {
                Ok(m) => m,
                Err(e) => {
                    crate::voice::hush();
                    let _ = app.emit_to("chat", "chat-undelta", ());
                    let _ = app.emit_to("pet", "pet-react", "confused");
                    return Err(e);
                }
            }
        };
        let calls = message["tool_calls"].as_array().cloned().unwrap_or_default();
        if calls.is_empty() {
            if let Some(last) = crate::voice::next_sentence(&mut unsaid, true) {
                crate::voice::speak_part(turn, &last);
            }
            let text = match written.trim() {
                "" => message["content"].as_str().unwrap_or("").trim().to_string(),
                streamed => streamed.to_string(),
            };
            // Nothing was streamed (an older model, or no stream at all): say it in one go.
            if written.trim().is_empty() {
                crate::voice::speak(&text);
            }
            let _ = app.emit_to("pet", "pet-react", "talk");
            let _ = app.emit_to("chat", "chat-undelta", ());
            return Ok(Reply { text, asks });
        }
        // It wants a tool after all. Whatever it wrote before that was thinking out loud, not
        // the answer: take it back off the page and stop saying it.
        if !written.trim().is_empty() {
            turn = crate::voice::begin();
            let _ = app.emit_to("chat", "chat-undelta", ());
        }
        let _ = app.emit_to("pet", "pet-react", "think");
        // Back whole, with every field it came with: Gemini 3 wants its own thought signatures
        // returned, and they do not all sit on the tool calls. Rebuilding the message from
        // content and tool_calls alone drops them, and the next round is then refused.
        let mut echo = message.clone();
        echo["role"] = json!("assistant");
        history.push(echo);
        for call in &calls {
            let name = call["function"]["name"].as_str().unwrap_or("").to_string();
            let args: Value = match &call["function"]["arguments"] {
                Value::String(s) => serde_json::from_str(s).unwrap_or(Value::Null),
                other => other.clone(),
            };
            // The home network and SSH can take seconds; not on the async runtime's threads.
            let app2 = app.clone();
            let (result, more) = tauri::async_runtime::spawn_blocking(move || {
                let mut asks = vec![];
                let r = run_tool(&app2, &name, &args, &mut asks);
                (r, asks)
            })
            .await
            .unwrap_or_else(|e| (json!({ "error": e.to_string() }), vec![]));
            asks.extend(more);
            history.push(json!({ "role": "tool", "tool_call_id": call["id"], "content": result.to_string() }));
        }
    }
    let _ = app.emit_to("pet", "pet-react", "confused");
    Err("too many rounds".into())
}

/// What was said in a recording, written down by Gemini.
pub async fn transcribe(app: &AppHandle, wav: &[u8]) -> Result<String, String> {
    let _ = app.emit_to("pet", "pet-react", "think");
    let audio = base64::engine::general_purpose::STANDARD.encode(wav);
    let history = [
        json!({ "role": "system", "content": "Přepiš přesně, co člověk v nahrávce říká, jazykem, kterým mluví (většinou česky). \
Vrať jen samotný přepis s interpunkcí, bez uvozovek a komentářů. Když v nahrávce není žádná řeč, vrať jen znak -." }),
        json!({ "role": "user", "content": [
            { "type": "text", "text": "Přepis:" },
            { "type": "input_audio", "input_audio": { "data": audio, "format": "wav" } }
        ] }),
    ];
    let m = gemini(&history, None, LIGHTEST, None).await?;
    let text = m["content"].as_str().unwrap_or("").trim().trim_matches(['"', '„', '“']).trim().to_string();
    Ok(if text == "-" { String::new() } else { text })
}

/// The buddy says something of its own (news from Wisp, work that finished, the morning): a
/// reaction, and the line in the bubble, shown without taking the keyboard, and out loud when
/// someone is there to hear it.
pub fn say(app: &AppHandle, text: &str, reaction: &str) {
    crate::voice::speak_if_here(text);
    crate::pet::wake_for_reminder();
    let _ = app.emit_to("pet", "pet-react", reaction);
    open_quietly(app);
    let (app, text) = (app.clone(), text.to_string());
    std::thread::spawn(move || {
        // The bubble may have just been made: give its page a moment to listen.
        std::thread::sleep(Duration::from_millis(700));
        let _ = app.emit_to("chat", "say", text);
    });
}

/// Opens the bubble above the buddy (or brings it back), and tells the body to stand still.
pub fn open(app: &AppHandle) {
    show(app, true);
}

/// For a reminder: the bubble appears but leaves the keyboard where it is, so nobody's typing
/// lands in it by surprise.
pub fn open_quietly(app: &AppHandle) {
    show(app, false);
}

fn show(app: &AppHandle, focus: bool) {
    crate::pet::set_talking(true);
    if let Some(w) = app.get_webview_window("chat") {
        let _ = w.show();
        if focus {
            let _ = w.set_focus();
        }
        let _ = app.emit_to("chat", "chat-shown", focus);
        return;
    }
    let made = WebviewWindowBuilder::new(app, "chat", WebviewUrl::App("index.html?view=chat".into()))
        .title("Wisp Buddy")
        .inner_size(BUBBLE_W, BUBBLE_H)
        .decorations(false)
        .transparent(true)
        .always_on_top(true)
        .skip_taskbar(true)
        .shadow(false)
        .resizable(false)
        .visible_on_all_workspaces(true)
        .position(-2000.0, -2000.0)
        .focused(focus)
        .build();
    if let (Ok(w), true) = (made, focus) {
        let _ = w.set_focus();
    }
}

#[tauri::command]
pub fn chat_close(app: AppHandle) {
    // Closing the bubble ends the answer: no talking on to a shut window.
    crate::voice::hush();
    crate::pet::set_talking(false);
    if let Some(w) = app.get_webview_window("chat") {
        let _ = w.hide();
    }
}

/// Keep the bubble just above the buddy's head, inside the screen.
pub fn follow(app: &AppHandle, x: f64, y: f64, size: f64, area: (f64, f64, f64, f64, f64)) {
    let Some(w) = app.get_webview_window("chat") else { return };
    if !w.is_visible().unwrap_or(false) {
        return;
    }
    let (left, top, right, _bottom, scale) = area;
    let bx = (x + size / 2.0 - BUBBLE_W / 2.0).clamp(left + 6.0, right - BUBBLE_W - 6.0);
    let by = (y - BUBBLE_H + size * 0.18).max(top + 6.0);
    let _ = w.set_position(PhysicalPosition::new((bx * scale).round() as i32, (by * scale).round() as i32));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Feeds a message in the pieces a stream would send it in.
    fn streamed(pieces: &[Value]) -> Value {
        let mut message = json!({});
        for piece in pieces {
            merge_delta("", &mut message, piece);
        }
        message
    }

    #[test]
    fn the_answer_grows_out_of_its_pieces() {
        let message = streamed(&[
            json!({ "role": "assistant", "content": "Jasně, " }),
            json!({ "content": "otevřeno." }),
            json!({ "content": "" }),
        ]);
        assert_eq!(message["content"], json!("Jasně, otevřeno."));
        // Said once, not three times over.
        assert_eq!(message["role"], json!("assistant"));
    }

    #[test]
    fn a_tool_call_arrives_in_pieces_and_comes_out_whole() {
        let message = streamed(&[
            json!({ "role": "assistant", "tool_calls": [{ "index": 0, "id": "call_1", "type": "function", "function": { "name": "open_url", "arguments": "{\"url\":" } }] }),
            json!({ "tool_calls": [{ "index": 0, "function": { "arguments": "\"https://github.com\"}" } }] }),
        ]);
        let call = &message["tool_calls"][0];
        // An id repeated by the stream is the same id, not twice as long a one.
        assert_eq!(call["id"], json!("call_1"));
        assert_eq!(call["function"]["name"], json!("open_url"));
        assert_eq!(call["function"]["arguments"], json!("{\"url\":\"https://github.com\"}"));
    }

    #[test]
    fn two_tools_at_once_do_not_land_on_top_of_each_other() {
        let message = streamed(&[
            json!({ "tool_calls": [{ "index": 0, "id": "a", "function": { "name": "open_url", "arguments": "{}" } }] }),
            json!({ "tool_calls": [{ "index": 1, "id": "b", "function": { "name": "open_app", "arguments": "{}" } }] }),
        ]);
        assert_eq!(message["tool_calls"].as_array().map(Vec::len), Some(2));
        assert_eq!(message["tool_calls"][0]["function"]["name"], json!("open_url"));
        assert_eq!(message["tool_calls"][1]["function"]["name"], json!("open_app"));
    }

    #[test]
    fn whatever_else_the_model_sends_is_kept() {
        // Gemini 3 wants its own thought signatures handed back, and they do not all sit on the
        // tool calls. Rebuilding only content and tool_calls drops them and the next round is refused.
        let message = streamed(&[
            json!({ "role": "assistant", "extra_content": { "google": { "thought_signature": "sig-1" } } }),
            json!({ "content": "Hotovo." }),
        ]);
        assert_eq!(message["extra_content"]["google"]["thought_signature"], json!("sig-1"));
    }

    #[test]
    fn a_czech_letter_split_across_two_chunks_survives() {
        let mut raw: Vec<u8> = Vec::new();
        let whole = "data: {\"t\":\"říkám\"}\n".as_bytes().to_vec();
        // Cut inside the ř: its first byte ends up in one chunk, its second in the next.
        let (first, second) = whole.split_at(13);
        raw.extend_from_slice(first);
        // Nothing is a whole line yet.
        assert!(whole_lines(&mut raw).is_empty());
        raw.extend_from_slice(second);
        assert_eq!(whole_lines(&mut raw), vec!["data: {\"t\":\"říkám\"}".to_string()]);
        assert!(raw.is_empty());
    }

    #[test]
    fn the_model_that_worked_is_asked_first_next_time() {
        assert_eq!(models_from(0), vec![0, 1, 2]);
        assert_eq!(models_from(2), vec![2, 0, 1]);
    }
}
