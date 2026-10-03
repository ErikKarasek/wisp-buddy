//! Other apps' windows, as ledges the buddy can stand on: the visible part of each window's
//! top edge. Read from CoreGraphics, front to back, a few times a second.
//!
//! Bounds and layers need no permission (only window titles would want Screen Recording).
//! Coordinates are global points with the main display's top-left at 0,0, the same as the
//! buddy's logical position on the main display.

use objc2::msg_send;
use objc2::runtime::{AnyClass, AnyObject};
use std::ffi::{c_void, CString};

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGWindowListCopyWindowInfo(option: u32, relative_to: u32) -> *const c_void;
}
#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFRelease(cf: *const c_void);
}

#[derive(Clone, Copy, Debug)]
struct Win {
    id: u32,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

/// A stretch of a window's top edge that nothing covers.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ledge {
    pub id: u32,
    pub y: f64,
    pub x1: f64,
    pub x2: f64,
}

fn ns_string(s: &str) -> *mut AnyObject {
    let c = CString::new(s).unwrap_or_default();
    unsafe { msg_send![AnyClass::get(c"NSString").unwrap(), stringWithUTF8String: c.as_ptr()] }
}

unsafe fn get(dict: *mut AnyObject, key: &str) -> *mut AnyObject {
    msg_send![dict, objectForKey: ns_string(key)]
}

unsafe fn number(obj: *mut AnyObject) -> f64 {
    if obj.is_null() {
        0.0
    } else {
        msg_send![obj, doubleValue]
    }
}

/// Ordinary windows of other apps on screen, front to back.
fn windows() -> Vec<Win> {
    let me = std::process::id() as f64;
    objc2::rc::autoreleasepool(|_| unsafe {
        // On screen only, without the desktop.
        let list = CGWindowListCopyWindowInfo((1 << 0) | (1 << 4), 0);
        if list.is_null() {
            return vec![];
        }
        let arr = list as *mut AnyObject;
        let count: usize = msg_send![arr, count];
        let mut out = Vec::with_capacity(count);
        for i in 0..count {
            let w: *mut AnyObject = msg_send![arr, objectAtIndex: i];
            // Layer 0 is ordinary app windows: not the menu bar, the Dock, Wisp's notch or us.
            if number(get(w, "kCGWindowLayer")) != 0.0 || number(get(w, "kCGWindowOwnerPID")) == me || number(get(w, "kCGWindowAlpha")) < 0.1 {
                continue;
            }
            let b = get(w, "kCGWindowBounds");
            let win = Win { id: number(get(w, "kCGWindowNumber")) as u32, x: number(get(b, "X")), y: number(get(b, "Y")), w: number(get(b, "Width")), h: number(get(b, "Height")) };
            // Tooltips, popovers and other slivers are not furniture.
            if win.w < 160.0 || win.h < 100.0 {
                continue;
            }
            out.push(win);
        }
        CFRelease(list);
        out
    })
}

/// Each window's top edge minus the parts hidden behind windows in front of it.
fn visible_tops(wins: &[Win]) -> Vec<Ledge> {
    let mut out = vec![];
    for (i, w) in wins.iter().enumerate() {
        let mut segs = vec![(w.x, w.x + w.w)];
        for f in &wins[..i] {
            // A window in front covers this edge where its rectangle spans the edge's height.
            if w.y < f.y - 1.0 || w.y > f.y + f.h {
                continue;
            }
            let (c1, c2) = (f.x, f.x + f.w);
            segs = segs
                .into_iter()
                .flat_map(|(a, b)| {
                    let mut keep = vec![];
                    if c1 > a {
                        keep.push((a, c1.min(b)));
                    }
                    if c2 < b {
                        keep.push((c2.max(a), b));
                    }
                    keep
                })
                .filter(|(a, b)| b - a > 1.0)
                .collect();
        }
        for (x1, x2) in segs {
            out.push(Ledge { id: w.id, y: w.y, x1, x2 });
        }
    }
    out
}

pub fn ledges() -> Vec<Ledge> {
    visible_tops(&windows())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn win(id: u32, x: f64, y: f64, w: f64, h: f64) -> Win {
        Win { id, x, y, w, h }
    }

    #[test]
    fn a_window_in_front_hides_the_middle_of_an_edge_behind_it() {
        // Front to back: a small window over the middle of a wide one's top edge.
        let ledges = visible_tops(&[win(2, 400.0, 50.0, 200.0, 300.0), win(1, 100.0, 200.0, 800.0, 500.0)]);
        assert_eq!(ledges[0], Ledge { id: 2, y: 50.0, x1: 400.0, x2: 600.0 });
        assert_eq!(ledges[1], Ledge { id: 1, y: 200.0, x1: 100.0, x2: 400.0 });
        assert_eq!(ledges[2], Ledge { id: 1, y: 200.0, x1: 600.0, x2: 900.0 });
    }

    #[test]
    fn a_window_in_front_but_below_the_edge_hides_nothing() {
        let ledges = visible_tops(&[win(2, 0.0, 600.0, 900.0, 200.0), win(1, 100.0, 200.0, 800.0, 300.0)]);
        assert_eq!(ledges.iter().filter(|l| l.id == 1).count(), 1);
    }
}

#[cfg(test)]
mod live {
    /// `cargo test --lib live_ledges -- --ignored --nocapture` prints what the buddy sees now.
    #[test]
    #[ignore]
    fn live_ledges() {
        for l in super::ledges() {
            println!("{l:?}");
        }
    }
}
