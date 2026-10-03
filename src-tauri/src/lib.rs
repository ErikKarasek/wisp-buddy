//! Wisp Buddy: a character from Wisp's family that lives on the desktop. It walks along the
//! bottom of the screen, can be picked up and thrown, and sleeps at night.

mod pet;

use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{App, Emitter};

fn tray(app: &App) -> tauri::Result<()> {
    let summon = MenuItem::with_id(app, "summon", "Zavolat sem", true, None::<&str>)?;
    let sleep = CheckMenuItem::with_id(app, "sleep", "Spát", true, false, None::<&str>)?;
    let motion = CheckMenuItem::with_id(app, "motion", "Tvary se hýbou", true, true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Ukončit Wisp Buddy", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&summon, &sleep, &motion, &PredefinedMenuItem::separator(app)?, &quit])?;

    let sleep_item = sleep.clone();
    let motion_item = motion.clone();
    TrayIconBuilder::with_id("buddy")
        .icon(app.default_window_icon().cloned().expect("an app icon"))
        .icon_as_template(false)
        .tooltip("Wisp Buddy")
        .menu(&menu)
        .on_menu_event(move |app, e| match e.id.as_ref() {
            "summon" => pet::summon(app),
            "sleep" => {
                let asleep = pet::toggle_sleep();
                let _ = sleep_item.set_checked(asleep);
            }
            "motion" => {
                let on = motion_item.is_checked().unwrap_or(true);
                let _ = app.emit_to("pet", "shape-motion", on);
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .build(app)?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![pet::grab, pet::release])
        .setup(|app| {
            // A buddy, not an app to switch to: no Dock icon, no menu bar of its own.
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            tray(app)?;
            pet::start(app.handle().clone());
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("Wisp Buddy failed to start");
}
