//! Reminders, kept in config.json under "reminders", and the nudge to go to bed. Made by
//! talking (Gemini calls set_reminder, see chat.rs), fired by a thread that looks every few
//! seconds: the buddy wakes up and the bubble shows the reminder without taking the focus.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

use crate::config;

#[derive(Serialize, Deserialize, Clone)]
pub struct Reminder {
    pub id: String,
    /// When, in milliseconds since 1970.
    pub at: i64,
    pub text: String,
    /// "daily", "weekdays" or none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repeat: Option<String>,
}

// ---------- local time, through libc so it follows the Mac's time zone ----------

pub fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

fn local(ms: i64) -> libc::tm {
    let secs = (ms / 1000) as libc::time_t;
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    unsafe { libc::localtime_r(&secs, &mut tm) };
    tm
}

/// A local date and time to milliseconds (mktime works out daylight saving).
fn from_local(year: i32, month: i32, day: i32, hour: i32, minute: i32) -> i64 {
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    tm.tm_year = year - 1900;
    tm.tm_mon = month - 1;
    tm.tm_mday = day;
    tm.tm_hour = hour;
    tm.tm_min = minute;
    tm.tm_isdst = -1;
    (unsafe { libc::mktime(&mut tm) }) as i64 * 1000
}

/// "2026-10-03T15:00" (local, as the model writes it) to milliseconds.
pub fn parse_local(s: &str) -> Option<i64> {
    let s = s.trim();
    let (date, time) = s.split_once(['T', ' '])?;
    let mut d = date.split('-').map(|p| p.parse::<i32>().ok());
    let (y, mo, da) = (d.next()??, d.next()??, d.next()??);
    let mut t = time.split(':').map(|p| p.get(..2).unwrap_or(p).parse::<i32>().ok());
    let (h, mi) = (t.next()??, t.next().flatten().unwrap_or(0));
    Some(from_local(y, mo, da, h, mi))
}

/// "pondělí 6. 10. 9:00" for the model and the bubble.
pub fn describe(ms: i64) -> String {
    let tm = local(ms);
    let days = ["neděle", "pondělí", "úterý", "středa", "čtvrtek", "pátek", "sobota"];
    format!("{} {}. {}. {}:{:02}", days[tm.tm_wday as usize % 7], tm.tm_mday, tm.tm_mon + 1, tm.tm_hour, tm.tm_min)
}

/// The model's notion of now: "2026-10-03T17:42, sobota".
pub fn now_iso() -> String {
    let tm = local(now_ms());
    let days = ["neděle", "pondělí", "úterý", "středa", "čtvrtek", "pátek", "sobota"];
    format!("{}-{:02}-{:02}T{:02}:{:02}, {}", tm.tm_year + 1900, tm.tm_mon + 1, tm.tm_mday, tm.tm_hour, tm.tm_min, days[tm.tm_wday as usize % 7])
}

/// The next time a repeating reminder comes round after `at`.
fn next_after(at: i64, repeat: &str) -> i64 {
    let mut next = at;
    loop {
        let tm = local(next);
        next = from_local(tm.tm_year + 1900, tm.tm_mon + 1, tm.tm_mday + 1, tm.tm_hour, tm.tm_min);
        let wday = local(next).tm_wday;
        if repeat != "weekdays" || (1..=5).contains(&wday) {
            return next;
        }
    }
}

// ---------- the list in config.json ----------

pub fn list(app: &AppHandle) -> Vec<Reminder> {
    config::field(app, "reminders").and_then(|v| serde_json::from_value(v).ok()).unwrap_or_default()
}

fn store(app: &AppHandle, mut list: Vec<Reminder>) {
    list.sort_by_key(|r| r.at);
    config::set_field(app, "reminders", serde_json::to_value(list).unwrap_or(Value::Array(vec![])));
}

fn new_id() -> String {
    format!("{:x}", now_ms() ^ ((std::process::id() as i64) << 20))
}

pub fn add(app: &AppHandle, at: i64, text: &str, repeat: Option<String>) -> Reminder {
    let r = Reminder { id: new_id(), at, text: text.trim().to_string(), repeat: repeat.filter(|r| r == "daily" || r == "weekdays") };
    let mut all = list(app);
    all.push(r.clone());
    store(app, all);
    r
}

pub fn remove(app: &AppHandle, id: &str) -> bool {
    let mut all = list(app);
    let before = all.len();
    all.retain(|r| r.id != id);
    let gone = all.len() != before;
    if gone {
        store(app, all);
    }
    gone
}

#[tauri::command]
pub fn reminder_list(app: AppHandle) -> Value {
    json!(list(&app).into_iter().map(|r| json!({ "id": r.id, "at": r.at, "text": r.text, "repeat": r.repeat, "when": describe(r.at) })).collect::<Vec<_>>())
}

#[tauri::command]
pub fn reminder_remove(app: AppHandle, id: String) -> bool {
    remove(&app, &id)
}

/// "Za 10 min" in the bubble: the same text again later.
#[tauri::command]
pub fn reminder_snooze(app: AppHandle, text: String, minutes: i64) {
    add(&app, now_ms() + minutes.clamp(1, 24 * 60) * 60_000, &text, None);
}

