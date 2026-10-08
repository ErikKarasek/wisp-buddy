//! Real work on request: "oprav ten bug ve scoutovi" goes to Wisp's night shift, which runs
//! Claude Code in its own git worktree, commits, pushes and opens a draft PR. The buddy only
//! writes the task into Wisp's queue (night.json, which Wisp reads every 30 s) and keeps an eye
//! on it: when a task finishes, it says so in the bubble and does a somersault.
//!
//! Nothing starts without a yes: the chat asks with a button first (chat.rs).

use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::fs::{File, OpenOptions, TryLockError};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

fn home() -> PathBuf {
    PathBuf::from(std::env::var("HOME").unwrap_or_default())
}

/// Wisp's queue. Wisp owns the file; the buddy adds to it the way Wisp does.
fn queue_file() -> PathBuf {
    home().join("Library/Application Support/cz.erikkarasek.dispecink/night.json")
}

fn wisp_installed() -> bool {
    queue_file().parent().is_some_and(|d| d.is_dir())
}

/// How long to wait for Wisp before writing anyway.
const LOCK_WAIT: Duration = Duration::from_secs(1);

/// The same lock Wisp takes in its night.rs: an flock on night.lock beside the queue, held
/// across the read and the write, so the two processes take turns instead of overwriting each
/// other. It has to be a file of its own, because the queue is replaced by a rename and a lock
/// on the inode that was renamed away would guard nothing. Both sides have to keep the same
/// name; this is the one place it is written down on this side.
///
/// Dropping the file unlocks it, and so does the process dying, so a crash cannot leave the
/// queue locked. Reading alone needs no lock: a reader always sees one whole file.
fn lock_in(dir: &PathBuf, wait: Duration) -> Option<File> {
    let f = OpenOptions::new().create(true).read(true).write(true).open(dir.join("night.lock")).ok()?;
    // A turn is a millisecond of reading and writing one small file. A whole second means Wisp
    // is stuck, and then it is better to write the task than to lose it.
    let until = Instant::now() + wait;
    loop {
        match f.try_lock() {
            Ok(()) => return Some(f),
            Err(TryLockError::WouldBlock) if Instant::now() < until => std::thread::sleep(Duration::from_millis(5)),
            _ => return None,
        }
    }
}

fn queue_lock() -> Option<File> {
    lock_in(&queue_file().parent()?.to_path_buf(), LOCK_WAIT)
}

fn now_ms() -> u64 {
    crate::reminders::now_ms() as u64
}

