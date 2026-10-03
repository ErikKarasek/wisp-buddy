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

/// The window is square; the character stands with its feet at 92 % of its height. How big
/// is the tray's choice (small, medium, large), in points.
static SIZE_PT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(140);
fn size() -> f64 {
    SIZE_PT.load(Ordering::Relaxed) as f64
}
/// The body's top, as a share of the window from its top: what must stay on the screen. The
/// window above it is empty and may go up under the menu bar.
const HEAD: f64 = 0.12;
const FEET: f64 = 0.92;
const WALK_SPEED: f64 = 42.0;
const GRAVITY: f64 = 2600.0;
/// A landing faster than this bounces, and makes it dizzy.
const HARD_LANDING: f64 = 900.0;
/// How high it can jump up onto a window, in points: any window on a laptop's screen.
const MAX_JUMP: f64 = 1000.0;

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
    /// Where its eyes were last told to look (towards the mouse), to send only changes.
    look_sent: Option<(f64, f64)>,
    /// When it last jumped up somewhere, so a hovering mouse does not make it hop non-stop.
    jumped_at: Instant,
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
    a.bottom - size() * FEET
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
        0.25
    }
}

/// A jump in an arc onto the edge `l`, landing at `target` (its middle's x). `top` is the
/// screen's top (below the menu bar), which its head must not hit on the way.
fn jump_to(p: &mut Pet, l: &Ledge, target: f64, top: f64) {
    let feet = p.y + size() * FEET;
    let cx = p.x + size() / 2.0;
    // Up to a little above the edge, then down onto it: the time decides the sideways speed.
    // Under the menu bar there may be little room above the edge, so less of a hop over it.
    let over = (l.y - size() * (FEET - HEAD) - top - 2.0).clamp(2.0, 36.0);
    let rise = (feet - l.y).max(0.0) + over;
    let vy = (2.0 * GRAVITY * rise).sqrt();
    let t = vy / GRAVITY + (2.0 * over / GRAVITY).sqrt();
    p.mode = Mode::Fall;
    p.on = None;
    p.vy = -vy;
    p.vx = (target.clamp(l.x1 + 30.0, l.x2 - 30.0) - cx) / t;
    p.facing = if p.vx < 0.0 { -1.0 } else { 1.0 };
    p.jumped_at = Instant::now();
}

/// Whether it can jump from where it stands up onto `l`: high enough to bother, low enough to
/// reach, near enough sideways, and wide enough to land on.
fn reachable(p: &Pet, l: &Ledge) -> bool {
    let feet = p.y + size() * FEET;
    let cx = p.x + size() / 2.0;
    l.x2 - l.x1 >= 120.0 && feet - l.y > 60.0 && feet - l.y < MAX_JUMP && ((l.x1 + l.x2) / 2.0 - cx).abs() < 700.0
}

/// Now and then, a jump up onto a window it can reach. Returns whether it jumped.
fn jump_up(p: &mut Pet, ledges: &[Ledge], top: f64) -> bool {
    let options: Vec<Ledge> = ledges.iter().filter(|l| reachable(p, l)).copied().collect();
    if options.is_empty() {
        return false;
    }
    let l = options[(rand() * options.len() as f64) as usize % options.len()];
    let target = l.x1 + 50.0 + rand() * (l.x2 - l.x1 - 100.0).max(0.0);
    jump_to(p, &l, target, top);
    true
}

/// WISP_BUDDY_STILL=1, for recording a video by hand: it does nothing on its own (no walks, no
/// jumps), so the mouse finds it where it was a moment ago. Thrown, poked or carried, it still reacts.
fn still() -> bool {
    static STILL: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *STILL.get_or_init(|| std::env::var("WISP_BUDDY_STILL").is_ok_and(|v| v == "1"))
}

