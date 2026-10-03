//! The buddy's body: where it is on the screen, what it is doing, and how it moves. A thread
//! steps it sixty times a second and moves the window; the page only draws the character and
//! hears what to look like ("pet" events) and when to react ("pet-react").
//!
//! Distances are in logical points and converted with the monitor's scale when the window
//! moves, so the buddy walks at the same pace on a Retina screen and an external one.

use crate::windows::Ledge;
use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, WebviewWindow};

/// The window is square; the character stands with its feet at 92 % of its height.
const SIZE: f64 = 140.0;
const FEET: f64 = 0.92;
const WALK_SPEED: f64 = 42.0;
const GRAVITY: f64 = 2600.0;
/// A landing faster than this bounces, and makes it dizzy.
const HARD_LANDING: f64 = 900.0;
/// How high it can jump up onto a window, in points.
const MAX_JUMP: f64 = 620.0;

/// Whether it climbs onto windows (the tray's switch). Off, windows are just pictures to it.
static CLIMB: AtomicBool = AtomicBool::new(true);
pub fn set_climb(on: bool) {
    CLIMB.store(on, Ordering::Relaxed);
    if !on {
        // Whoever stands on a window now steps off it.
        with(|p| {
            if p.on.is_some() {
                p.on = None;
            }
        });
    }
}

#[derive(Clone, Copy, PartialEq, Serialize, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Idle,
    Walk,
    Sit,
    Sleep,
    Fall,
    Held,
}

#[derive(Clone, Serialize)]
struct PetState {
    mode: Mode,
    facing: f64,
}

/// The screen the buddy is on, minus the menu bar and the Dock, in logical points.
#[derive(Clone, Copy)]
struct Area {
    left: f64,
    top: f64,
    right: f64,
    bottom: f64,
    scale: f64,
}

struct Pet {
    /// Top-left corner of the window, logical points.
    x: f64,
    y: f64,
    vx: f64,
    vy: f64,
    mode: Mode,
    facing: f64,
    /// When the current idle, walk or sit ends and the next one is picked.
    until: Instant,
    /// Woken by hand at night: stays up until then.
    awake_until: Option<Instant>,
    /// Asleep because the tray said so, day or not.
    sleep_by_hand: bool,
    /// Carried by the mouse: where it was grabbed, relative to the window, and recent positions
    /// for the throw.
    grab: (f64, f64),
    grabbed_at: Instant,
    moved: f64,
    trail: Vec<(Instant, f64, f64)>,
    /// Whether the window lets clicks through (the cursor is not on the body).
    passthrough: bool,
    /// The chat bubble is open: it stands still and stays awake.
    talking: bool,
    /// A click waits this long for a second one before it becomes a poke (a double click opens
    /// the chat instead), and when the last quick click was.
    hop_at: Option<Instant>,
    last_click: Option<Instant>,
    /// Where the mouse was and when it last moved: someone is at the Mac.
    cursor: (f64, f64),
    cursor_moved: Instant,
    /// The window it stands on, and where that window's edge started last frame, to ride along
    /// when the window is dragged.
    on: Option<u32>,
    ledge_x1: f64,
    sent: Option<(Mode, f64)>,
}

static PET: Mutex<Option<Pet>> = Mutex::new(None);

fn with<R>(f: impl FnOnce(&mut Pet) -> R) -> Option<R> {
    PET.lock().ok()?.as_mut().map(f)
}

/// A number from the clock, good enough for picking what to do next.
fn rand() -> f64 {
    let n = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.subsec_nanos()).unwrap_or(0);
    let x = (n as f64 * 12.9898).sin() * 43_758.545_3;
    x - x.floor()
}

fn is_night() -> bool {
    // Local hour from libc, so it follows the Mac's time zone.
    let now = unsafe { libc::time(std::ptr::null_mut()) };
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    unsafe { libc::localtime_r(&now, &mut tm) };
    tm.tm_hour >= 23 || tm.tm_hour < 6
}

