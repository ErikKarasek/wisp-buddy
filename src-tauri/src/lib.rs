//! Wisp Buddy: a character from Wisp's family that lives on the desktop. It walks along the
//! bottom of the screen, can be picked up and thrown, and sleeps at night.

mod chat;
mod config;
mod pet;
mod reminders;
mod windows;
mod wisp;

use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
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
    let climbing = config::field(app.handle(), "climb").and_then(|v| v.as_bool()).unwrap_or(true);
    pet::set_climb(climbing);
    let climb = CheckMenuItem::with_id(app, "climb", "Leze po oknech", true, climbing, None::<&str>)?;
    let linked = config::field(app.handle(), "wisp").and_then(|v| v.as_bool()).unwrap_or(true);
    wisp::set_on(linked);
    let wisp_link = CheckMenuItem::with_id(app, "wisp", "Propojit s Wispem", true, linked, None::<&str>)?;
    let to_phone = config::field(app.handle(), "phone").and_then(|v| v.as_bool()).unwrap_or(true);
    let phone = CheckMenuItem::with_id(app, "phone", "Připomínky i na telefon", true, to_phone, None::<&str>)?;
    // When to be sent to bed, if at all.
    let bed_now = reminders::bedtime_setting(app.handle());
    let beds: Vec<CheckMenuItem<tauri::Wry>> = [("23:00", "Ve 23:00"), ("00:00", "O půlnoci"), ("01:00", "V 1:00"), ("off", "Vůbec")]
        .iter()
        .map(|(id, label)| {
            let on = bed_now.as_deref().unwrap_or("off") == *id;
            CheckMenuItem::with_id(app, format!("bed:{id}"), *label, true, on, None::<&str>)
        })
        .collect::<Result<_, _>>()?;
    let bed_refs: Vec<&dyn tauri::menu::IsMenuItem<tauri::Wry>> = beds.iter().map(|b| b as &dyn tauri::menu::IsMenuItem<tauri::Wry>).collect();
    let bedtime = Submenu::with_items(app, "Poslat mě spát", true, &bed_refs)?;
    // How big it is. Smaller needs less room above a window to climb onto it.
    let size_now = config::field(app.handle(), "size").and_then(|v| v.as_u64()).unwrap_or(140) as u32;
    pet::init_size(size_now);
    let sizes: Vec<CheckMenuItem<tauri::Wry>> = [(100u32, "Malý"), (140, "Střední"), (180, "Velký")]
        .iter()
        .map(|(pts, label)| CheckMenuItem::with_id(app, format!("size:{pts}"), *label, true, *pts == size_now, None::<&str>))
        .collect::<Result<_, _>>()?;
    let size_refs: Vec<&dyn tauri::menu::IsMenuItem<tauri::Wry>> = sizes.iter().map(|b| b as &dyn tauri::menu::IsMenuItem<tauri::Wry>).collect();
    let size_menu = Submenu::with_items(app, "Velikost", true, &size_refs)?;
    let quit = MenuItem::with_id(app, "quit", "Ukončit Wisp Buddy", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&summon, &talk, &studio, &size_menu, &sleep, &motion, &climb, &wisp_link, &phone, &bedtime, &PredefinedMenuItem::separator(app)?, &quit])?;

    let sleep_item = sleep.clone();
    let motion_item = motion.clone();
    let climb_item = climb.clone();
    let wisp_item = wisp_link.clone();
    let phone_item = phone.clone();
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
            "wisp" => {
                let on = wisp_item.is_checked().unwrap_or(true);
                wisp::set_on(on);
                config::set_field(app, "wisp", serde_json::Value::Bool(on));
            }
            "phone" => {
                let on = phone_item.is_checked().unwrap_or(true);
                config::set_field(app, "phone", serde_json::Value::Bool(on));
            }
            "climb" => {
                let on = climb_item.is_checked().unwrap_or(true);
                pet::set_climb(on);
                config::set_field(app, "climb", serde_json::Value::Bool(on));
            }
            id if id.starts_with("size:") => {
                let pts: u32 = id[5..].parse().unwrap_or(140);
                for b in &sizes {
                    let _ = b.set_checked(b.id().as_ref() == id);
                }
                pet::set_size(app, pts);
                config::set_field(app, "size", serde_json::Value::from(pts));
            }
            id if id.starts_with("bed:") => {
                let choice = &id[4..];
                for b in &beds {
                    let _ = b.set_checked(b.id().as_ref() == id);
                }
                let value = if choice == "off" { serde_json::Value::Null } else { serde_json::Value::String(choice.into()) };
                config::set_field(app, "bedtime", value);
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
            chat::chat_send, chat::chat_close, chat::gemini_key_set, chat::gemini_key_present, chat::gemini_key_forget, open_url,
            reminders::reminder_list, reminders::reminder_remove, reminders::reminder_snooze])
        .setup(|app| {
            // A buddy, not an app to switch to: no Dock icon, no menu bar of its own.
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            tray(app)?;
            pet::start(app.handle().clone());
            reminders::start(app.handle().clone());
            wisp::start(app.handle().clone());
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("Wisp Buddy failed to start");
}
