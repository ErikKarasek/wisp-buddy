//! The buddy's voice and ears. It says its answers out loud with the best Czech voice the Mac
//! has (`say`, tray → Mluví nahlas), sentence by sentence as the answer is still being written,
//! and it listens: the microphone button in the bubble records with ffmpeg until pressed again,
//! and Gemini writes down what was said (chat.rs).

use std::io::Write;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

static ALOUD: AtomicBool = AtomicBool::new(true);
/// Bumped whenever the buddy is cut off: anything queued under an older number is dropped
/// instead of being read out after the fact.
static TURN: AtomicU64 = AtomicU64::new(0);
/// The sentence being said right now, so it can be cut off mid-word.
static PLAYING: Mutex<Option<Child>> = Mutex::new(None);
static RECORDING: Mutex<Option<(Child, PathBuf, Instant)>> = Mutex::new(None);

/// Recordings stop by themselves after this long.
const MAX_SECS: u32 = 45;

pub fn set_aloud(on: bool) {
    ALOUD.store(on, Ordering::Relaxed);
    if !on {
        hush();
    }
}

// ---------- which voice ----------

/// What the Mac has installed, picked once and then only re-checked while it is still the poor
/// one: installing a better voice should not need a restart.
static CHOSEN: Mutex<Option<(String, Instant)>> = Mutex::new(None);

/// How good a voice is by its name. macOS ships a compact Zuzana, and she is the one that sounds
/// like a translator. For Czech the only better Apple voice is Iveta, a 121 MB download in System
/// Settings → Accessibility → Spoken Content → System Voice → Manage Voices; unlike other
/// languages, Czech has no Premium or Enhanced Zuzana at all. The suffixes are still read here
/// because other languages do carry them.
fn rank(name: &str) -> u8 {
    if name.contains("Premium") {
        4
    } else if name.contains("Enhanced") {
        3
    } else if name.starts_with("Iveta") {
        2
    } else {
        1
    }
}

/// Every Czech voice `say` knows, best first.
fn czech_voices() -> Vec<String> {
    let out = Command::new("/usr/bin/say").args(["-v", "?"]).output();
    let text = out.map(|o| String::from_utf8_lossy(&o.stdout).into_owned()).unwrap_or_default();
    let mut found: Vec<String> = text
        .lines()
        // "Zuzana              cs_CZ    # Ahoj! Já jsem Zuzana."
        .filter_map(|line| line.split_once("cs_CZ").map(|(name, _)| name.trim().to_string()))
        .filter(|name| !name.is_empty())
        .collect();
    found.sort_by_key(|name| std::cmp::Reverse(rank(name)));
    found
}

/// The voice to speak with, and whether it is one of the better ones.
fn voice() -> (String, bool) {
    let mut chosen = CHOSEN.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((name, picked)) = chosen.as_ref() {
        // A good one is settled. A poor one may just mean the download has not happened yet.
        if rank(name) > 1 || picked.elapsed() < Duration::from_secs(60) {
            return (name.clone(), rank(name) > 1);
        }
    }
    let name = czech_voices().into_iter().next().unwrap_or_else(|| "Zuzana".into());
    *chosen = Some((name.clone(), Instant::now()));
    let good = rank(&name) > 1;
    (name, good)
}

// ---------- saying it ----------

/// Without links, ids and emoji, which only sound silly read out.
fn speakable(text: &str) -> String {
    let plain: String = text.chars().filter(|c| (*c as u32) < 0x2190 || ('\u{2190}'..='\u{21ff}').contains(c)).collect();
    plain.split_whitespace().filter(|w| !w.starts_with("http://") && !w.starts_with("https://")).collect::<Vec<_>>().join(" ")
}

/// One thread says the queued sentences in order: the next one starts only when the one before
/// it has been said, so a streamed answer comes out as speech instead of as a chorus.
fn mouth() -> &'static Sender<(u64, String)> {
    static MOUTH: OnceLock<Sender<(u64, String)>> = OnceLock::new();
    MOUTH.get_or_init(|| {
        let (tx, rx) = std::sync::mpsc::channel::<(u64, String)>();
        std::thread::spawn(move || {
            for (turn, text) in rx {
                if turn != TURN.load(Ordering::SeqCst) {
                    continue;
                }
                let (name, good) = voice();
                // The better voices carry a normal pace; the compact one needs the hurry to
                // sound less like a machine reading a list.
                let rate = if good { "185" } else { "195" };
                let child = Command::new("/usr/bin/say").args(["-v", &name, "-r", rate, "--", &text]).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn();
                let Ok(child) = child else { continue };
                *PLAYING.lock().unwrap_or_else(|e| e.into_inner()) = Some(child);
                // Wait it out, holding the lock only for the look, so hush() can get in.
                loop {
                    {
                        let mut playing = PLAYING.lock().unwrap_or_else(|e| e.into_inner());
                        match playing.as_mut() {
                            // Taken away by hush(), or finished on its own.
                            None => break,
                            Some(c) => {
                                if matches!(c.try_wait(), Ok(Some(_)) | Err(_)) {
                                    *playing = None;
                                    break;
                                }
                            }
                        }
                    }
                    if turn != TURN.load(Ordering::SeqCst) {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(30));
                }
            }
        });
        tx
    })
}