// ---------- firing ----------

#[derive(Serialize, Clone)]
struct Fired {
    text: String,
    /// Minutes late, when the Mac was asleep at the time.
    late: i64,
    bedtime: bool,
}

fn fire(app: &AppHandle, fired: Fired) {
    crate::pet::wake_for_reminder();
    let _ = app.emit_to("pet", "pet-react", if fired.bedtime { "yawn" } else { "remind" });
    crate::chat::open_quietly(app);
    // The bubble may have just been made: give its page a moment to listen.
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(700));
        let _ = app.emit_to("chat", "reminder", fired);
    });
}

const BEDTIME_LINES: [&str; 5] = [
    "Je {t}. Nechceš to pro dnešek zabalit? Já už sotva držím oči otevřené.",
    "Hej, už je {t}. Zítra to půjde líp, když se vyspíš.",
    "{t}… Já bych šel spát. A ty bys měl taky.",
    "Je {t}. Co kdybychom to dneska dodělali zítra?",
    "Už je {t}. Uložíš to a půjdeš spát? Já tu hlídám.",
];

/// Midnight until the tray says otherwise.
pub const DEFAULT_BEDTIME: &str = "00:00";

/// The bedtime from the tray ("23:00", "00:00", "01:00"), or none when switched off.
pub fn bedtime_setting(app: &AppHandle) -> Option<String> {
    match config::field(app, "bedtime") {
        None => Some(DEFAULT_BEDTIME.into()),
        Some(v) => v.as_str().map(String::from),
    }
}

fn bedtime(app: &AppHandle) -> Option<(i32, i32)> {
    let s = bedtime_setting(app)?;
    let (h, m) = s.split_once(':')?;
    Some((h.parse().ok()?, m.parse().ok()?))
}

pub fn start(app: AppHandle) {
    std::thread::spawn(move || {
        // The night the nudge was given, as the date the evening started on.
        let mut nudged: Option<(i32, i32, i32)> = None;
        let mut last_check = Instant::now() - Duration::from_secs(60);
        loop {
            std::thread::sleep(Duration::from_secs(5));
            let now = now_ms();

            // Reminders that are due: fire, then move a repeating one on or drop the rest.
            let all = list(&app);
            let (due, mut keep): (Vec<_>, Vec<_>) = all.into_iter().partition(|r| r.at <= now);
            if !due.is_empty() {
                for r in &due {
                    fire(&app, Fired { text: r.text.clone(), late: ((now - r.at) / 60_000).max(0), bedtime: false });
                    if let Some(rep) = &r.repeat {
                        let mut next = r.clone();
                        while next.at <= now {
                            next.at = next_after(next.at, rep);
                        }
                        keep.push(next);
                    }
                }
                store(&app, keep);
            }

            // Bedtime, once a night, only if someone is at the Mac (the mouse moved lately).
            if last_check.elapsed() < Duration::from_secs(30) {
                continue;
            }
            last_check = Instant::now();
            let Some((bh, bm)) = bedtime(&app) else { continue };
            let tm = local(now);
            let minutes = tm.tm_hour * 60 + tm.tm_min;
            let target = bh * 60 + bm;
            // From bedtime until 5 in the morning; after midnight it belongs to the evening before.
            let late = if target >= 12 * 60 { minutes >= target || minutes < 5 * 60 } else { minutes >= target && minutes < 5 * 60 };
            if !late || !crate::pet::someone_here() {
                continue;
            }
            let evening = if tm.tm_hour < 12 { local(now - 86_400_000) } else { tm };
            let night = (evening.tm_year, evening.tm_mon, evening.tm_mday);
            if nudged == Some(night) {
                continue;
            }
            nudged = Some(night);
            let line = BEDTIME_LINES[(now / 1000) as usize % BEDTIME_LINES.len()];
            let t = format!("{}:{:02}", tm.tm_hour, tm.tm_min);
            fire(&app, Fired { text: line.replace("{t}", &t), late: 0, bedtime: true });
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_models_time_and_writes_it_back() {
        let at = parse_local("2026-10-06T09:05").unwrap();
        assert_eq!(describe(at), "úterý 6. 10. 9:05");
        assert_eq!(parse_local("2026-10-06 21:30"), Some(from_local(2026, 10, 6, 21, 30)));
        assert_eq!(parse_local("2026-10-06T21:30:00"), Some(from_local(2026, 10, 6, 21, 30)));
        assert!(parse_local("zítra").is_none());
    }

    #[test]
    fn daily_moves_a_day_and_weekdays_skip_the_weekend() {
        let friday = from_local(2026, 10, 2, 9, 0);
        assert_eq!(describe(next_after(friday, "daily")), "sobota 3. 10. 9:00");
        assert_eq!(describe(next_after(friday, "weekdays")), "pondělí 5. 10. 9:00");
    }

    #[test]
    fn keeps_the_hour_across_the_change_to_winter_time() {
        // Czech clocks go back on 25 October 2026.
        let before = from_local(2026, 10, 24, 9, 0);
        assert_eq!(describe(next_after(before, "daily")), "neděle 25. 10. 9:00");
    }
}