fn area_of(win: &WebviewWindow) -> Option<Area> {
    let m = win.current_monitor().ok().flatten().or_else(|| win.primary_monitor().ok().flatten())?;
    let scale = m.scale_factor();
    let wa = m.work_area();
    Some(Area {
        left: wa.position.x as f64 / scale,
        top: wa.position.y as f64 / scale,
        right: (wa.position.x as f64 + wa.size.width as f64) / scale,
        bottom: (wa.position.y as f64 + wa.size.height as f64) / scale,
        scale,
    })
}

/// The window's top when the feet stand on the floor (the Dock's edge or the screen's bottom).
fn floor(a: &Area) -> f64 {
    a.bottom - SIZE * FEET
}

/// Recording a video (WISP_BUDDY_DEMO=1): jumps more, and starts on the floor a third of the
/// way across, beside whatever window the clip is about, so it has to jump up to it.
fn demo() -> bool {
    static DEMO: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *DEMO.get_or_init(|| std::env::var("WISP_BUDDY_DEMO").is_ok_and(|v| v == "1"))
}

/// How often a change of activity is a jump instead. WISP_BUDDY_DEMO=1 (for recording a video)
/// makes it most of the time, so a short clip shows it.
fn jump_chance() -> f64 {
    if demo() {
        0.6
    } else {
        0.14
    }
}

/// Now and then, a jump up onto a window it can reach. Returns whether it jumped.
fn jump_up(p: &mut Pet, ledges: &[Ledge]) -> bool {
    let feet = p.y + SIZE * FEET;
    let cx = p.x + SIZE / 2.0;
    let reachable: Vec<&Ledge> = ledges
        .iter()
        .filter(|l| l.x2 - l.x1 >= 180.0 && feet - l.y > 60.0 && feet - l.y < MAX_JUMP && ((l.x1 + l.x2) / 2.0 - cx).abs() < 700.0)
        .collect();
    if reachable.is_empty() {
        return false;
    }
    let l = reachable[(rand() * reachable.len() as f64) as usize % reachable.len()];
    // Up to a little above the edge, then down onto it: the time decides the sideways speed.
    let rise = feet - l.y + 36.0;
    let vy = (2.0 * GRAVITY * rise).sqrt();
    let t = vy / GRAVITY + (2.0 * 36.0 / GRAVITY).sqrt();
    let target = l.x1 + 50.0 + rand() * (l.x2 - l.x1 - 100.0);
    p.mode = Mode::Fall;
    p.on = None;
    p.vy = -vy;
    p.vx = (target - cx) / t;
    p.facing = if p.vx < 0.0 { -1.0 } else { 1.0 };
    true
}

/// WISP_BUDDY_STILL=1, for recording a video by hand: it does nothing on its own (no walks, no
/// jumps), so the mouse finds it where it was a moment ago. Thrown, poked or carried, it still reacts.
fn still() -> bool {
    static STILL: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *STILL.get_or_init(|| std::env::var("WISP_BUDDY_STILL").is_ok_and(|v| v == "1"))
}

/// Pick the next calm thing to do: walk somewhere, stand, or sit for a while.
fn next_activity(p: &mut Pet, a: &Area) {
    let now = Instant::now();
    if still() {
        p.mode = Mode::Idle;
        p.until = now + Duration::from_secs(3600);
        return;
    }
    let r = rand();
    if r < 0.45 {
        p.mode = Mode::Walk;
        // Head away from a nearby edge, otherwise either way.
        let room_left = p.x - a.left;
        let room_right = a.right - SIZE - p.x;
        p.facing = if room_left < 120.0 { 1.0 } else if room_right < 120.0 { -1.0 } else if rand() < 0.5 { -1.0 } else { 1.0 };
        p.until = now + Duration::from_secs_f64(3.0 + rand() * 6.0);
    } else if r < 0.85 {
        p.mode = Mode::Idle;
        p.until = now + Duration::from_secs_f64(3.0 + rand() * 5.0);
    } else {
        p.mode = Mode::Sit;
        p.until = now + Duration::from_secs_f64(6.0 + rand() * 8.0);
    }
}