/// Cuts off whatever is being said and starts a new answer. The number it gives back belongs to
/// that answer: sentences handed to `speak_part` under it are dropped once it is cut off again.
pub fn begin() -> u64 {
    hush();
    TURN.load(Ordering::SeqCst)
}

/// One more sentence of the answer that `begin` started.
pub fn speak_part(turn: u64, text: &str) {
    if !ALOUD.load(Ordering::Relaxed) {
        return;
    }
    let text = speakable(text);
    if text.trim().is_empty() {
        return;
    }
    let _ = mouth().send((turn, text));
}

/// Says a whole line out loud, on its own, cutting off anything before it.
pub fn speak(text: &str) {
    if !ALOUD.load(Ordering::Relaxed) {
        return;
    }
    let turn = begin();
    speak_part(turn, text);
}

/// Says it only when someone is at the Mac: news nobody asked for should not talk to an empty room.
pub fn speak_if_here(text: &str) {
    if crate::pet::someone_here() {
        speak(text);
    }
}

pub fn hush() {
    TURN.fetch_add(1, Ordering::SeqCst);
    if let Some(mut c) = PLAYING.lock().unwrap_or_else(|e| e.into_inner()).take() {
        let _ = c.kill();
        let _ = c.wait();
    }
}

#[tauri::command]
pub fn voice_hush() {
    hush();
}

/// The next whole sentence sitting in `buf`, taken out of it, or None while the answer has not
/// written one yet. So the buddy can start talking before the model has finished thinking.
pub fn next_sentence(buf: &mut String, flush: bool) -> Option<String> {
    let bytes: Vec<(usize, char)> = buf.char_indices().collect();
    let mut cut = None;
    for (i, (at, c)) in bytes.iter().enumerate() {
        let ends = matches!(c, '.' | '!' | '?' | '\n');
        // "3. 10." and "v 17:42" are not the end of anything.
        let after_digit = i > 0 && bytes[i - 1].1.is_ascii_digit();
        let next = bytes.get(i + 1).map(|(_, c)| *c);
        let breathes = next.is_none_or(|c| c.is_whitespace());
        if ends && breathes && !(after_digit && *c == '.') && *at >= 2 {
            cut = Some(at + c.len_utf8());
            break;
        }
        // A sentence that will not end soon still deserves to be started.
        if *c == ',' && breathes && *at >= 140 {
            cut = Some(at + c.len_utf8());
            break;
        }
    }
    match cut {
        Some(at) => {
            let rest = buf.split_off(at);
            let said = std::mem::replace(buf, rest.trim_start().to_string());
            Some(said)
        }
        None if flush && !buf.trim().is_empty() => Some(std::mem::take(buf)),
        None => None,
    }
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
    // "q" lets ffmpeg finish the file properly. It takes a moment, and that moment is the
    // first thing the person waits through, so it is a short one.
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(b"q");
    }
    let deadline = Instant::now() + Duration::from_millis(500);
    while Instant::now() < deadline {
        if matches!(child.try_wait(), Ok(Some(_))) {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
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

    #[test]
    fn a_premium_voice_beats_the_compact_one() {
        assert!(super::rank("Zuzana (Premium)") > super::rank("Zuzana (Enhanced)"));
        assert!(super::rank("Zuzana (Enhanced)") > super::rank("Zuzana"));
    }

    #[test]
    fn speaks_each_sentence_as_soon_as_it_is_whole() {
        let mut buf = "Hotovo, mrknu na to.".to_string();
        assert_eq!(super::next_sentence(&mut buf, false), Some("Hotovo, mrknu na to.".into()));
        assert_eq!(buf, "");

        let mut buf = "Ahoj! A jak".to_string();
        assert_eq!(super::next_sentence(&mut buf, false), Some("Ahoj!".into()));
        assert_eq!(buf, "A jak");
        assert_eq!(super::next_sentence(&mut buf, false), None);
        assert_eq!(super::next_sentence(&mut buf, true), Some("A jak".into()));
    }

    #[test]
    fn a_date_is_not_the_end_of_a_sentence() {
        let mut buf = "Schůzka je 3. 10. v 17:42 u tebe".to_string();
        assert_eq!(super::next_sentence(&mut buf, false), None);
    }
}
