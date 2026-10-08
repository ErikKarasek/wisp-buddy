//! The buddy's voice and ears. It says its answers out loud with the Mac's own Czech voice
//! (`say`, tray → Mluví nahlas), and it listens: the microphone button in the bubble records
//! with ffmpeg until pressed again, and Gemini writes down what was said (chat.rs).

use std::io::Write;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

static ALOUD: AtomicBool = AtomicBool::new(true);
/// What is being said now, so a new answer cuts the old one off instead of talking over it.
static SPEAKING: Mutex<Option<Child>> = Mutex::new(None);
static RECORDING: Mutex<Option<(Child, PathBuf, Instant)>> = Mutex::new(None);

/// Recordings stop by themselves after this long.
const MAX_SECS: u32 = 45;

pub fn set_aloud(on: bool) {
    ALOUD.store(on, Ordering::Relaxed);
    if !on {
        hush();
    }
}

/// Without links, ids and emoji, which only sound silly read out.
fn speakable(text: &str) -> String {
    let plain: String = text.chars().filter(|c| (*c as u32) < 0x2190 || ('\u{2190}'..='\u{21ff}').contains(c)).collect();
    plain.split_whitespace().filter(|w| !w.starts_with("http://") && !w.starts_with("https://")).collect::<Vec<_>>().join(" ")
}

/// Says it out loud, when the tray allows it.
pub fn speak(text: &str) {
    if !ALOUD.load(Ordering::Relaxed) {
        return;
    }
    let text = speakable(text);
    if text.is_empty() {
        return;
    }
    hush();
    let child = Command::new("/usr/bin/say").args(["-v", "Zuzana", "-r", "195", "--", &text]).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn();
    if let Ok(c) = child {
        *SPEAKING.lock().unwrap_or_else(|e| e.into_inner()) = Some(c);
    }
}

/// Says it only when someone is at the Mac: news nobody asked for should not talk to an empty room.
pub fn speak_if_here(text: &str) {
    if crate::pet::someone_here() {
        speak(text);
    }
}

pub fn hush() {
    if let Some(mut c) = SPEAKING.lock().unwrap_or_else(|e| e.into_inner()).take() {
        let _ = c.kill();
        let _ = c.wait();
    }
}

#[tauri::command]
pub fn voice_hush() {
    hush();
}

// ---------- listening ----------

fn ffmpeg() -> Option<PathBuf> {
    ["/opt/homebrew/bin/ffmpeg", "/usr/local/bin/ffmpeg"].iter().map(PathBuf::from).find(|p| p.exists())
}

/// The Mac's own microphone: not the iPhone's (Continuity lists it first), when it can tell.
fn microphone(ff: &PathBuf) -> String {
    let out = Command::new(ff).args(["-hide_banner", "-f", "avfoundation", "-list_devices", "true", "-i", ""]).output();
    let text = out.map(|o| String::from_utf8_lossy(&o.stderr).into_owned()).unwrap_or_default();
    let audio = text.split("audio devices").nth(1).unwrap_or_default();
    for line in audio.lines() {
        // "[AVFoundation indev @ 0x…] [1] Mikrofon MacBook Air"
        let Some(rest) = line.split("] [").nth(1) else { continue };
        let Some((index, name)) = rest.split_once("] ") else { continue };
        let lower = name.to_lowercase();
        if lower.contains("macbook") || lower.contains("imac") || lower.contains("built-in") || lower.contains("vestavěn") {
            return format!(":{index}");
        }
    }
    ":0".into()
}

/// Starts recording. The buddy stands still and listens.
#[tauri::command]
pub fn listen_start(app: AppHandle) -> Result<(), String> {
    let mut rec = RECORDING.lock().unwrap_or_else(|e| e.into_inner());
    if rec.is_some() {
        return Ok(());
    }
    hush();
    let ff = ffmpeg().ok_or("Chybí ffmpeg (brew install ffmpeg).")?;
    let dir = std::env::temp_dir().join("wisp-buddy-voice");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(format!("hlas-{}.wav", crate::reminders::now_ms()));
    let child = Command::new(&ff)
        .args(["-hide_banner", "-loglevel", "error", "-f", "avfoundation", "-i", &microphone(&ff), "-ac", "1", "-ar", "16000", "-t", &MAX_SECS.to_string(), "-y"])
        .arg(&path)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| e.to_string())?;
    *rec = Some((child, path, Instant::now()));
    let _ = app.emit_to("pet", "pet-react", "listen");
    Ok(())
}

/// Stops and hands back the recording; None when there was none or it was too short to mean anything.
fn stop() -> Option<Vec<u8>> {
    let (mut child, path, began) = RECORDING.lock().unwrap_or_else(|e| e.into_inner()).take()?;
    // "q" lets ffmpeg finish the file properly.
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(b"q");
    }
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        if matches!(child.try_wait(), Ok(Some(_))) {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let _ = child.kill();
    let _ = child.wait();
    let audio = std::fs::read(&path).ok();
    let _ = std::fs::remove_file(&path);
    if began.elapsed() < Duration::from_millis(600) {
        return None;
    }
    // A wav header alone is 44 bytes; less than a third of a second of sound is nothing said.
    audio.filter(|a| a.len() > 44 + 16_000 * 2 / 3)
}

/// Stops recording and writes down what was said. Empty when nothing was.
#[tauri::command]
pub async fn listen_stop(app: AppHandle) -> Result<String, String> {
    let Some(audio) = stop() else { return Ok(String::new()) };
    crate::chat::transcribe(&app, &audio).await
}

#[tauri::command]
pub fn listen_cancel(app: AppHandle) {
    if stop().is_some() {
        let _ = app.emit_to("pet", "pet-react", "wake");
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn reads_out_words_but_not_links_or_emoji() {
        assert_eq!(super::speakable("Hotovo 🎉 PR: https://github.com/x/y/pull/1"), "Hotovo PR:");
        assert_eq!(super::speakable("Za 10 minut → call"), "Za 10 minut → call");
    }
}
