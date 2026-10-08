//! The home: the Windows PC and the Samsung TV, on the same network as the Mac.
//!
//! The PC wakes with a Wake-on-LAN packet (it is on a cable, and WoL is switched on in its BIOS
//! and network card). Switching it off, restarting or putting it to sleep goes over SSH, when
//! Windows' own OpenSSH server is on and the Mac's key is allowed there. The TV listens on its
//! local remote-control socket (Tizen, port 8002); the first time it asks on screen whether to
//! allow "Wisp Buddy", and the token it then gives is kept. A TV in deep standby wakes with WoL.
//!
//! Settings in config.json under "home": { pc: { mac, ip, ssh }, tv: { ip, mac, token, name } },
//! filled in by talking (the chat's home_setup and find_devices).

use base64::Engine;
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::net::{IpAddr, SocketAddr, TcpStream, UdpSocket};
use std::process::Command;
use std::time::{Duration, Instant};
use tauri::AppHandle;
use tungstenite::Message;

use crate::config;

// ---------- settings ----------

pub fn settings(app: &AppHandle) -> Value {
    config::field(app, "home").filter(Value::is_object).unwrap_or_else(|| json!({}))
}

fn set(app: &AppHandle, device: &str, key: &str, value: Value) {
    config::update_field(app, "home", |mut v| {
        if !v.is_object() {
            v = json!({});
        }
        if !v[device].is_object() {
            v[device] = json!({});
        }
        v[device][key] = value;
        v
    });
}

fn get(app: &AppHandle, device: &str, key: &str) -> Option<String> {
    settings(app)[device][key].as_str().map(String::from).filter(|s| !s.is_empty())
}

/// "1C-69-7A-0A-B-cc" or "1c:69:7a:a:b:cc" → "1c:69:7a:0a:0b:cc".
pub fn norm_mac(s: &str) -> Option<String> {
    let parts: Vec<&str> = s.trim().split([':', '-']).collect();
    if parts.len() != 6 {
        return None;
    }
    let bytes: Option<Vec<u8>> = parts.iter().map(|p| if p.len() <= 2 { u8::from_str_radix(p, 16).ok() } else { None }).collect();
    Some(bytes?.iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(":"))
}

fn valid_ip(s: &str) -> Option<String> {
    s.trim().parse::<IpAddr>().ok().map(|ip| ip.to_string())
}

/// Saves what the chat learnt: a MAC address, an IP address, the SSH login for the PC.
pub fn setup(app: &AppHandle, device: &str, mac: Option<&str>, ip: Option<&str>, ssh: Option<&str>) -> Value {
    if device != "pc" && device != "tv" {
        return json!({ "error": "device is pc or tv" });
    }
    let mut saved = vec![];
    if let Some(m) = mac.filter(|m| !m.trim().is_empty()) {
        let Some(m) = norm_mac(m) else { return json!({ "error": "that is not a MAC address (six pairs like 1c:69:7a:0a:0b:cc)" }) };
        set(app, device, "mac", json!(m));
        saved.push("mac");
    }
    if let Some(i) = ip.filter(|i| !i.trim().is_empty()) {
        let Some(i) = valid_ip(i) else { return json!({ "error": "that is not an IP address" }) };
        set(app, device, "ip", json!(i));
        saved.push("ip");
        // The TV tells its own MAC and name, for waking it later. Its "wifiMac" is only the
        // address that wakes it when it is on Wi-Fi; a MAC from the network itself is better.
        if device == "tv" {
            if let Some(info) = samsung_info(&i) {
                let wireless = info["device"]["networkType"].as_str() != Some("wired");
                if let Some(m) = info["device"]["wifiMac"].as_str().and_then(norm_mac).filter(|_| wireless && !saved.contains(&"mac")) {
                    set(app, "tv", "mac", json!(m));
                    saved.push("mac (from the TV)");
                }
                if let Some(n) = info["device"]["name"].as_str().or(info["name"].as_str()) {
                    set(app, "tv", "name", json!(n));
                }
            }
        }
    }
    if let Some(s) = ssh.filter(|s| !s.trim().is_empty()) {
        if device != "pc" {
            return json!({ "error": "ssh is for the pc" });
        }
        let s = s.trim();
        // user@host, nothing that a shell could read as more.
        if !s.chars().all(|c| c.is_alphanumeric() || "@._-".contains(c)) || s.starts_with('-') {
            return json!({ "error": "ssh login is user@address" });
        }
        set(app, "pc", "ssh", json!(s));
        saved.push("ssh");
    }
    json!({ "ok": !saved.is_empty(), "saved": saved, "now": settings(app) })
}