/// Pick the next calm thing to do: walk somewhere, stand, or sit for a while.
/// Pick the next thing to do, and the reaction it comes with (a little game is one).
fn next_activity(p: &mut Pet, a: &Area, cursor: Option<(f64, f64)>) -> Option<&'static str> {
    let now = Instant::now();
    if still() {
        p.mode = Mode::Idle;
        p.until = now + Duration::from_secs(3600);
        return None;
    }
    let cx = p.x + size() / 2.0;
    let feet = p.y + size() * FEET;
    // You are around: the mouse moved in the last few seconds, low on the screen, near enough
    // to bother about. Then it comes over, or sits down next to a mouse that has stopped.
    if let Some((mx, my)) = cursor {
        let lively = p.cursor_moved.elapsed() < Duration::from_secs(4);
        let near_floor = my > feet - 320.0 && my < feet + 40.0;
        let dx = mx - cx;
        // Working higher up in a window: it still keeps you company, walking along underneath,
        // just less eagerly than when the mouse is down beside it.
        if lively && !near_floor && my < feet && dx.abs() > 140.0 && rand() < 0.35 {
            p.mode = Mode::Walk;
            p.facing = dx.signum();
            p.until = now + Duration::from_secs_f64((dx.abs() - 100.0) / WALK_SPEED);
            return None;
        }
        if near_floor && dx.abs() < 600.0 {
            if lively && dx.abs() > 110.0 && rand() < 0.6 {
                p.mode = Mode::Walk;
                p.facing = dx.signum();
                p.until = now + Duration::from_secs_f64((dx.abs() - 80.0) / WALK_SPEED);
                return None;
            }
            if !lively && p.cursor_moved.elapsed() < Duration::from_secs(30) && dx.abs() <= 160.0 && rand() < 0.5 {
                p.mode = Mode::Sit;
                p.until = now + Duration::from_secs_f64(8.0 + rand() * 8.0);
                return None;
            }
        }
    }
    let r = rand();
    // Now and then a little game on its own: a hop, a somersault, a wiggle.
    if r < 0.12 {
        p.mode = Mode::Idle;
        p.until = now + Duration::from_secs_f64(2.0 + rand() * 2.0);
        let game = rand();
        if game < 0.4 {
            p.mode = Mode::Fall;
            p.vx = 0.0;
            p.vy = -440.0;
            return Some("hop");
        }
        return Some(if game < 0.7 { "spin" } else { "wiggle" });
    }
    if r < 0.5 {
        p.mode = Mode::Walk;
        // Head away from a nearby edge, otherwise either way.
        let room_left = p.x - a.left;
        let room_right = a.right - size() - p.x;
        p.facing = if room_left < 120.0 { 1.0 } else if room_right < 120.0 { -1.0 } else if rand() < 0.5 { -1.0 } else { 1.0 };
        p.until = now + Duration::from_secs_f64(3.0 + rand() * 6.0);
    } else if r < 0.86 {
        p.mode = Mode::Idle;
        p.until = now + Duration::from_secs_f64(3.0 + rand() * 5.0);
    } else {
        p.mode = Mode::Sit;
        p.until = now + Duration::from_secs_f64(6.0 + rand() * 8.0);
    }
    None
}

