//! Talking to the buddy: the chat bubble's window, the Gemini key in the Keychain, and the
//! call to Gemini. The key never reaches a page; the bubble only sends the conversation and
//! gets the answer back.

use serde::Deserialize;
use serde_json::{json, Value};
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

fn system_prompt(name: &str) -> String {
    format!(
        "Jsi {name}, malá postavička z rodiny Wisp, která žije na ploše Macu. Chodíš po spodku obrazovky, \
občas si sedneš a v noci spíš. Člověk tě může chytit myší a hodit, a teď si s tebou píše.\n\
Odpovídej česky (anglicky jen když on píše anglicky), krátce, jednou až třemi větami, jako kamarád. \
Buď milý a trochu hravý, ale ne přeslazený. Žádné odrážky ani nadpisy, žádné emoji navíc.\n\
Když nevíš, řekni to. Nevymýšlej si, co nevidíš: nevidíš obrazovku, soubory ani co člověk dělá.\n\
Teď je {}.",
        now_text()
    )
}

/// One answer from Gemini to the conversation so far. Tries the next model when one is busy.
#[tauri::command]
pub async fn chat_send(app: AppHandle, name: String, messages: Vec<Message>) -> Result<String, String> {
    let key = key().ok_or("nokey")?;
    let mut body_messages = vec![json!({ "role": "system", "content": system_prompt(&name) })];
    // The last twenty turns are plenty for a chat bubble and keep each call small.
    let start = messages.len().saturating_sub(20);
    for m in &messages[start..] {
        let role = if m.role == "buddy" { "assistant" } else { "user" };
        body_messages.push(json!({ "role": role, "content": m.text }));
    }
    let client = reqwest::Client::builder().timeout(Duration::from_secs(40)).build().map_err(|e| e.to_string())?;
    let _ = app.emit_to("pet", "pet-react", "think");
    let mut last_err = String::new();
    for model in MODELS {
        let res = client
            .post(GEMINI_URL)
            .bearer_auth(&key)
            .json(&json!({ "model": model, "messages": body_messages, "reasoning_effort": "low" }))
            .send()
            .await;
        match res {
            Ok(r) if r.status().is_success() => {
                let v: Value = r.json().await.map_err(|e| e.to_string())?;
                let text = v["choices"][0]["message"]["content"].as_str().unwrap_or("").trim().to_string();
                if !text.is_empty() {
                    let _ = app.emit_to("pet", "pet-react", "talk");
                    return Ok(text);
                }
                last_err = format!("{model}: prázdná odpověď");
            }
            Ok(r) => {
                let status = r.status().as_u16();
                let detail = r.text().await.unwrap_or_default();
                // A wrong key will not get better with another model.
                if (status == 400 && detail.to_lowercase().contains("key")) || status == 401 || status == 403 {
                    let _ = app.emit_to("pet", "pet-react", "confused");
                    return Err("badkey".into());
                }
                last_err = format!("{model}: HTTP {status}");
            }
            Err(e) => last_err = format!("{model}: {e}"),
        }
    }
    let _ = app.emit_to("pet", "pet-react", "confused");
    Err(last_err)
}

/// Opens the bubble above the buddy (or brings it back), and tells the body to stand still.
pub fn open(app: &AppHandle) {
    crate::pet::set_talking(true);
    if let Some(w) = app.get_webview_window("chat") {
        let _ = w.show();
        let _ = w.set_focus();
        let _ = app.emit_to("chat", "chat-shown", ());
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
        .build();
    if let Ok(w) = made {
        let _ = w.set_focus();
    }
}

#[tauri::command]
pub fn chat_close(app: AppHandle) {
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