// ---------- the network ----------

#[derive(Debug, PartialEq)]
pub struct Device {
    pub ip: String,
    pub mac: String,
    pub name: Option<String>,
}

/// What `arp -a` prints: "desktop-pc.home (192.168.1.23) at 1c:69:7a:a:b:cc on en0 ifscope [ethernet]".
pub fn parse_arp(text: &str) -> Vec<Device> {
    text.lines()
        .filter_map(|line| {
            let (name, rest) = line.split_once(" (")?;
            let (ip, rest) = rest.split_once(") at ")?;
            let mac = norm_mac(rest.split_whitespace().next()?)?;
            // Broadcast and multicast are not devices.
            if mac == "ff:ff:ff:ff:ff:ff" || mac.starts_with("01:00:5e") || ip.ends_with(".255") {
                return None;
            }
            Some(Device { ip: ip.to_string(), mac, name: (name != "?").then(|| name.to_string()) })
        })
        .collect()
}

fn open(ip: &str, port: u16, ms: u64) -> bool {
    let Ok(ip) = ip.parse::<IpAddr>() else { return false };
    TcpStream::connect_timeout(&SocketAddr::new(ip, port), Duration::from_millis(ms)).is_ok()
}

/// A plain HTTP GET on the home network, for the TV's description.
fn http_get(ip: &str, port: u16, path: &str) -> Option<String> {
    let addr = SocketAddr::new(ip.parse().ok()?, port);
    let mut s = TcpStream::connect_timeout(&addr, Duration::from_millis(700)).ok()?;
    s.set_read_timeout(Some(Duration::from_secs(2))).ok()?;
    write!(s, "GET {path} HTTP/1.1\r\nHost: {ip}:{port}\r\nConnection: close\r\n\r\n").ok()?;
    let mut raw = Vec::new();
    let _ = s.read_to_end(&mut raw);
    let raw = String::from_utf8_lossy(&raw).into_owned();
    let (head, body) = raw.split_once("\r\n\r\n")?;
    head.starts_with("HTTP/1.1 200").then(|| body.to_string())
}

/// A Samsung TV describes itself on port 8001: name, model, MAC, whether it is on or in standby.
fn samsung_info(ip: &str) -> Option<Value> {
    let body = http_get(ip, 8001, "/api/v2/")?;
    let start = body.find('{')?;
    let v: Value = serde_json::from_str(body[start..].trim_end_matches(|c: char| c != '}')).ok()?;
    v["device"].is_object().then_some(v)
}

/// The devices the Mac has seen on the network lately, each with a guess at what it is.
pub fn find_devices(app: &AppHandle) -> Value {
    let out = Command::new("/usr/sbin/arp").arg("-a").output().map(|o| String::from_utf8_lossy(&o.stdout).into_owned()).unwrap_or_default();
    let found = parse_arp(&out);
    let threads: Vec<_> = found
        .into_iter()
        .map(|d| {
            std::thread::spawn(move || {
                let mut what = vec![];
                if let Some(info) = samsung_info(&d.ip) {
                    what.push(format!("Samsung TV „{}“", info["device"]["name"].as_str().or(info["name"].as_str()).unwrap_or("?")));
                } else if d.name.as_deref().is_some_and(|n| n.to_lowercase().contains("samsung")) {
                    // The router knows it by name even while it sleeps and keeps quiet.
                    what.push("Samsung TV (asi teď vypnutá)".to_string());
                }
                if open(&d.ip, 445, 400) || open(&d.ip, 3389, 400) {
                    what.push("Windows (sdílení souborů nebo vzdálená plocha)".into());
                }
                if open(&d.ip, 22, 400) {
                    what.push("SSH".into());
                }
                json!({ "ip": d.ip, "mac": d.mac, "name": d.name, "looks_like": what })
            })
        })
        .collect();
    let list: Vec<Value> = threads.into_iter().filter_map(|t| t.join().ok()).collect();
    // One TV and none set yet: that is the one.
    let tvs: Vec<&Value> = list.iter().filter(|d| d["looks_like"].as_array().is_some_and(|a| a.iter().any(|w| w.as_str().is_some_and(|w| w.starts_with("Samsung"))))).collect();
    let mut note = String::new();
    if tvs.len() == 1 && get(app, "tv", "ip").is_none() {
        // The MAC the network sees is the one that wakes it.
        setup(app, "tv", tvs[0]["mac"].as_str(), tvs[0]["ip"].as_str(), None);
        note = "The Samsung TV was saved as the TV.".into();
    }
    json!({ "devices": list, "note": note, "hint": "Only devices the Mac talked to lately are listed; a PC that is off is usually missing." })
}

