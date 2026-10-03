# Wisp Buddy

A little character from [Wisp](https://github.com/ErikKarasek/wisp)'s family that lives on your Mac's desktop. It walks along the bottom of the screen, sits down now and then, and sleeps at night. Pick it up with the mouse and throw it, or poke it.

Made with Tauri 2 (Rust + TypeScript). The body (walking, falling, being carried) is a small physics loop in Rust that moves a transparent window; the character is Wisp's mascot engine, drawn in SVG.

## Build

```sh
corepack pnpm install
corepack pnpm tauri:build
```

The app lands in `src-tauri/target/release/bundle/macos/`. It is not signed or notarized, so the first launch has to be allowed in System Settings → Privacy & Security.

## Where it is going

- A studio to put your own character together (twenty shapes, colours, eyes)
- Chatting with it through Gemini (your own free key)
- Reminders, a Pomodoro timer and a nudge to go to bed
- Sitting on top of the window you work in
- Showing your agents' state when Wisp runs on the same Mac