fn read() -> Vec<Value> {
    std::fs::read(queue_file()).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

fn write(all: &[Value]) -> Result<(), String> {
    let file = queue_file();
    let tmp = file.with_extension("json.buddy.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(all).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &file).map_err(|e| e.to_string())
}

/// The git projects in ~/Developer.
pub fn projects() -> Vec<String> {
    let mut out: Vec<String> = std::fs::read_dir(home().join("Developer"))
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().join(".git").is_dir())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    out.sort();
    out
}

/// "buddy", "Wisp Buddy", "job mail" → the folder it means, if exactly one fits. As Wisp does it.
pub fn find_project(name: &str) -> Option<String> {
    pick(name, &projects())
}

fn pick(name: &str, all: &[String]) -> Option<String> {
    let norm = |s: &str| s.to_lowercase().chars().filter(|c| c.is_alphanumeric()).collect::<String>();
    let want = norm(name);
    if want.is_empty() {
        return None;
    }
    if let Some(p) = all.iter().find(|p| norm(p) == want) {
        return Some(p.clone());
    }
    let partial: Vec<&String> = all.iter().filter(|p| norm(p).contains(&want)).collect();
    (partial.len() == 1).then(|| partial[0].clone())
}

/// Into Wisp's queue. `now`: at once; otherwise Wisp starts it at night or when Erik is away.
pub fn add(project: &str, task: &str, now: bool) -> Result<String, String> {
    if !wisp_installed() {
        return Err("Wisp na tomhle Macu není, práci za mě dělá jeho noční směna.".into());
    }
    let t = task_entry(&format!("{:x}", now_ms()), project, task, now);
    let id = t["id"].as_str().unwrap_or_default().to_string();
    // Reading and writing as one step, so a status Wisp wrote meanwhile does not get rolled back.
    let _across = queue_lock();
    let mut all = read();
    all.push(t);
    write(&all)?;
    Ok(id)
}

/// A task exactly as Wisp's night.rs keeps it. Wisp reads the whole file as one list, so an entry
/// it cannot read would cost it the whole queue.
fn task_entry(id: &str, project: &str, task: &str, now: bool) -> Value {
    json!({
        "id": id, "project": project, "task": task.trim(), "status": "queued", "now": now,
        "addedMs": now_ms(), "finishedMs": null, "summary": null, "pr": null, "branch": null,
    })
}

/// The queue and the last results, for the chat.
pub fn status() -> Value {
    if !wisp_installed() {
        return json!({ "error": "Wisp is not installed on this Mac, so there is no one to do the work." });
    }
    let all = read();
    let pick = |t: &Value| json!({ "project": t["project"], "task": t["task"], "status": t["status"], "pr": t["pr"], "summary": t["summary"], "now": t["now"] });
    let open: Vec<Value> = all.iter().filter(|t| t["status"] == "queued" || t["status"] == "running").map(pick).collect();
    let done: Vec<Value> = all.iter().rev().filter(|t| t["status"] == "done" || t["status"] == "failed").take(5).map(pick).collect();
    json!({ "open": open, "recent": done, "projects": projects() })
}

/// Tasks that finished since `since_ms`, as lines for the morning.
pub fn finished_since(since_ms: u64) -> Vec<String> {
    read()
        .iter()
        .filter(|t| t["finishedMs"].as_u64().is_some_and(|f| f >= since_ms) && (t["status"] == "done" || t["status"] == "failed"))
        .map(|t| {
            let how = if t["status"] == "done" { "hotovo" } else { "nedopadlo" };
            let pr = t["pr"].as_str().map(|p| format!(", PR {p}")).unwrap_or_default();
            format!("{} – {} ({how}{pr})", t["project"].as_str().unwrap_or("?"), t["task"].as_str().unwrap_or(""))
        })
        .collect()
}

pub fn start(app: AppHandle) {
    std::thread::spawn(move || {
        // Status per task at the last look; the first look only learns.
        let mut before: Option<HashMap<String, String>> = None;
        loop {
            std::thread::sleep(Duration::from_secs(15));
            if !wisp_installed() {
                continue;
            }
            let all = read();
            let now: HashMap<String, String> = all.iter().map(|t| (t["id"].as_str().unwrap_or("").to_string(), t["status"].as_str().unwrap_or("").to_string())).collect();
            if let Some(was) = &before {
                for t in &all {
                    let id = t["id"].as_str().unwrap_or("");
                    let state = t["status"].as_str().unwrap_or("");
                    if was.get(id).map(String::as_str) != Some("running") || state == "running" {
                        continue;
                    }
                    let project = t["project"].as_str().unwrap_or("?");
                    let line = match (state, t["pr"].as_str()) {
                        ("done", Some(pr)) => format!("Hotovo v {project}: {}. PR ke kontrole: {pr}", t["task"].as_str().unwrap_or("")),
                        ("done", None) => format!("Hotovo v {project}: {}", t["summary"].as_str().unwrap_or("bez změn")),
                        _ => format!("V {project} to nedopadlo: {}", t["summary"].as_str().unwrap_or("nevím proč")),
                    };
                    // At night nobody reads it; the morning overview brings it instead.
                    if crate::pet::someone_here() {
                        crate::chat::say(&app, &line, if state == "done" { "celebrate" } else { "sad" });
                    } else if state == "done" {
                        let _ = app.emit_to("pet", "pet-react", "celebrate");
                    }
                }
            }
            before = Some(now);
        }
    });
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_queue_lock_holds_wisp_off_until_it_is_dropped() {
        let dir = std::env::temp_dir().join(format!("buddy-lock-{}", super::now_ms()));
        std::fs::create_dir_all(&dir).unwrap();
        // The real wait is a second; a test has no patience for it.
        let short = std::time::Duration::from_millis(50);
        let held = super::lock_in(&dir, short).expect("the first writer takes it");
        assert!(super::lock_in(&dir, short).is_none(), "nobody else writes while it is held");
        drop(held);
        assert!(super::lock_in(&dir, short).is_some(), "free again once the first writer is done");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn finds_a_project_by_a_loose_name() {
        let all: Vec<String> = ["wisp-buddy", "job-mail", "job-tracker", "dispecink"].iter().map(|s| s.to_string()).collect();
        assert_eq!(super::pick("Wisp Buddy", &all).as_deref(), Some("wisp-buddy"));
        assert_eq!(super::pick("mail", &all).as_deref(), Some("job-mail"));
        assert_eq!(super::pick("job", &all), None);
        assert_eq!(super::pick("", &all), None);
    }

    #[test]
    fn a_task_has_every_field_wisp_needs() {
        let t = super::task_entry("19a2b3c4d5e", "wisp-buddy", "  přidej zvuky  ", true);
        assert_eq!(t["task"], "přidej zvuky");
        // Wisp's Task: id, project, task, status (strings), addedMs (number) are required.
        for k in ["id", "project", "task", "status"] {
            assert!(t[k].is_string(), "{k}");
        }
        assert!(t["addedMs"].is_u64());
        assert_eq!(t["now"], true);
        assert_eq!(t["status"], "queued");
    }
}