fn step(app: &AppHandle, win: &WebviewWindow, a: &Area, ledges: &[Ledge], dt: f64) {
    let cursor = app.cursor_position().ok();
    let mut react: Option<&str> = None;
    let mut ignore: Option<bool> = None;
    let mut moved_to: Option<(f64, f64)> = None;
    let mut emit: Option<PetState> = None;

    with(|p| {
        let now = Instant::now();
        let ground = floor(a);
        let min_x = a.left;
        let max_x = a.right - SIZE;

        // A single click, once it is clear no second one follows: a little hop.
        if p.hop_at.is_some_and(|t| now >= t) {
            p.hop_at = None;
            if p.mode != Mode::Held {
                p.mode = Mode::Fall;
                p.vx = 0.0;
                p.vy = -520.0;
                react = Some("poke");
            }
        }

        let cx = p.x + SIZE / 2.0;
        // Where its feet would rest on each window edge, if it stands on one.
        let stand_on = |l: &Ledge| l.y - SIZE * FEET;

        match p.mode {
            Mode::Held => {
                p.on = None;
                if let Some(c) = cursor {
                    let (cx, cy) = (c.x / a.scale, c.y / a.scale);
                    let (nx, ny) = (cx - p.grab.0, cy - p.grab.1);
                    p.moved += (nx - p.x).abs() + (ny - p.y).abs();
                    p.x = nx;
                    p.y = ny;
                    p.trail.push((now, nx, ny));
                    p.trail.retain(|(t, _, _)| now.duration_since(*t) < Duration::from_millis(120));
                }
            }
            Mode::Fall => {
                let feet_before = p.y + SIZE * FEET;
                p.vy += GRAVITY * dt;
                p.x += p.vx * dt;
                p.y += p.vy * dt;
                // Coming down through a window's top edge: land on it, the highest one first.
                let mut surface = ground;
                if p.vy > 0.0 {
                    let feet_after = p.y + SIZE * FEET;
                    let cx = p.x + SIZE / 2.0;
                    let hit = ledges
                        .iter()
                        .filter(|l| cx > l.x1 + 12.0 && cx < l.x2 - 12.0 && feet_before <= l.y + 1.0 && feet_after >= l.y && stand_on(l) < ground)
                        .min_by(|a, b| a.y.total_cmp(&b.y));
                    if let Some(l) = hit {
                        surface = stand_on(l);
                        p.on = Some(l.id);
                        p.ledge_x1 = l.x1;
                    }
                }
                if surface == ground && p.y >= ground {
                    p.on = None;
                }
                if p.x < min_x || p.x > max_x {
                    p.x = p.x.clamp(min_x, max_x);
                    p.vx = -p.vx * 0.5;
                }
                if p.y < a.top {
                    p.y = a.top;
                    p.vy = p.vy.abs() * 0.3;
                }
                if p.y >= surface {
                    p.y = surface;
                    if p.vy > HARD_LANDING {
                        // Bounce once, a bit dazed.
                        p.vy = -p.vy * 0.32;
                        p.vx *= 0.6;
                        react = Some("dizzy");
                    } else {
                        // A real fall gets a proud landing; a click that never left the floor does not.
                        if react.is_none() && p.vy > 250.0 {
                            react = Some("land");
                        }
                        p.vx = 0.0;
                        p.vy = 0.0;
                        p.mode = Mode::Idle;
                        p.until = now + Duration::from_secs_f64(1.5 + rand() * 2.0);
                    }
                }
            }
            _ => {
                // What it stands on: a window's edge (riding along when the window moves), or the
                // floor. A window that closed, shrank or got covered under its feet drops it.
                let mut support = ground;
                let mut edge: Option<Ledge> = None;
                if let Some(id) = p.on {
                    let mine: Vec<&Ledge> = ledges.iter().filter(|l| l.id == id).collect();
                    // The stretch it was on, followed by how far its start moved.
                    let found = mine
                        .iter()
                        .find(|l| cx + (l.x1 - p.ledge_x1) > l.x1 - 4.0 && cx + (l.x1 - p.ledge_x1) < l.x2 + 4.0)
                        .or_else(|| mine.iter().find(|l| cx > l.x1 - 4.0 && cx < l.x2 + 4.0));
                    match found {
                        Some(l) => {
                            let dx = l.x1 - p.ledge_x1;
                            if dx.abs() < 400.0 {
                                p.x += dx;
                            }
                            p.ledge_x1 = l.x1;
                            support = stand_on(l);
                            // Ridden up or down with the window: no fall, just follow.
                            p.y = support;
                            edge = Some(**l);
                        }
                        None => p.on = None,
                    }
                }
                // On its feet. At night it sleeps, unless woken a moment ago or talking.
                let night = !p.talking && (p.sleep_by_hand || (is_night() && p.awake_until.map_or(true, |t| now > t)));
                if night && p.mode != Mode::Sleep {
                    p.mode = Mode::Sleep;
                } else if !night && p.mode == Mode::Sleep {
                    p.mode = Mode::Idle;
                    p.until = now;
                }
                // Off its support (a monitor change, a Dock that grew, a window gone): fall.
                if (p.y - support).abs() > 2.0 {
                    p.mode = Mode::Fall;
                    p.vx = 0.0;
                    p.vy = 0.0;
                } else if p.talking {
                    // Listening: stand still until the bubble closes.
                    p.mode = Mode::Idle;
                    p.until = now + Duration::from_secs(2);
                } else if p.mode != Mode::Sleep {
                    if now >= p.until {
                        // Sometimes up onto a window instead of another stroll.
                        if CLIMB.load(Ordering::Relaxed) && !still() && rand() < jump_chance() && jump_up(p, ledges) {
                            react = Some("jump");
                        } else {
                            next_activity(p, a);
                        }
                    }
                    if p.mode == Mode::Walk {
                        p.x += p.facing * WALK_SPEED * dt;
                        if p.x <= min_x || p.x >= max_x {
                            p.x = p.x.clamp(min_x, max_x);
                            p.facing = -p.facing;
                        }
                        // At the end of a window's edge: mostly turn round, sometimes hop off.
                        if let Some(l) = edge {
                            let cx = p.x + SIZE / 2.0;
                            if cx < l.x1 + 10.0 || cx > l.x2 - 10.0 {
                                if rand() < 0.3 {
                                    p.mode = Mode::Fall;
                                    p.on = None;
                                    p.vx = p.facing * 160.0;
                                    p.vy = -260.0;
                                    react = Some("jump");
                                } else {
                                    p.x = (cx.clamp(l.x1 + 10.0, l.x2 - 10.0)) - SIZE / 2.0;
                                    p.facing = -p.facing;
                                }
                            }
                        }
                    }
                }
            }
        }
        p.x = p.x.clamp(min_x - SIZE * 0.3, max_x + SIZE * 0.3);

        if let Some(c) = cursor {
            if (c.x - p.cursor.0).abs() + (c.y - p.cursor.1).abs() > 2.0 {
                p.cursor = (c.x, c.y);
                p.cursor_moved = now;
            }
        }

        // Clicks go through the window except on the body itself, a circle round its middle.
        if let Some(c) = cursor {
            let (cx, cy) = (c.x / a.scale - p.x, c.y / a.scale - p.y);
            let (bx, by, r) = (SIZE * 0.5, SIZE * 0.6, SIZE * 0.36);
            let on_body = (cx - bx).powi(2) + (cy - by).powi(2) < r * r;
            let through = !on_body && p.mode != Mode::Held;
            if through != p.passthrough {
                p.passthrough = through;
                ignore = Some(through);
            }
        }

        moved_to = Some((p.x, p.y));
        let facing = if p.mode == Mode::Fall && p.vx.abs() > 30.0 { p.vx.signum() } else { p.facing };
        if p.sent != Some((p.mode, facing)) {
            p.sent = Some((p.mode, facing));
            emit = Some(PetState { mode: p.mode, facing });
        }
    });

    if let Some((x, y)) = moved_to {
        let _ = win.set_position(PhysicalPosition::new((x * a.scale).round() as i32, (y * a.scale).round() as i32));
        crate::chat::follow(app, x, y, SIZE, (a.left, a.top, a.right, a.bottom, a.scale));
    }
    if let Some(through) = ignore {
        let _ = win.set_ignore_cursor_events(through);
    }
    if let Some(s) = emit {
        let _ = app.emit_to("pet", "pet", s);
    }
    if let Some(r) = react {
        let _ = app.emit_to("pet", "pet-react", r);
    }
}

