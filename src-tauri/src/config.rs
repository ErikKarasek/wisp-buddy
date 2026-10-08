//! The buddy's settings, a JSON file in the app's config folder: the saved characters, which
//! one it wears, and whether the shapes move. The page owns the shape of it; Rust only keeps it.

use serde_json::Value;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager};

/// One writer at a time: the reminder thread, the tray and the pages all change this file.
static WRITE: Mutex<()> = Mutex::new(());

/// Fields Rust keeps (the tray and the reminder thread). A page saving its own part (the
/// characters, what is worn) never overwrites these with the copy it loaded a moment ago.
const RUST_OWNED: [&str; 13] =
    ["reminders", "bedtime", "bedtimeNudged", "climb", "wisp", "shapeMotion", "phone", "size", "memory", "voice", "home", "briefing", "briefingDone"];

fn path(app: &AppHandle) -> Option<PathBuf> {
    Some(app.path().app_config_dir().ok()?.join("config.json"))
}

#[tauri::command]
pub fn config_load(app: AppHandle) -> Value {
    path(&app)
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or(Value::Null)
}

/// Saves and tells every window, so the buddy puts on a new look the moment the studio saves.
#[tauri::command]
pub fn config_save(app: AppHandle, config: Value) -> Result<(), String> {
    let _w = WRITE.lock().unwrap_or_else(|e| e.into_inner());
    let mut config = config;
    let current = config_load(app.clone());
    if let (Value::Object(new), Value::Object(cur)) = (&mut config, &current) {
        for key in RUST_OWNED {
            match cur.get(key) {
                Some(v) => {
                    new.insert(key.to_string(), v.clone());
                }
                None => {
                    new.remove(key);
                }
            }
        }
    }
    write(&app, &config)
}

fn write(app: &AppHandle, config: &Value) -> Result<(), String> {
    let p = path(app).ok_or("no config folder")?;
    if let Some(dir) = p.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let text = serde_json::to_string_pretty(config).map_err(|e| e.to_string())?;
    // Write beside and rename, so a crash mid-write never leaves half a file.
    let tmp = p.with_extension("json.tmp");
    std::fs::write(&tmp, text).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &p).map_err(|e| e.to_string())?;
    let _ = app.emit("config-changed", ());
    Ok(())
}

/// The characters saved in Wisp on this Mac, if Wisp is installed: the family's gallery.
#[tauri::command]
pub fn wisp_characters() -> Value {
    let home = std::env::var("HOME").unwrap_or_default();
    let file = PathBuf::from(home).join("Library/Application Support/cz.erikkarasek.dispecink/config.json");
    std::fs::read_to_string(file)
        .ok()
        .and_then(|s| serde_json::from_str::<Value>(&s).ok())
        .and_then(|v| v.get("characters").cloned())
        .unwrap_or(Value::Array(vec![]))
}

/// Change one top-level field and save, for the tray (the motion switch) without a page.
pub fn set_field(app: &AppHandle, key: &str, value: Value) {
    let _w = WRITE.lock().unwrap_or_else(|e| e.into_inner());
    let mut cfg = config_load(app.clone());
    if !cfg.is_object() {
        cfg = Value::Object(Default::default());
    }
    cfg[key] = value;
    let _ = write(app, &cfg);
}

/// Read, change and write one field as one step, for a change that depends on what is there
/// (the reminder list), so nothing slips in between.
pub fn update_field(app: &AppHandle, key: &str, f: impl FnOnce(Value) -> Value) {
    let _w = WRITE.lock().unwrap_or_else(|e| e.into_inner());
    let mut cfg = config_load(app.clone());
    if !cfg.is_object() {
        cfg = Value::Object(Default::default());
    }
    let old = cfg.get(key).cloned().unwrap_or(Value::Null);
    cfg[key] = f(old);
    let _ = write(app, &cfg);
}

/// One top-level field, for the tray's initial state.
pub fn field(app: &AppHandle, key: &str) -> Option<Value> {
    config_load(app.clone()).get(key).cloned()
}
