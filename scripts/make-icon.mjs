// Renders the app icon: the buddy on a dark rounded square (macOS doesn't add one for us),
// via headless Chrome, into src-tauri/icons/source.png. Then `tauri icon` makes every size.
import { execFileSync } from 'node:child_process'
import { writeFileSync, mkdtempSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'

const svg = execFileSync('npx', ['--yes', 'tsx', '-e', `
  import { mascotSvg } from './src/mascot/svg.ts';
  process.stdout.write(mascotSvg({ shape: 'round', color: '#6d7fe0' }, 'happy', 640));
`], { encoding: 'utf8' })
const html = `<html><body style="margin:0;background:transparent">
<div style="width:1024px;height:1024px;display:grid;place-items:center">
<div style="width:824px;height:824px;border-radius:185px;background:linear-gradient(160deg,#2a2d38,#15161b);display:grid;place-items:center;box-shadow:inset 0 0 0 2px #ffffff14">${svg}</div></div></body></html>`
const dir = mkdtempSync(join(tmpdir(), 'buddy-icon-'))
writeFileSync(join(dir, 'icon.html'), html)
execFileSync('/Applications/Google Chrome.app/Contents/MacOS/Google Chrome', [
  '--headless=new', '--disable-gpu', '--hide-scrollbars', '--default-background-color=00000000',
  '--window-size=1024,1024', `--screenshot=${join(process.cwd(), 'src-tauri/icons/source.png')}`, `file://${join(dir, 'icon.html')}`,
], { stdio: 'ignore' })
console.log('src-tauri/icons/source.png')