pub fn start(app: AppHandle) {
    std::thread::spawn(move || {
        let Some(win) = app.get_webview_window("pet") else { return };
        // The config's size comes out a little smaller on some screens; the body maths needs it exact.
        let _ = win.set_size(tauri::LogicalSize::new(SIZE, SIZE));
        let Some(mut area) = area_of(&win) else { return };
        // Drop in from above the middle of the screen (a third of the way across for a video).
        let start = if demo() { area.left + (area.right - area.left) / 3.0 } else { (area.left + area.right) / 2.0 };
        *PET.lock().unwrap() = Some(Pet {
            x: start - SIZE / 2.0,
            y: area.top,
            vx: 0.0,
            vy: 0.0,
            mode: Mode::Fall,
            facing: 1.0,
            until: Instant::now(),
            awake_until: None,
            sleep_by_hand: false,
            grab: (0.0, 0.0),
            grabbed_at: Instant::now(),
            moved: 0.0,
            trail: vec![],
            passthrough: false,
            talking: false,
            hop_at: None,
            last_click: None,
            cursor: (0.0, 0.0),
            cursor_moved: Instant::now(),
            on: None,
            ledge_x1: 0.0,
            sent: None,
        });
        let _ = win.show();
        let mut last = Instant::now();
        let mut area_at = Instant::now();
        let mut ledges: Vec<Ledge> = vec![];
        let mut ledges_at = Instant::now() - Duration::from_secs(1);
        loop {
            std::thread::sleep(Duration::from_millis(16));
            let now = Instant::now();
            let dt = now.duration_since(last).as_secs_f64().min(0.05);
            last = now;
            // The screen can change (another monitor, the Dock resized); look again every second.
            if now.duration_since(area_at) > Duration::from_secs(1) {
                area_at = now;
                if let Some(a) = area_of(&win) {
                    area = a;
                }
            }
            // Windows move, open and close: look five times a second. Only edges with room for
            // it above, on this screen, and above the floor count.
            if now.duration_since(ledges_at) > Duration::from_millis(200) {
                ledges_at = now;
                ledges = if CLIMB.load(Ordering::Relaxed) {
                    crate::windows::ledges()
                        .into_iter()
                        .filter(|l| l.y - SIZE * FEET >= area.top && l.y < area.bottom - 40.0 && l.x2 > area.left && l.x1 < area.right)
                        .collect()
                } else {
                    vec![]
                };
            }
            step(&app, &win, &area, &ledges, dt);
        }
    });
}

