# Wisp Buddy

A little character from [Wisp](https://github.com/ErikKarasek/wisp)'s family that lives on your Mac's desktop. It walks along the bottom of the screen, keeps close to your mouse, sits down now and then, jumps up onto your windows (point at a window's top edge and it hops up there) and rides along when you drag them, and sleeps at night. Tray → Velikost makes it small, medium or large. Pick it up with the mouse and throw it, or poke it. Its look is yours: the studio (tray → Postavička…) has twenty shapes, colours, eyes and build, and offers the characters you already made in Wisp. Double-click it to chat: it answers through Gemini with your own free key from Google AI Studio, kept in the Keychain. Ask it in the chat to remind you of something ("připomeň mi v 15:00…", "every weekday at 9…"): when the time comes it wakes up and shows it in the bubble, with Done and snooze buttons. And past bedtime (tray → Poslat mě spát) it tells you to go to sleep.

Made with Tauri 2 (Rust + TypeScript). The body (walking, falling, being carried) is a small physics loop in Rust that moves a transparent window; the character is Wisp's mascot engine, drawn in SVG.

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