fn step(app: &AppHandle, win: &WebviewWindow, a: &Area, ledges: &[Ledge], dt: f64) {
    let cursor = app.cursor_position().ok();
    let mut react: Option<&str> = None;
    let mut ignore: Option<bool> = None;
    let mut moved_to: Option<(f64, f64)> = None;
    let mut emit: Option<PetState> = None;
    let mut look_emit: Option<Option<(f64, f64)>> = None;

    with(|p| {
        let now = Instant::now();
        let ground = floor(a);
        let min_x = a.left;
        let max_x = a.right - size();

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

        let cx = p.x + size() / 2.0;
        // Where its feet would rest on each window edge, if it stands on one.
        let stand_on = |l: &Ledge| l.y - size() * FEET;

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
                let feet_before = p.y + size() * FEET;
                p.vy += GRAVITY * dt;
                p.x += p.vx * dt;
                p.y += p.vy * dt;
                // Coming down through a window's top edge: land on it, the highest one first.
                let mut surface = ground;
                if p.vy > 0.0 {
                    let feet_after = p.y + size() * FEET;
                    let cx = p.x + size() / 2.0;
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
                // The ceiling is the menu bar for its head, not for the empty top of its window.
                let ceiling = a.top - size() * HEAD;
                if p.y < ceiling {
                    p.y = ceiling;
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
                    // The mouse resting on a window's top edge it can reach: it jumps up there.
                    if !still() && CLIMB.load(Ordering::Relaxed) && p.mode != Mode::Fall && p.jumped_at.elapsed() > Duration::from_secs(4) {
                        if let Some(c) = cursor {
                            let (mx, my) = (c.x / a.scale, c.y / a.scale);
                            let hovered = ledges
                                .iter()
                                .find(|l| Some(l.id) != p.on && (my - l.y).abs() < 28.0 && mx > l.x1 + 20.0 && mx < l.x2 - 20.0 && reachable(p, l))
                                .copied();
                            if let Some(l) = hovered {
                                jump_to(p, &l, mx, a.top);
                                react = Some("jump");
                            }
                        }
                    }
                    // A mouse moving nearby gets noticed now, not when the current rest ends.
                    let mut woke_early = false;
                    if !still() && (p.mode == Mode::Idle || p.mode == Mode::Sit) && p.cursor_moved.elapsed() < Duration::from_millis(400) {
                        if let Some(c) = cursor {
                            let (mx, my) = (c.x / a.scale, c.y / a.scale);
                            let feet = p.y + size() * FEET;
                            let dx = mx - (p.x + size() / 2.0);
                            if my > feet - 320.0 && my < feet + 40.0 && dx.abs() > 110.0 && dx.abs() < 600.0 && p.until > now + Duration::from_millis(800) {
                                p.until = now;
                                woke_early = true;
                            }
                        }
                    }
                    // Already off on a jump to the mouse: no new plan on top of it.
                    if now >= p.until && p.mode != Mode::Fall {
                        // Sometimes up onto a window instead of another stroll, but not when the
                        // mouse just woke it: that is for walking over to the mouse.
                        if CLIMB.load(Ordering::Relaxed) && !still() && !woke_early && rand() < jump_chance() && jump_up(p, ledges, a.top) {
                            react = Some("jump");
                        } else {
                            let c = cursor.map(|c| (c.x / a.scale, c.y / a.scale));
                            if let Some(r) = next_activity(p, a, c) {
                                react = Some(r);
                            }
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
                            let cx = p.x + size() / 2.0;
                            if cx < l.x1 + 10.0 || cx > l.x2 - 10.0 {
                                if rand() < 0.3 {
                                    p.mode = Mode::Fall;
                                    p.on = None;
                                    p.vx = p.facing * 160.0;
                                    p.vy = -260.0;
                                    react = Some("jump");
                                } else {
                                    p.x = (cx.clamp(l.x1 + 10.0, l.x2 - 10.0)) - size() / 2.0;
                                    p.facing = -p.facing;
                                }
                            }
                        }
                    }
                }
            }
        }
        p.x = p.x.clamp(min_x - size() * 0.3, max_x + size() * 0.3);

        if let Some(c) = cursor {
            if (c.x - p.cursor.0).abs() + (c.y - p.cursor.1).abs() > 2.0 {
                p.cursor = (c.x, c.y);
                p.cursor_moved = now;
            }
        }

        // Clicks go through the window except on the body itself, a circle round its middle.
        if let Some(c) = cursor {
            let (cx, cy) = (c.x / a.scale - p.x, c.y / a.scale - p.y);
            let (bx, by, r) = (size() * 0.5, size() * 0.6, size() * 0.36);
            let on_body = (cx - bx).powi(2) + (cy - by).powi(2) < r * r;
            let through = !on_body && p.mode != Mode::Held;
            if through != p.passthrough {
                p.passthrough = through;
                ignore = Some(through);
            }
        }

        // Eyes on the mouse when it is about (moved lately, within reach); straight ahead otherwise.
        let look = cursor.and_then(|c| {
            let (dx, dy) = (c.x / a.scale - (p.x + size() / 2.0), c.y / a.scale - (p.y + size() * 0.6));
            let near = dx.abs() < 500.0 && dy.abs() < 400.0 && p.cursor_moved.elapsed() < Duration::from_secs(6);
            near.then(|| (((dx / 220.0).clamp(-1.0, 1.0) * 10.0).round() / 10.0, ((dy / 220.0).clamp(-1.0, 1.0) * 10.0).round() / 10.0))
        });
        if look != p.look_sent {
            p.look_sent = look;
            look_emit = Some(look);
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
        crate::chat::follow(app, x, y, size(), (a.left, a.top, a.right, a.bottom, a.scale));
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
    if let Some(look) = look_emit {
        let _ = app.emit_to("pet", "pet-look", look.map(|(x, y)| [x, y]));
    }
}

pub fn start(app: AppHandle) {
    std::thread::spawn(move || {
        let Some(win) = app.get_webview_window("pet") else { return };
        // The config's size comes out a little smaller on some screens; the body maths needs it exact.
        let _ = win.set_size(tauri::LogicalSize::new(size(), size()));
        let Some(mut area) = area_of(&win) else { return };
        // Drop in from above the middle of the screen (a third of the way across for a video).
        let start = if demo() { area.left + (area.right - area.left) / 3.0 } else { (area.left + area.right) / 2.0 };
        *PET.lock().unwrap() = Some(Pet {
            x: start - size() / 2.0,
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
            look_sent: None,
            jumped_at: Instant::now(),
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
                        .filter(|l| l.y - size() * (FEET - HEAD) >= area.top && l.y < area.bottom - 40.0 && l.x2 > area.left && l.x1 < area.right)
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
        p.x = c.x / scale - size() / 2.0;
        p.y = wa.position.y as f64 / scale;
        p.vx = 0.0;
        p.vy = 0.0;
        p.mode = Mode::Fall;
        p.awake_until = Some(Instant::now() + Duration::from_secs(120));
        p.sleep_by_hand = false;
    });
    let _ = win.set_position(PhysicalPosition::new((c.x - size() / 2.0 * scale) as i32, wa.position.y));
}

/// From the tray: small, medium or large. The window resizes around its feet, so it stays
/// standing where it stood.
pub fn set_size(app: &AppHandle, pts: u32) {
    let pts = pts.clamp(80, 240);
    let old = size();
    SIZE_PT.store(pts, Ordering::Relaxed);
    let new = size();
    with(|p| {
        p.x += (old - new) / 2.0;
        p.y += (old - new) * FEET;
    });
    if let Some(win) = app.get_webview_window("pet") {
        let _ = win.set_size(tauri::LogicalSize::new(new, new));
    }
}

/// The size to start with, before the window shows (from the config).
pub fn init_size(pts: u32) {
    SIZE_PT.store(pts.clamp(80, 240), Ordering::Relaxed);
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
