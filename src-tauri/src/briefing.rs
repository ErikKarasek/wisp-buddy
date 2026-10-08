//! The morning overview: the first time someone is at the Mac after six in the morning, the buddy
//! says good morning and what the day holds, out loud and in the bubble. The facts are its own
//! (today's reminders, what the night shift finished, what Wisp's agents need, what it was told
//! to remember); Gemini only puts them into a few friendly sentences. Once a day, kept in the
//! config ("briefingDone"), tray → Ranní přehled. Asking "co mě dneska čeká?" gives the same.

use serde_json::{json, Value};
use std::time::Duration;
use tauri::AppHandle;

use crate::config;

/// From six until noon; after that it is no longer a morning overview.
const FROM_HOUR: i32 = 6;
const UNTIL_HOUR: i32 = 12;

fn local(ms: i64) -> libc::tm {
    let secs = (ms / 1000) as libc::time_t;
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    unsafe { libc::localtime_r(&secs, &mut tm) };
    tm
}

fn date_of(ms: i64) -> String {
    let tm = local(ms);
    format!("{}-{:02}-{:02}", tm.tm_year + 1900, tm.tm_mon + 1, tm.tm_mday)
}

pub fn enabled(app: &AppHandle) -> bool {
    config::field(app, "briefing").and_then(|v| v.as_bool()).unwrap_or(true)
}

/// What the day holds, as facts for the model (also the chat's `briefing` tool).
pub fn facts(app: &AppHandle) -> Value {
    let now = crate::reminders::now_ms();
    let today = date_of(now);
    let reminders: Vec<String> = crate::reminders::list(app)
        .into_iter()
        .filter(|r| date_of(r.at) == today)
        .map(|r| {
            let tm = local(r.at);
            format!("{}:{:02} {}", tm.tm_hour, tm.tm_min, r.text)
        })
        .collect();
    // Since yesterday evening.
    let night = crate::work::finished_since((now - 16 * 3_600_000).max(0) as u64);
    json!({
        "now": crate::reminders::now_iso(),
        "reminders_today": reminders,
        "night_shift_finished": night,
        "wisp": crate::wisp::summary_for_chat(),
        "remembered": crate::memory::list(app).iter().map(|f| f.text.clone()).collect::<Vec<_>>(),
    })
}

/// The same facts in plain sentences, for when Gemini cannot be asked.
fn plain(f: &Value) -> String {
    let mut out = vec!["Dobré ráno!".to_string()];
    let list = |k: &str| f[k].as_array().map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect::<Vec<_>>()).unwrap_or_default();
    let reminders = list("reminders_today");
    if reminders.is_empty() {
        out.push("Na dnešek nemám žádné připomínky.".into());
    } else {
        out.push(format!("Dneska: {}.", reminders.join(", ")));
    }
    let night = list("night_shift_finished");
    if !night.is_empty() {
        out.push(format!("V noci: {}.", night.join("; ")));
    }
    out.join(" ")
}

async fn compose(app: &AppHandle) -> String {
    let f = facts(app);
    let ask = format!(
        "Je ráno a člověk si právě sedl k Macu. Pozdrav ho a v nejvýš čtyřech krátkých větách mu řekni, co ho dnes čeká, \
podle těchhle faktů (nic dalšího si nevymýšlej, prázdné věci vynech, odkazy na PR nečti nahlas, stačí říct, že čekají na kontrolu). \
Bude se to číst nahlas, takže žádné odrážky ani emoji.\n{}",
        f
    );
    let history = [json!({ "role": "system", "content": crate::chat::persona(app) }), json!({ "role": "user", "content": ask })];
    match crate::chat::complete(&history, None).await {
        Ok(m) => m["content"].as_str().map(str::trim).filter(|t| !t.is_empty()).map(String::from).unwrap_or_else(|| plain(&f)),
        Err(_) => plain(&f),
    }
}

/// Says the overview now.
pub async fn give(app: &AppHandle) {
    let text = compose(app).await;
    crate::chat::say(app, &text, "remind");
}

pub fn start(app: AppHandle) {
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(Duration::from_secs(30));
            if !enabled(&app) || !crate::pet::someone_here() {
                continue;
            }
            let now = crate::reminders::now_ms();
            let tm = local(now);
            if tm.tm_hour < FROM_HOUR || tm.tm_hour >= UNTIL_HOUR {
                continue;
            }
            let today = date_of(now);
            if config::field(&app, "briefingDone").and_then(|v| v.as_str().map(String::from)).as_deref() == Some(today.as_str()) {
                continue;
            }
            config::set_field(&app, "briefingDone", json!(today));
            tauri::async_runtime::block_on(give(&app));
        }
    });
}

#[cfg(test)]
mod tests {
    #[test]
    fn plain_overview_without_gemini() {
        let f = serde_json::json!({ "reminders_today": ["9:00 banka"], "night_shift_finished": [] });
        assert_eq!(super::plain(&f), "Dobré ráno! Dneska: 9:00 banka.");
    }
}
