//! The link to Wisp, when it runs on the same Mac: every few seconds the buddy asks Wisp's
//! local server who works, who failed and who waits (GET /buddy/state, with the key Wisp keeps
//! for its hooks), and reacts when something changes. Without Wisp nothing happens.

use serde_json::Value;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Emitter};

const PORT: u16 = 47811;
static ON: AtomicBool = AtomicBool::new(true);
/// The last summary, for the chat to know what the agents are up to.
static LATEST: Mutex<Option<Value>> = Mutex::new(None);

pub fn set_on(on: bool) {
    ON.store(on, Ordering::Relaxed);
    if !on {
        if let Ok(mut l) = LATEST.lock() {
            *l = None;
        }
    }
}

fn key() -> Option<String> {
    let home = std::env::var("HOME").ok()?;
    let k = std::fs::read_to_string(format!("{home}/Library/Application Support/cz.erikkarasek.dispecink/cc-hook-key")).ok()?;
    let k = k.trim().to_string();
    (k.len() >= 32).then_some(k)
}

/// A plain GET to Wisp on this Mac; nothing to install for one local request.
fn fetch(key: &str) -> Option<Value> {
    let mut s = TcpStream::connect_timeout(&([127, 0, 0, 1], PORT).into(), Duration::from_millis(500)).ok()?;
    s.set_read_timeout(Some(Duration::from_secs(2))).ok()?;
    write!(s, "GET /buddy/state?k={key} HTTP/1.1\r\nHost: 127.0.0.1:{PORT}\r\nConnection: close\r\n\r\n").ok()?;
    let mut raw = String::new();
    s.read_to_string(&mut raw).ok()?;
    let (head, body) = raw.split_once("\r\n\r\n")?;
    if !head.starts_with("HTTP/1.1 200") {
        return None;
    }
    serde_json::from_str(body).ok().filter(|v: &Value| v["ready"] == Value::Bool(true))
}

/// A message through Wisp to wherever Wisp sends news: a notification on the Mac and, when
/// Wisp's Telegram is set up, the phone. POST /notify needs no key (local programs only, which
/// Wisp checks). `urgent` gets through a macOS Focus too. Returns whether Wisp took it.
pub fn notify(title: &str, text: &str) -> bool {
    let body = serde_json::json!({ "title": title, "text": text, "urgent": true }).to_string();
    let Ok(mut s) = TcpStream::connect_timeout(&([127, 0, 0, 1], PORT).into(), Duration::from_millis(500)) else { return false };
    let _ = s.set_read_timeout(Some(Duration::from_secs(3)));
    let sent = write!(
        s,
        "POST /notify HTTP/1.1\r\nHost: 127.0.0.1:{PORT}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let mut raw = String::new();
    sent.is_ok() && s.read_to_string(&mut raw).is_ok() && raw.starts_with("HTTP/1.1 200")
}

/// Lines for the chat's instructions: what the agents are doing right now.
pub fn summary_for_chat() -> Option<String> {
    let v = LATEST.lock().ok()?.clone()?;
    let mut by: HashMap<&str, Vec<String>> = HashMap::new();
    for i in v["items"].as_array()? {
        let state = i["state"].as_str().unwrap_or("");
        let name = i["name"].as_str().unwrap_or("?");
        let doing = i["doing"].as_str().unwrap_or("");
        by.entry(state).or_default().push(if doing.is_empty() { name.to_string() } else { format!("{name} ({doing})") });
    }
    let list = |k: &str| by.get(k).map(|v| v.join("; ")).unwrap_or_else(|| "nikdo".into());
    Some(format!(
        "Na tomhle Macu běží Wisp, přehled automatizací a AI agentů. Teď: pracuje: {}. Selhalo: {}. Čeká na člověka: {}. \
Hotovo nedávno: {}. Limit Claude: {} % v pětihodinovém okně, {} % týdne.",
        list("run"),
        list("bad"),
        list("you"),
        list("done"),
        v["claude"]["session"].as_i64().map_or("?".into(), |n| n.to_string()),
        v["claude"]["week"].as_i64().map_or("?".into(), |n| n.to_string()),
    ))
}

pub fn start(app: AppHandle) {
    std::thread::spawn(move || {
        // State per item at the last look; the first look only learns, it announces nothing.
        let mut before: Option<HashMap<String, String>> = None;
        let mut busy_sent = None;
        loop {
            std::thread::sleep(Duration::from_secs(3));
            if !ON.load(Ordering::Relaxed) {
                before = None;
                if busy_sent != Some(false) {
                    busy_sent = Some(false);
                    let _ = app.emit_to("pet", "wisp-busy", false);
                }
                continue;
            }
            let Some(v) = key().and_then(|k| fetch(&k)) else { continue };
            if let Ok(mut l) = LATEST.lock() {
                *l = Some(v.clone());
            }
            let items = v["items"].as_array().cloned().unwrap_or_default();
            let now: HashMap<String, String> = items.iter().map(|i| (i["id"].as_str().unwrap_or("").to_string(), i["state"].as_str().unwrap_or("").to_string())).collect();

            // Someone is working: the buddy looks busy too.
            let busy = now.values().any(|s| s == "run");
            if busy_sent != Some(busy) {
                busy_sent = Some(busy);
                let _ = app.emit_to("pet", "wisp-busy", busy);
            }

            if let Some(was) = &before {
                for i in &items {
                    let id = i["id"].as_str().unwrap_or("");
                    let state = i["state"].as_str().unwrap_or("");
                    let old = was.get(id).map(String::as_str).unwrap_or("");
                    if old == state {
                        continue;
                    }
                    let name = i["name"].as_str().unwrap_or("Něco");
                    let doing = i["doing"].as_str().unwrap_or("").trim();
                    let detail = if doing.is_empty() { String::new() } else { format!(": {doing}") };
                    match state {
                        // Finished a job (not failed, not switched off): a somersault.
                        "done" | "ok" | "sleep" if old == "run" => {
                            let _ = app.emit_to("pet", "pet-react", "celebrate");
                        }
                        "bad" => crate::chat::say(&app, &format!("{name} selhal{detail}"), "sad"),
                        "you" => crate::chat::say(&app, &format!("{name} na tebe čeká{detail}"), "curious"),
                        _ => {}
                    }
                }
            }
            before = Some(now);
        }
    });
}

#[cfg(test)]
mod live {
    /// `cargo test --lib live_wisp -- --ignored --nocapture`: what Wisp tells the buddy right now.
    #[test]
    #[ignore]
    fn live_wisp() {
        let v = super::fetch(&super::key().expect("Wisp's key")).expect("Wisp answers");
        *super::LATEST.lock().unwrap() = Some(v);
        println!("{}", super::summary_for_chat().unwrap());
    }
}