// ---------- Wake-on-LAN ----------

pub fn magic_packet(mac: &str) -> Option<Vec<u8>> {
    let mac = norm_mac(mac)?;
    let bytes: Vec<u8> = mac.split(':').filter_map(|p| u8::from_str_radix(p, 16).ok()).collect();
    let mut packet = vec![0xff; 6];
    for _ in 0..16 {
        packet.extend_from_slice(&bytes);
    }
    Some(packet)
}

fn wake_on_lan(mac: &str, ip: Option<&str>) -> Result<(), String> {
    let packet = magic_packet(mac).ok_or("bad MAC address")?;
    let socket = UdpSocket::bind("0.0.0.0:0").map_err(|e| e.to_string())?;
    socket.set_broadcast(true).map_err(|e| e.to_string())?;
    let mut targets = vec!["255.255.255.255".to_string()];
    // The home network's own broadcast too (a /24, as home routers make it).
    if let Some(ip) = ip.and_then(|i| i.rsplit_once('.')).map(|(net, _)| format!("{net}.255")) {
        targets.push(ip);
    }
    let mut sent = false;
    for t in &targets {
        for port in [9, 7] {
            // Three times: a packet may get lost and costs nothing.
            for _ in 0..3 {
                sent |= socket.send_to(&packet, (t.as_str(), port)).is_ok();
            }
        }
    }
    sent.then_some(()).ok_or_else(|| "the packet did not go out".into())
}

// ---------- the PC ----------

fn pc_up(ip: &str) -> bool {
    [445, 3389, 22, 5985].iter().any(|p| open(ip, *p, 500))
        || Command::new("/sbin/ping").args(["-c", "1", "-t", "1", ip]).output().is_ok_and(|o| o.status.success())
}

/// Wake or ask; switching off, restarting and sleep come through `pc_power` after a yes.
pub fn pc(app: &AppHandle, action: &str) -> Value {
    let ip = get(app, "pc", "ip");
    let mac = get(app, "pc", "mac");
    match action {
        "status" => match &ip {
            Some(ip) => json!({ "on": pc_up(ip) }),
            None => json!({ "error": "The PC's IP address is not known yet (home_setup or find_devices)." }),
        },
        "wake" => {
            let Some(mac) = mac else { return json!({ "error": "The PC's MAC address is not known yet. Ask for it (Windows: ipconfig /all, Physical Address of the Ethernet adapter) or try find_devices while the PC is on." }) };
            if ip.as_deref().is_some_and(pc_up) {
                return json!({ "ok": true, "already_on": true });
            }
            if let Err(e) = wake_on_lan(&mac, ip.as_deref()) {
                return json!({ "error": e });
            }
            // Tell when it is up, if it can be told.
            if let Some(ip) = ip {
                let app = app.clone();
                std::thread::spawn(move || {
                    let began = Instant::now();
                    while began.elapsed() < Duration::from_secs(120) {
                        std::thread::sleep(Duration::from_secs(5));
                        if pc_up(&ip) {
                            crate::chat::say(&app, "Počítač naběhl.", "celebrate");
                            return;
                        }
                    }
                    crate::chat::say(&app, "Počítač se do dvou minut neozval. Je v BIOSu a u síťovky zapnuté probouzení po síti (Wake-on-LAN)?", "confused");
                });
            }
            json!({ "ok": true, "sent": "Wake-on-LAN", "note": "Booting takes a while; the buddy says when the PC answers." })
        }
        _ => json!({ "error": "action is wake or status here; shutdown, restart and sleep need the user's yes first" }),
    }
}