/// The mouse went down on the body: pick it up.
#[tauri::command]
pub fn grab(app: AppHandle) {
    let Some(win) = app.get_webview_window("pet") else { return };
    let Some(a) = area_of(&win) else { return };
    let Ok(c) = app.cursor_position() else { return };
    let woke = with(|p| {
        let woke = p.mode == Mode::Sleep;
        p.mode = Mode::Held;
        p.grab = (c.x / a.scale - p.x, c.y / a.scale - p.y);
        p.grabbed_at = Instant::now();
        p.moved = 0.0;
        p.trail.clear();
        p.vx = 0.0;
        p.vy = 0.0;
        if woke {
            p.awake_until = Some(Instant::now() + Duration::from_secs(120));
            p.sleep_by_hand = false;
        }
        woke
    });
    if woke == Some(true) {
        let _ = app.emit_to("pet", "pet-react", "wake");
    }
}

/// The mouse went up: a short click is a poke (a hop), two quick ones open the chat, anything
/// else is a throw.
#[tauri::command]
pub fn release(app: AppHandle) {
    let double = with(|p| {
        if p.mode != Mode::Held {
            return false;
        }
        let now = Instant::now();
        let quick = p.grabbed_at.elapsed() < Duration::from_millis(350) && p.moved < 6.0;
        p.mode = Mode::Fall;
        if quick {
            p.vx = 0.0;
            p.vy = 0.0;
            if p.last_click.is_some_and(|t| now.duration_since(t) < Duration::from_millis(400)) {
                p.last_click = None;
                p.hop_at = None;
                return true;
            }
            p.last_click = Some(now);
            p.hop_at = Some(now + Duration::from_millis(280));
        } else if let (Some(first), Some(last)) = (p.trail.first(), p.trail.last()) {
            let dt = last.0.duration_since(first.0).as_secs_f64().max(0.016);
            p.vx = ((last.1 - first.1) / dt).clamp(-2500.0, 2500.0);
            p.vy = ((last.2 - first.2) / dt).clamp(-2500.0, 2500.0);
        }
        false
    });
    if double == Some(true) {
        crate::chat::open(&app);
    }
}

