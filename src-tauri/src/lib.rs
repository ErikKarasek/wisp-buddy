//! Wisp Buddy: a character from Wisp's family that lives on the desktop. It walks along the
//! bottom of the screen, can be picked up and thrown, and sleeps at night.

mod chat;
mod config;
mod pet;

use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{App, AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

/// A web link from a page (the AI Studio link in the chat), in the default browser. Only https.
#[tauri::command]
fn open_url(url: String) -> Result<(), String> {
    if !url.starts_with("https://") {
        return Err("only https links".into());
    }
    std::process::Command::new("/usr/bin/open").arg(&url).spawn().map(|_| ()).map_err(|e| e.to_string())
}

/// The studio window, made when first asked for and brought to the front after that.
fn open_studio(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("studio") {
        let _ = w.show();
        let _ = w.set_focus();
        return;
    }
    let made = WebviewWindowBuilder::new(app, "studio", WebviewUrl::App("index.html?view=studio".into()))
        .title("Postavička · Wisp Buddy")
        .inner_size(860.0, 600.0)
        .min_inner_size(720.0, 520.0)
        .background_color(tauri::window::Color(22, 23, 27, 255))
        .build();
    if let Ok(w) = made {
        let _ = w.set_focus();
    }
}

fn tray(app: &App) -> tauri::Result<()> {
    let summon = MenuItem::with_id(app, "summon", "Zavolat sem", true, None::<&str>)?;
    let studio = MenuItem::with_id(app, "studio", "Postavička…", true, None::<&str>)?;
    let talk = MenuItem::with_id(app, "talk", "Povídat si", true, None::<&str>)?;
    let sleep = CheckMenuItem::with_id(app, "sleep", "Spát", true, false, None::<&str>)?;
    let moving = config::field(app.handle(), "shapeMotion").and_then(|v| v.as_bool()).unwrap_or(true);
    let motion = CheckMenuItem::with_id(app, "motion", "Tvary se hýbou", true, moving, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Ukončit Wisp Buddy", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&summon, &talk, &studio, &sleep, &motion, &PredefinedMenuItem::separator(app)?, &quit])?;

    let sleep_item = sleep.clone();
    let motion_item = motion.clone();
    TrayIconBuilder::with_id("buddy")
        .icon(app.default_window_icon().cloned().expect("an app icon"))
        .icon_as_template(false)
        .tooltip("Wisp Buddy")
        .menu(&menu)
        .on_menu_event(move |app, e| match e.id.as_ref() {
            "summon" => pet::summon(app),
            "studio" => open_studio(app),
            "talk" => chat::open(app),
            "sleep" => {
                let asleep = pet::toggle_sleep();
                let _ = sleep_item.set_checked(asleep);
            }
            "motion" => {
                let on = motion_item.is_checked().unwrap_or(true);
                config::set_field(app, "shapeMotion", serde_json::Value::Bool(on));
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
        .invoke_handler(tauri::generate_handler![pet::grab, pet::release, config::config_load, config::config_save, config::wisp_characters,
            chat::chat_send, chat::chat_close, chat::gemini_key_set, chat::gemini_key_present, chat::gemini_key_forget, open_url])
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