/// What can be asked before running `pc_power`: whether it can be done at all.
pub fn pc_power_ready(app: &AppHandle) -> Result<(), String> {
    get(app, "pc", "ssh").map(|_| ()).ok_or_else(|| {
        "Vypínat PC umím přes SSH: ve Windows zapni OpenSSH Server (Nastavení → Systém → Volitelné funkce), \
dej tam klíč z Macu a řekni mi přihlášení, třeba erik@192.168.1.23."
            .into()
    })
}

/// Shutdown, restart or sleep over SSH, a minute late so it can still be stopped (shutdown /a).
pub fn pc_power(app: &AppHandle, action: &str) -> Result<String, String> {
    pc_power_ready(app)?;
    let login = get(app, "pc", "ssh").unwrap_or_default();
    let command = match action {
        "shutdown" => "shutdown /s /t 60",
        "restart" => "shutdown /r /t 60",
        "sleep" => "rundll32.exe powrprof.dll,SetSuspendState 0,1,0",
        _ => return Err("Tohle s počítačem neumím.".into()),
    };
    let out = Command::new("/usr/bin/ssh")
        .args(["-o", "BatchMode=yes", "-o", "ConnectTimeout=6", "-o", "StrictHostKeyChecking=accept-new", &login, command])
        .output()
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(format!("Počítač se přes SSH nedal: {}", if err.is_empty() { "nevím proč" } else { &err }));
    }
    Ok(match action {
        "shutdown" => "Počítač se za minutu vypne.",
        "restart" => "Počítač se za minutu restartuje.",
        _ => "Počítač jde spát.",
    }
    .into())
}

// ---------- the TV ----------

type Socket = tungstenite::WebSocket<tungstenite::stream::MaybeTlsStream<TcpStream>>;

/// The TV's remote-control socket, paired: the first time the TV asks on screen.
fn tv_socket(app: &AppHandle, ip: &str) -> Result<Socket, String> {
    let name = base64::engine::general_purpose::STANDARD.encode("Wisp Buddy");
    let token = get(app, "tv", "token").map(|t| format!("&token={t}")).unwrap_or_default();
    let addr = SocketAddr::new(ip.parse().map_err(|_| "bad IP")?, 8002);
    // Newer TVs only take the encrypted socket; older ones only the plain one on 8001.
    let (tcp, url) = match TcpStream::connect_timeout(&addr, Duration::from_millis(1500)) {
        Ok(t) => (t, format!("wss://{ip}:8002/api/v2/channels/samsung.remote.control?name={name}{token}")),
        Err(_) => {
            let plain = SocketAddr::new(addr.ip(), 8001);
            let t = TcpStream::connect_timeout(&plain, Duration::from_millis(1500)).map_err(|_| "Televize neodpovídá. Je zapnutá a na stejné síti?".to_string())?;
            (t, format!("ws://{ip}:8001/api/v2/channels/samsung.remote.control?name={name}"))
        }
    };
    // Long enough to walk to the TV and press Allow the first time.
    tcp.set_read_timeout(Some(Duration::from_secs(30))).map_err(|e| e.to_string())?;
    // The TV signs its own certificate.
    let tls = native_tls::TlsConnector::builder()
        .danger_accept_invalid_certs(true)
        .danger_accept_invalid_hostnames(true)
        .build()
        .map_err(|e| e.to_string())?;
    let (mut ws, _) = tungstenite::client_tls_with_config(url.as_str(), tcp, None, Some(tungstenite::Connector::NativeTls(tls))).map_err(|e| format!("Televize nepustila spojení: {e}"))?;
    loop {
        match ws.read() {
            Ok(Message::Text(t)) => {
                let v: Value = serde_json::from_str(&t).unwrap_or_default();
                match v["event"].as_str() {
                    Some("ms.channel.connect") => {
                        if let Some(tok) = v["data"]["token"].as_str() {
                            set(app, "tv", "token", json!(tok));
                        }
                        return Ok(ws);
                    }
                    Some("ms.channel.unauthorized") | Some("ms.channel.timeOut") => {
                        return Err("Televize to nepovolila. Na obrazovce dej Povolit, nebo to povol v Nastavení → Obecné → Externí zařízení.".into())
                    }
                    _ => {}
                }
            }
            Ok(_) => {}
            Err(_) => return Err("Televize nepotvrdila spojení. Jestli se na ní ptá, dej Povolit a zkus to znovu.".into()),
        }
    }
}

