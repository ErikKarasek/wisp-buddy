//! What the buddy knows about its person, kept in config.json under "memory": short facts it
//! was told to remember ("pracuju v Brně", "PC zapínám kvůli hrám"). Every conversation starts
//! with them, so it does not ask the same thing twice. Added and dropped by talking (chat.rs).

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::AppHandle;

use crate::config;

/// Enough for a person's life in short lines, small enough for every call.
const MAX: usize = 150;

#[derive(Serialize, Deserialize, Clone)]
pub struct Fact {
    pub id: String,
    pub text: String,
    /// When it was told, in milliseconds since 1970.
    pub at: i64,
}

pub fn list(app: &AppHandle) -> Vec<Fact> {
    config::field(app, "memory").and_then(|v| serde_json::from_value(v).ok()).unwrap_or_default()
}

fn change<R>(app: &AppHandle, f: impl FnOnce(&mut Vec<Fact>) -> R) -> R {
    let mut out = None;
    config::update_field(app, "memory", |v| {
        let mut all: Vec<Fact> = serde_json::from_value(v).unwrap_or_default();
        out = Some(f(&mut all));
        // The oldest go first when it is full.
        let over = all.len().saturating_sub(MAX);
        all.drain(..over);
        serde_json::to_value(all).unwrap_or(Value::Array(vec![]))
    });
    out.expect("update_field runs the change")
}

fn same(a: &str, b: &str) -> bool {
    let norm = |s: &str| s.to_lowercase().chars().filter(|c| c.is_alphanumeric()).collect::<String>();
    norm(a) == norm(b)
}

/// Remembers a fact; the same fact again keeps the old one.
pub fn add(app: &AppHandle, text: &str) -> Fact {
    let text = text.trim().to_string();
    change(app, |all| {
        if let Some(f) = all.iter().find(|f| same(&f.text, &text)) {
            return f.clone();
        }
        let f = Fact { id: format!("m{}", crate::reminders::new_id()), text, at: crate::reminders::now_ms() };
        all.push(f.clone());
        f
    })
}

pub fn remove(app: &AppHandle, id: &str) -> bool {
    change(app, |all| {
        let before = all.len();
        all.retain(|f| f.id != id);
        all.len() != before
    })
}

/// Lines for the chat's instructions, with ids so a fact can be forgotten.
pub fn for_chat(app: &AppHandle) -> Option<String> {
    let all = list(app);
    if all.is_empty() {
        return None;
    }
    let lines: Vec<String> = all.iter().map(|f| format!("- [{}] {}", f.id, f.text)).collect();
    Some(format!("Co o něm víš (sám sis to zapamatoval dřív):\n{}", lines.join("\n")))
}

#[tauri::command]
pub fn memory_list(app: AppHandle) -> Value {
    json!(list(&app))
}

#[tauri::command]
pub fn memory_remove(app: AppHandle, id: String) -> bool {
    remove(&app, &id)
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_same_fact_is_the_same_whatever_the_case_and_dots() {
        assert!(super::same("Pracuju v Brně.", "pracuju v brně"));
        assert!(!super::same("Pracuju v Brně", "Pracuju v Praze"));
    }
}