/// The chat bubble opened or closed.
pub fn set_talking(on: bool) {
    with(|p| {
        p.talking = on;
        if on {
            p.sleep_by_hand = false;
            p.awake_until = Some(Instant::now() + Duration::from_secs(10 * 60));
        }
    });
}

/// From the tray: bring it to the screen under the mouse, dropping in from above.
pub fn summon(app: &AppHandle) {
    let Some(win) = app.get_webview_window("pet") else { return };
    let Ok(c) = app.cursor_position() else { return };
    let monitor = app.available_monitors().ok().and_then(|ms| {
        ms.into_iter().find(|m| {
            let (p, s) = (m.position(), m.size());
            c.x >= p.x as f64 && c.x < (p.x + s.width as i32) as f64 && c.y >= p.y as f64 && c.y < (p.y + s.height as i32) as f64
        })
    });
    let Some(m) = monitor else { return };
    let scale = m.scale_factor();
    let wa = m.work_area();
    with(|p| {
        p.x = c.x / scale - SIZE / 2.0;
        p.y = wa.position.y as f64 / scale;
        p.vx = 0.0;
        p.vy = 0.0;
        p.mode = Mode::Fall;
        p.awake_until = Some(Instant::now() + Duration::from_secs(120));
        p.sleep_by_hand = false;
    });
    let _ = win.set_position(PhysicalPosition::new((c.x - SIZE / 2.0 * scale) as i32, wa.position.y));
}

/// From the tray: sleep now, or wake up.
pub fn toggle_sleep() -> bool {
    with(|p| {
        p.sleep_by_hand = !p.sleep_by_hand;
        if !p.sleep_by_hand {
            p.awake_until = Some(Instant::now() + Duration::from_secs(30 * 60));
        }
        p.sleep_by_hand
    })
    .unwrap_or(false)
}

/// A reminder is due: wake up, whatever the hour, and stay up a while.
pub fn wake_for_reminder() {
    with(|p| {
        p.sleep_by_hand = false;
        p.awake_until = Some(Instant::now() + Duration::from_secs(5 * 60));
        if p.mode == Mode::Sleep {
            p.mode = Mode::Idle;
            p.until = Instant::now() + Duration::from_secs(3);
        }
    });
}

/// Whether someone is at the Mac: the mouse moved in the last three minutes.
pub fn someone_here() -> bool {
    with(|p| p.cursor_moved.elapsed() < Duration::from_secs(180)).unwrap_or(false)
}