fn send(ws: &mut Socket, v: Value) -> Result<(), String> {
    ws.send(Message::Text(v.to_string())).map_err(|e| e.to_string())
}

fn press(ws: &mut Socket, key: &str) -> Result<(), String> {
    send(ws, json!({ "method": "ms.remote.control", "params": { "Cmd": "Click", "DataOfCmd": key, "Option": "false", "TypeOfRemote": "SendRemoteKey" } }))
}

fn finish(mut ws: Socket) {
    // A moment for the TV to take it before the socket closes.
    std::thread::sleep(Duration::from_millis(400));
    let _ = ws.close(None);
}

/// Words the chat can use for keys, and the TV's names for them.
pub const KEYS: [(&str, &str); 16] = [
    ("volume_up", "KEY_VOLUP"),
    ("volume_down", "KEY_VOLDOWN"),
    ("mute", "KEY_MUTE"),
    ("home", "KEY_HOME"),
    ("back", "KEY_RETURN"),
    ("ok", "KEY_ENTER"),
    ("up", "KEY_UP"),
    ("down", "KEY_DOWN"),
    ("left", "KEY_LEFT"),
    ("right", "KEY_RIGHT"),
    ("play", "KEY_PLAY"),
    ("pause", "KEY_PAUSE"),
    ("channel_up", "KEY_CHUP"),
    ("channel_down", "KEY_CHDOWN"),
    ("source", "KEY_SOURCE"),
    ("tv", "KEY_TV"),
];

/// Apps that can be opened by name: the ids Tizen knows them by.
pub const APPS: [(&str, &str); 5] = [
    ("youtube", "111299001912"),
    ("netflix", "11101200001"),
    ("spotify", "3201606009684"),
    ("disney", "3201901017640"),
    ("prime", "3201910019365"),
];

fn tv_ip(app: &AppHandle) -> Result<String, String> {
    get(app, "tv", "ip").ok_or_else(|| "IP adresu televize ještě neznám. Zkus find_devices, když je zapnutá.".into())
}

/// on, off, status, key (with `key`), app (with `app`), volume (with `times`, + or −).
pub fn tv(app: &AppHandle, action: &str, key: Option<&str>, app_name: Option<&str>, times: Option<i64>) -> Value {
    match tv_inner(app, action, key, app_name, times) {
        Ok(v) => v,
        Err(e) => json!({ "error": e }),
    }
}

