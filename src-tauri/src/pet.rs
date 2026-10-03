//! The buddy's body: where it is on the screen, what it is doing, and how it moves. A thread
//! steps it sixty times a second and moves the window; the page only draws the character and
//! hears what to look like ("pet" events) and when to react ("pet-react").
//!
//! Distances are in logical points and converted with the monitor's scale when the window
//! moves, so the buddy walks at the same pace on a Retina screen and an external one.

use serde::Serialize;
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

/// Pick the next calm thing to do: walk somewhere, stand, or sit for a while.
fn next_activity(p: &mut Pet, a: &Area) {
    let now = Instant::now();
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

fn step(app: &AppHandle, win: &WebviewWindow, a: &Area, dt: f64) {
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

        match p.mode {
            Mode::Held => {
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
                p.vy += GRAVITY * dt;
                p.x += p.vx * dt;
                p.y += p.vy * dt;
                if p.x < min_x || p.x > max_x {
                    p.x = p.x.clamp(min_x, max_x);
                    p.vx = -p.vx * 0.5;
                }
                if p.y < a.top {
                    p.y = a.top;
                    p.vy = p.vy.abs() * 0.3;
                }
                if p.y >= ground {
                    p.y = ground;
                    if p.vy > HARD_LANDING {
                        // Bounce once, a bit dazed.
                        p.vy = -p.vy * 0.32;
                        p.vx *= 0.6;
                        react = Some("dizzy");
                    } else {
                        p.vx = 0.0;
                        p.vy = 0.0;
                        p.mode = Mode::Idle;
                        p.until = now + Duration::from_secs_f64(1.5 + rand() * 2.0);
                        if react.is_none() {
                            react = Some("land");
                        }
                    }
                }
            }
            _ => {
                // On the floor. At night it sleeps, unless woken a moment ago.
                let night = p.sleep_by_hand || (is_night() && p.awake_until.map_or(true, |t| now > t));
                if night && p.mode != Mode::Sleep {
                    p.mode = Mode::Sleep;
                } else if !night && p.mode == Mode::Sleep {
                    p.mode = Mode::Idle;
                    p.until = now;
                }
                // Moved off the floor (a monitor change, a Dock that grew): fall back down.
                if (p.y - ground).abs() > 2.0 {
                    p.mode = Mode::Fall;
                    p.vx = 0.0;
                    p.vy = 0.0;
                } else if p.mode != Mode::Sleep {
                    if now >= p.until {
                        next_activity(p, a);
                    }
                    if p.mode == Mode::Walk {
                        p.x += p.facing * WALK_SPEED * dt;
                        if p.x <= min_x || p.x >= max_x {
                            p.x = p.x.clamp(min_x, max_x);
                            p.facing = -p.facing;
                        }
                    }
                }
            }
        }
        p.x = p.x.clamp(min_x - SIZE * 0.3, max_x + SIZE * 0.3);

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
        // Drop in from above the middle of the screen.
        *PET.lock().unwrap() = Some(Pet {
            x: (area.left + area.right) / 2.0 - SIZE / 2.0,
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
            sent: None,
        });
        let _ = win.show();
        let mut last = Instant::now();
        let mut area_at = Instant::now();
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
            step(&app, &win, &area, dt);
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

/// The mouse went up: a short click is a poke (a hop), anything else is a throw.
#[tauri::command]
pub fn release(app: AppHandle) {
    let poke = with(|p| {
        if p.mode != Mode::Held {
            return false;
        }
        let quick = p.grabbed_at.elapsed() < Duration::from_millis(350) && p.moved < 6.0;
        p.mode = Mode::Fall;
        if quick {
            p.vx = 0.0;
            p.vy = -520.0;
        } else if let (Some(first), Some(last)) = (p.trail.first(), p.trail.last()) {
            let dt = last.0.duration_since(first.0).as_secs_f64().max(0.016);
            p.vx = ((last.1 - first.1) / dt).clamp(-2500.0, 2500.0);
            p.vy = ((last.2 - first.2) / dt).clamp(-2500.0, 2500.0);
        }
        quick
    });
    if poke == Some(true) {
        let _ = app.emit_to("pet", "pet-react", "poke");
    }
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
