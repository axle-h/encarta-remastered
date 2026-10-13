// Renders one MapLibre view headlessly and saves a PNG once the map is idle.
// Usage: node shoot.mjs "http://localhost:8099/?style=atlas-default&lon=10&lat=50&z=4.5" out.png [timeout ms]
// Needs chromium-browser and server.js running on port 8099.
import { spawn } from 'node:child_process';
import fs from 'node:fs';
const [,, url, out, waitMs = '120000'] = process.argv;
const chrome = spawn('chromium-browser', ['--headless=new', '--use-angle=swiftshader', '--enable-unsafe-swiftshader', '--no-sandbox',
  '--window-size=1200,800', '--remote-debugging-port=9333', '--user-data-dir=' + (process.env.TMPDIR || '/tmp') + '/atlas-maplibre-profile', 'about:blank'], { stdio: 'ignore' });
const sleep = ms => new Promise(r => setTimeout(r, ms));
let ws;
for (let i = 0; i < 50; i++) { try { const v = await (await fetch('http://127.0.0.1:9333/json/list')).json(); const p = v.find(t => t.type === 'page'); if (p) { ws = new WebSocket(p.webSocketDebuggerUrl); break; } } catch {} await sleep(200); }
await new Promise(r => ws.onopen = r);
let id = 0; const pending = new Map();
ws.onmessage = m => { const d = JSON.parse(m.data); if (d.id && pending.has(d.id)) { pending.get(d.id)(d.result); pending.delete(d.id); } };
const call = (method, params = {}) => new Promise(r => { const i = ++id; pending.set(i, r); ws.send(JSON.stringify({ id: i, method, params })); });
await call('Page.enable'); await call('Page.navigate', { url });
const t0 = Date.now(); let title = '';
while (Date.now() - t0 < +waitMs) { await sleep(1000); const r = await call('Runtime.evaluate', { expression: 'document.title' }); title = r.result.value; if (title.startsWith('idle')) break; }
await sleep(500);
const r2 = await call('Runtime.evaluate', { expression: "JSON.stringify((window.errors||[]).slice(0,8))" });
const shot = await call('Page.captureScreenshot', { format: 'png' });
fs.writeFileSync(out, Buffer.from(shot.data, 'base64'));
console.log(title.slice(0, 300), '|', r2.result.value.slice(0, 1500), '|', ((Date.now() - t0) / 1000).toFixed(1) + 's');
chrome.kill(); process.exit(0);