fn tv_inner(app: &AppHandle, action: &str, key: Option<&str>, app_name: Option<&str>, times: Option<i64>) -> Result<Value, String> {
    let ip = tv_ip(app)?;
    let info = samsung_info(&ip);
    // Tizen says "on" or "standby"; older TVs say nothing and only answer when on.
    let power = info.as_ref().map(|i| i["device"]["PowerState"].as_str().unwrap_or("on").to_string());
    let on = power.as_deref() == Some("on");
    match action {
        "status" => Ok(json!({ "power": power.unwrap_or_else(|| "off".into()), "name": get(app, "tv", "name") })),
        "on" => {
            if on {
                return Ok(json!({ "ok": true, "already_on": true }));
            }
            if power.as_deref() == Some("standby") {
                let mut ws = tv_socket(app, &ip)?;
                press(&mut ws, "KEY_POWER")?;
                finish(ws);
                return Ok(json!({ "ok": true }));
            }
            let mac = get(app, "tv", "mac").ok_or("MAC adresu televize neznám, takže ji neumím probudit. Zapni ji jednou ručně a řekni mi, ať ji najdu.")?;
            wake_on_lan(&mac, Some(&ip))?;
            Ok(json!({ "ok": true, "sent": "Wake-on-LAN", "note": "Some TVs only wake like this over a cable or with 'Power on with mobile' switched on." }))
        }
        "off" => {
            if !on {
                return Ok(json!({ "ok": true, "already_off": true }));
            }
            let mut ws = tv_socket(app, &ip)?;
            press(&mut ws, "KEY_POWER")?;
            finish(ws);
            Ok(json!({ "ok": true }))
        }
        "key" | "volume" => {
            if !on {
                return Err("Televize je vypnutá.".into());
            }
            let (code, n) = if action == "volume" {
                let t = times.unwrap_or(3).clamp(-30, 30);
                (if t >= 0 { "KEY_VOLUP" } else { "KEY_VOLDOWN" }, t.unsigned_abs().max(1))
            } else {
                let k = key.unwrap_or("");
                let code = KEYS.iter().find(|(name, code)| *name == k || code.eq_ignore_ascii_case(k)).map(|(_, c)| *c).ok_or("unknown key")?;
                (code, times.unwrap_or(1).clamp(1, 20) as u64)
            };
            let mut ws = tv_socket(app, &ip)?;
            for _ in 0..n {
                press(&mut ws, code)?;
                std::thread::sleep(Duration::from_millis(150));
            }
            finish(ws);
            Ok(json!({ "ok": true, "pressed": code, "times": n }))
        }
        "app" => {
            if !on {
                return Err("Televize je vypnutá, nejdřív ji zapni.".into());
            }
            let want = app_name.unwrap_or("").to_lowercase();
            let id = APPS.iter().find(|(name, _)| want.contains(name)).map(|(_, id)| *id).ok_or("unknown app")?;
            let mut ws = tv_socket(app, &ip)?;
            send(&mut ws, json!({ "method": "ms.channel.emit", "params": { "event": "ed.apps.launch", "to": "host", "data": { "appId": id, "action_type": "DEEP_LINK" } } }))?;
            finish(ws);
            Ok(json!({ "ok": true }))
        }
        _ => Err("action is on, off, status, key, volume or app".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_mac_addresses_however_they_are_written() {
        assert_eq!(norm_mac("1C-69-7A-0A-B-CC").as_deref(), Some("1c:69:7a:0a:0b:cc"));
        assert_eq!(norm_mac("1c:69:7a:a:b:cc").as_deref(), Some("1c:69:7a:0a:0b:cc"));
        assert_eq!(norm_mac("1c:69:7a:0a:0b"), None);
        assert_eq!(norm_mac("zz:69:7a:0a:0b:cc"), None);
    }

    #[test]
    fn a_magic_packet_is_six_ffs_and_the_mac_sixteen_times() {
        let p = magic_packet("01:02:03:04:05:06").unwrap();
        assert_eq!(p.len(), 102);
        assert_eq!(&p[..6], &[0xff; 6]);
        assert_eq!(&p[6..12], &[1, 2, 3, 4, 5, 6]);
        assert_eq!(&p[96..], &[1, 2, 3, 4, 5, 6]);
    }

    #[test]
    fn lists_devices_from_arp_without_broadcast() {
        let arp = "? (192.168.1.1) at 0:11:22:33:44:55 on en0 ifscope [ethernet]\n\
desktop-pc.home (192.168.1.23) at 1c:69:7a:a:b:cc on en0 ifscope [ethernet]\n\
? (192.168.1.40) at (incomplete) on en0 ifscope [ethernet]\n\
? (192.168.1.255) at ff:ff:ff:ff:ff:ff on en0 ifscope [ethernet]\n\
? (224.0.0.251) at 1:0:5e:0:0:fb on en0 ifscope permanent [ethernet]\n";
        let d = parse_arp(arp);
        assert_eq!(d.len(), 2);
        assert_eq!(d[0], Device { ip: "192.168.1.1".into(), mac: "00:11:22:33:44:55".into(), name: None });
        assert_eq!(d[1].name.as_deref(), Some("desktop-pc.home"));
        assert_eq!(d[1].mac, "1c:69:7a:0a:0b:cc");
    }
}

#[cfg(test)]
mod live {
    /// `cargo test --lib live_network -- --ignored --nocapture`: what is on the home network now.
    #[test]
    #[ignore]
    fn live_network() {
        let out = std::process::Command::new("/usr/sbin/arp").arg("-a").output().unwrap();
        for d in super::parse_arp(&String::from_utf8_lossy(&out.stdout)) {
            let tv = super::samsung_info(&d.ip).map(|i| i["device"]["name"].to_string());
            println!("{:16} {} {:?} tv={:?}", d.ip, d.mac, d.name, tv);
        }
    }
}
