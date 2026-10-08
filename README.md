# Wisp Buddy

A little character from [Wisp](https://github.com/ErikKarasek/wisp)'s family that lives on your Mac's desktop. It walks along the bottom of the screen, keeps close to your mouse, sits down now and then, jumps up onto your windows (point at a window's top edge and it hops up there) and rides along when you drag them, and sleeps at night. Tray → Velikost makes it small, medium or large. Pick it up with the mouse and throw it, or poke it. Its look is yours: the studio (tray → Postavička…) has twenty shapes, colours, eyes and build, and offers the characters you already made in Wisp. Double-click it to chat: it answers through Gemini with your own free key from Google AI Studio, kept in the Keychain. Ask it in the chat to remind you of something ("připomeň mi v 15:00…", "every weekday at 9…"): when the time comes it wakes up and shows it in the bubble, with Done and snooze buttons. And past bedtime (tray → Poslat mě spát) it tells you to go to sleep.

Made with Tauri 2 (Rust + TypeScript). The body (walking, falling, being carried) is a small physics loop in Rust that moves a transparent window; the character is Wisp's mascot engine, drawn in SVG.

## As an assistant

It is not only company. In the chat (or through the 🎙 button in the bubble: click, speak, click again) it:

- **remembers** what you tell it about yourself and your plans, and starts every conversation knowing it (🧠 in the bubble lists it, ✕ forgets);
- **talks out loud** with the Mac's Czech voice (tray → Mluví nahlas);
- gives a **morning overview** the first time you sit down after six: today's reminders, what the night shift finished, what Wisp's agents need (tray → Ranní přehled, or Co mě dneska čeká? at any time);
- **does work**: "v job-mailu oprav to a to" goes to [Wisp](https://github.com/ErikKarasek/wisp)'s night shift, which runs Claude Code in its own branch and opens a draft PR. Nothing starts before you press Ano in the bubble; when it is done, the buddy says so with the PR link;
- **wakes the Windows PC** (Wake-on-LAN; the PC on a cable, WoL on in the BIOS and the network card) and, after an Ano, shuts it down, restarts it or puts it to sleep over SSH (Windows' OpenSSH Server with the Mac's key; tell the buddy the login, e.g. `erik@10.0.1.23`);
- **works the Samsung TV** on the home network: on, off, volume, keys, YouTube / Netflix / Spotify / Disney+ / Prime Video. The first time the TV asks on screen whether to allow Wisp Buddy.

Ask it to "find the devices at home" and it looks at what the Mac sees on the network; a Samsung TV is saved by itself. The PC's MAC address is in Windows under `ipconfig /all` (Physical Address of the Ethernet adapter). `cargo test --lib live_network -- --ignored --nocapture` prints what is on the network now.

## Build

```sh
corepack pnpm install
corepack pnpm tauri:build
```

The app lands in `src-tauri/target/release/bundle/macos/`. It is not signed or notarized, so the first launch has to be allowed in System Settings → Privacy & Security. To sign it with your own certificate (then the Keychain stops asking after every rebuild), put the identity in `src-tauri/tauri.local.conf.json`, which git ignores:

```json
{ "bundle": { "macOS": { "signingIdentity": "Apple Development: you@example.com (XXXXXXXXXX)" } } }
```

## With Wisp

When [Wisp](https://github.com/ErikKarasek/wisp) runs on the same Mac, the buddy asks it every few seconds (GET /buddy/state on Wisp's local server, with the key Wisp keeps for its hooks). It looks keen while an agent works, does a somersault when one finishes, and says in its bubble when one fails or waits for you. Ask it in the chat what the agents are doing. Tray → Propojit s Wispem turns it off.
