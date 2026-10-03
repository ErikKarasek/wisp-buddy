// `tauri build`, plus src-tauri/tauri.local.conf.json when it is there (not in git): your own
// signing identity, say. A signed build keeps its Keychain access across rebuilds.
import { existsSync } from "node:fs";
import { spawnSync } from "node:child_process";

const local = "src-tauri/tauri.local.conf.json";
const args = ["tauri", "build", ...(existsSync(local) ? ["--config", local] : []), ...process.argv.slice(2)];
process.exit(spawnSync("pnpm", args, { stdio: "inherit" }).status ?? 1);
