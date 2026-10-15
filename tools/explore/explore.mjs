#!/usr/bin/env node
// Encarta app explorer: scripted journeys plus seeded random "monkey" interaction in headless
// Chromium, collecting console/page errors, failed requests, broken images, error views and
// blank/stuck views. Writes out/explore/<timestamp>/{report.json,report.md,shots/}.
// Usage: node explore.mjs [--base URL] [--minutes 10] [--area atlas,games] [--seed N] [--list]
//   [--build] (fresh vite build of web/ first) [--static DIR] [--out DIR] [--viewport 1280x800] [--no-monkey] [--headed]
import { chromium } from "playwright-core";
import { spawn } from "node:child_process";
import { existsSync, mkdirSync, readdirSync, writeFileSync } from "node:fs";
import { homedir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const argv = process.argv.slice(2);
const opt = (n, d) => {
  const i = argv.indexOf("--" + n);
  return i < 0 ? d : argv[i + 1]?.startsWith("--") || argv[i + 1] === undefined ? true : argv[i + 1];
};
const AREAS = ["home", "search", "articles", "dictionary", "atlas", "timeline", "tours", "panoramas", "games", "kids", "options"];
if (opt("list")) { console.log(AREAS.join("\n")); process.exit(0); }
const minutes = Number(opt("minutes", 10));
const seed = Number(opt("seed", Date.now() % 1e9));
const areas = String(opt("area", AREAS.join(","))).split(",").filter(Boolean);
for (const a of areas) if (!AREAS.includes(a)) { console.error(`unknown area ${a}; one of ${AREAS.join(", ")}`); process.exit(2); }
const [VW, VH] = String(opt("viewport", "1280x800")).split("x").map(Number);
const stamp = new Date().toISOString().replace(/[:.]/g, "-").slice(0, 19);
const outDir = resolve(String(opt("out", join(ROOT, "out/explore", stamp))));
mkdirSync(join(outDir, "shots"), { recursive: true });

function rng(s) { // mulberry32
  return () => { s |= 0; s = (s + 0x6d2b79f5) | 0; let t = Math.imul(s ^ (s >>> 15), 1 | s); t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t; return ((t ^ (t >>> 14)) >>> 0) / 4294967296; };
}
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

// ---- server ----
let serverProc = null;
let base = opt("base", null);
async function ping(u) { try { return (await fetch(u + "/healthz")).ok; } catch { return false; } }
async function ensureServer() {
  if (base) { if (!(await ping(base))) throw new Error(`no server at ${base}`); return; }
  if (await ping("http://127.0.0.1:8080")) { base = "http://127.0.0.1:8080"; return; }
  const bin = ["target/release/encarta-server", "target/debug/encarta-server"].map((p) => join(ROOT, p)).find(existsSync);
  if (!bin) throw new Error("no running server and no built target/release/encarta-server (cargo build --release -p encarta-server)");
  let staticDir = opt("static", join(ROOT, "web/dist"));
  if (opt("build")) { // fresh build of the current source (with sourcemaps) into target/explore-dist, leaving web/dist alone
    staticDir = join(ROOT, "target/explore-dist");
    console.log("building web client ->", staticDir);
    await new Promise((res, rej) => spawn("npx", ["vite", "build", "--outDir", staticDir, "--emptyOutDir", "--sourcemap"], { cwd: join(ROOT, "web"), stdio: "inherit" }).on("exit", (c) => (c ? rej(new Error("vite build failed")) : res())));
  }
  const port = 8100 + Math.floor(Math.random() * 800);
  serverProc = spawn(bin, ["--out", join(ROOT, "out"), "--static", staticDir, "--listen", `127.0.0.1:${port}`], { stdio: "ignore" });
  base = `http://127.0.0.1:${port}`;
  for (let i = 0; i < 50 && !(await ping(base)); i++) await sleep(200);
  if (!(await ping(base))) throw new Error("server did not start");
}

// ---- sample content ids ----
const ids = { article: [], sidebar: [], media: [], map: [], timeline: [] };
async function loadIds() {
  for (const q of ["zebra", "history", "river", "music", "war", "city", "bird", "planet", "language", "art"]) {
    try {
      const j = await (await fetch(`${base}/api/search?q=${q}&limit=40`)).json();
      for (const h of j.hits ?? []) { const l = ids[h.kind]; if (l && !l.includes(h.ref_id) && l.length < 60) l.push(h.ref_id); }
    } catch { /* ignore */ }
  }
}

// ---- issue collection ----
const issues = new Map();
let shotN = 0;
const RE_NOISE = [
  /GPU stall due to ReadPixels/i, /GL Driver Message/i, /Automatic fallback to software WebGL/i,
  /Failed to load resource.*(favicon)/i, /ERR_ABORTED/, /\[Violation\]/, /AudioContext was not allowed/i,
  /Download the React DevTools/i, /performance warning: READ-usage buffer/i,
];
function norm(s) {
  return String(s).replace(/https?:\/\/[^/\s]+/g, "").replace(/0x[0-9a-f]+/gi, "0xN").replace(/\d+(\.\d+)?/g, "N").replace(/\s+/g, " ").slice(0, 220);
}
let ctx; // current area context: {area, page, steps, url}
async function report(type, msg, extra = {}) {
  if (RE_NOISE.some((r) => r.test(msg))) return;
  const key = `${type}|${norm(msg)}`;
  const ex = issues.get(key);
  if (ex) { ex.count++; return; }
  const issue = { id: issues.size + 1, type, message: String(msg).slice(0, 600), area: ctx.area, url: ctx.page.url().replace(base, ""),
    count: 1, seed, steps: [...ctx.steps], screenshot: null, ...extra };
  issues.set(key, issue);
  try {
    const f = `shots/${String(++shotN).padStart(3, "0")}-${ctx.area}-${type}.png`;
    await ctx.page.screenshot({ path: join(outDir, f), timeout: 4000 });
    issue.screenshot = f;
  } catch { /* page busy */ }
  console.log(`  [issue ${issue.id}] ${type}: ${issue.message.slice(0, 110)}`);
}
function step(s) { ctx.steps.push(s); if (ctx.steps.length > 400) ctx.steps.splice(1, 100); }

function attach(page) {
  page.on("console", (m) => {
    const t = m.type();
    if (t === "error" || t === "warning") {
      const loc = m.location();
      report(t === "error" ? "console-error" : "console-warning", m.text(), { source: loc?.url ? `${loc.url.replace(base, "")}:${loc.lineNumber}` : undefined });
    }
  });
  page.on("pageerror", (e) => report("page-error", e.stack?.split("\n").slice(0, 4).join(" | ") ?? String(e)));
  page.on("requestfailed", (r) => { const f = r.failure()?.errorText ?? ""; if (!/ABORTED/.test(f)) report("request-failed", `${r.method()} ${r.url().replace(base, "")} ${f}`); });
  page.on("response", (r) => { if (r.status() >= 400 && !/favicon/.test(r.url())) report("http-" + (r.status() >= 500 ? "5xx" : "4xx"), `${r.status()} ${r.request().method()} ${r.url().replace(base, "")}`); });
  page.on("dialog", (d) => d.dismiss().catch(() => {}));
  page.on("popup", (p) => p.close().catch(() => {}));
  page.on("crash", () => report("page-crash", "renderer crashed"));
}

// ---- health checks run in page ----
const HEALTH = () => {
  const out = [];
  const vis = (el) => { const r = el.getBoundingClientRect(); const s = getComputedStyle(el); return r.width > 2 && r.height > 2 && s.visibility !== "hidden" && s.display !== "none" && r.bottom > 0 && r.top < innerHeight; };
  for (const im of document.images) {
    if (!vis(im) || !im.src || im.loading === "lazy" && !im.complete) continue;
    if (im.complete && im.naturalWidth === 0) out.push(["broken-image", (im.currentSrc || im.src).replace(location.origin, "")]);
    else if (/placeholder|missing|noimage|no-image/i.test(im.currentSrc)) out.push(["placeholder-image", im.currentSrc.replace(location.origin, "")]);
  }
  for (const el of document.querySelectorAll("[class*=placeholder i],[class*=missing i],[class*=nomedia i],[class*=broken i]")) {
    // `data-expected-placeholder`: the app's own note for content the disc does not have.
    if (el.tagName === "INPUT" || !vis(el) || el.closest("[data-expected-placeholder]")) continue;
    out.push(["placeholder-element", `${el.tagName.toLowerCase()}.${String(el.className).slice(0, 60)} "${(el.textContent || "").trim().slice(0, 40)}"`]);
  }
  for (const el of document.querySelectorAll("[role=alert],.enc-page-error,.enc-error,vite-error-overlay,#webpack-dev-server-client-overlay")) {
    if (el.tagName !== "VITE-ERROR-OVERLAY" && !vis(el)) continue;
    out.push(["error-view", (el.textContent || el.tagName).trim().replace(/\s+/g, " ").slice(0, 160)]);
  }
  const text = (document.body.innerText || "").trim();
  if (/Maximum update depth|Application error|Unhandled Runtime Error/i.test(text)) out.push(["error-view", text.slice(0, 160)]);
  const spin = [...document.querySelectorAll(".enc-spinner,.enc-loading")].filter(vis).length;
  const media = document.querySelectorAll("canvas,img,svg,video").length;
  out.push(["_state", { spin, textLen: text.length, media }]);
  return out;
};
let stuckSince = 0;
async function health(label = "") {
  let res;
  try { res = await ctx.page.evaluate(HEALTH); } catch { return; }
  let st = null;
  for (const [t, d] of res) {
    if (t === "_state") st = d;
    else await report(t, d, label ? { after: label } : {});
  }
  if (st) {
    if (st.spin) { stuckSince ||= Date.now(); if (Date.now() - stuckSince > 12000) { await report("stuck-spinner", `loading spinner visible >12s at ${ctx.page.url().replace(base, "")}`); stuckSince = Date.now(); } } else stuckSince = 0;
    if (st.textLen < 15 && st.media < 3 && !st.spin) await report("blank-view", `almost no content at ${new URL(ctx.page.url()).pathname}`);
  }
}

// ---- actions ----
async function goto(path) {
  step(`goto ${path}`);
  try { await ctx.page.goto(base + path, { waitUntil: "domcontentloaded", timeout: 20000 }); } catch (e) { await report("navigation-failed", `${path}: ${e.message.split("\n")[0]}`); return; }
  await settle(500);
  await health("goto " + path);
}
const settle = (ms = 250) => ctx.page.waitForTimeout(ms);

const CANDIDATES = () => {
  const sel = 'a[href],button,[role=button],[role=tab],[role=menuitem],[role=option],[role=link],summary,input,select,textarea,label,[tabindex]:not([tabindex="-1"]),canvas,img,[onclick]';
  const seen = new Set(); const out = [];
  const els = [...document.querySelectorAll(sel)];
  for (const el of document.querySelectorAll("div,span,li,td,tr,p")) { if (els.length > 1500) break; if (getComputedStyle(el).cursor === "pointer") els.push(el); }
  for (const el of els) {
    if (el.disabled || seen.has(el)) continue; seen.add(el);
    const r = el.getBoundingClientRect();
    if (r.width < 3 || r.height < 3 || r.bottom < 0 || r.top > innerHeight || r.right < 0 || r.left > innerWidth) continue;
    const x = Math.min(innerWidth - 2, Math.max(2, r.left + r.width * (0.2 + Math.random() * 0.6)));
    const y = Math.min(innerHeight - 2, Math.max(2, r.top + r.height * (0.2 + Math.random() * 0.6)));
    const top = document.elementFromPoint(x, y);
    if (!top || !(top === el || el.contains(top) || top.contains(el))) continue;
    if (el.tagName === "A") {
      const h = el.getAttribute("href") || "";
      if (el.target === "_blank" || el.hasAttribute("download") || /^(mailto|tel|javascript):/.test(h)) continue;
      if (/^https?:/.test(h) && !h.startsWith(location.origin)) continue;
    }
    const label = (el.getAttribute("aria-label") || el.title || el.textContent || el.alt || el.placeholder || "").trim().replace(/\s+/g, " ").slice(0, 32);
    out.push({ x: Math.round(x), y: Math.round(y), tag: el.tagName.toLowerCase(), label, type: el.type || "", canvas: el.tagName === "CANVAS" });
  }
  return out;
};

const WORDS = ["zebra", "paris", "ab", "x", "war", "river", "música", "a b", "", "  ", "<script>", "1789", "Mount", "ñandú", "the", "çatal", "ZZZZ", "'"];
const KEYS = ["Escape", "Enter", "Tab", "ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown", "PageDown", "PageUp", "Home", "End", "Space", "+", "-", "w", "a", "s", "d", "Control+p", "Control+o", "Control+s", "Alt+ArrowLeft", "Control+m", "Control+t", "Control+d"];

async function act(r, area) {
  const p = ctx.page;
  const c = await p.evaluate(CANDIDATES).catch(() => []);
  const roll = r();
  const pick = () => c[Math.floor(r() * c.length)];
  const canv = c.filter((e) => e.canvas);
  try {
    if (roll < 0.1) { const k = KEYS[Math.floor(r() * KEYS.length)]; step(`key ${k}`); await p.keyboard.press(k); }
    else if (roll < 0.17) { const dy = Math.round((r() - 0.4) * 1200); step(`scroll ${dy}`); await p.mouse.wheel(0, dy); }
    else if (roll < 0.24 && c.some((e) => /^(text|search|)$/.test(e.type) && /input|textarea/.test(e.tag))) {
      const t = c.filter((e) => /input|textarea/.test(e.tag) && !/checkbox|radio|range|button|file/.test(e.type)); const e = t[Math.floor(r() * t.length)];
      const w = WORDS[Math.floor(r() * WORDS.length)];
      step(`type ${JSON.stringify(w)} into ${e.tag}[${e.label}] @${e.x},${e.y}`);
      await p.mouse.click(e.x, e.y); await p.keyboard.press("Control+a"); await p.keyboard.type(w, { delay: 5 }); if (r() < 0.7) await p.keyboard.press("Enter");
    } else if ((canv.length && roll < 0.55) || (area === "atlas" && roll < 0.8) || (["tours", "panoramas"].includes(area) && canv.length && roll < 0.7)) {
      const e = canv.length ? canv[Math.floor(r() * canv.length)] : pick();
      if (!e) return;
      const sub = r();
      const x = e.x, y = e.y;
      if (sub < 0.35) { const dy = r() < 0.7 ? -(100 + Math.round(r() * 500)) : 100 + Math.round(r() * 500); step(`wheel ${dy} @${x},${y}`); await p.mouse.move(x, y); for (let i = 0; i < 1 + Math.floor(r() * 5); i++) { await p.mouse.wheel(0, dy); await p.waitForTimeout(40); } }
      else if (sub < 0.65) { const dx = Math.round((r() - 0.5) * 500), dy = Math.round((r() - 0.5) * 400); step(`drag ${x},${y} by ${dx},${dy}`); await p.mouse.move(x, y); await p.mouse.down(); await p.mouse.move(x + dx / 2, y + dy / 2, { steps: 4 }); await p.mouse.move(x + dx, y + dy, { steps: 4 }); await p.mouse.up(); }
      else if (sub < 0.85) { step(`click ${x},${y} <canvas>`); await p.mouse.click(x, y); }
      else if (sub < 0.92) { step(`dblclick ${x},${y} <canvas>`); await p.mouse.dblclick(x, y); }
      else { step(`rightclick ${x},${y} <canvas>`); await p.mouse.click(x, y, { button: "right" }); }
    } else if (c.length) {
      const e = pick();
      step(`click ${e.x},${e.y} <${e.tag}> "${e.label}"`);
      await p.mouse.click(e.x, e.y);
    }
  } catch (e) { step(`(action error: ${e.message.split("\n")[0]})`); }
  await settle(area === "atlas" || area === "tours" ? 250 : 120);
}

// ---- areas ----
const pick = (r, a) => a[Math.floor(r() * a.length)];
const AREA = {
  home: { start: () => ["/"], inArea: (p) => p === "/" || /^\/(browse|help|favorites|media)/.test(p) },
  search: { start: (r) => ["/search?q=zebra", "/search?q=" + pick(r, WORDS.filter(Boolean))], inArea: (p) => p.startsWith("/search") },
  articles: {
    start: (r) => [`/item/${pick(r, ids.article)}`, `/item/${pick(r, ids.article)}/media`, `/item/${pick(r, ids.sidebar.length ? ids.sidebar : ids.article)}`, `/item/${pick(r, ids.media.length ? ids.media : ids.article)}`, "/favorites"],
    inArea: (p) => /^\/(item|favorites|media|browse)/.test(p),
  },
  dictionary: { start: () => ["/dictionary", "/thesaurus", "/translate"], inArea: (p) => /^\/(dictionary|thesaurus|translate)/.test(p) },
  atlas: { start: () => ["/atlas", "/atlas?lat=48.85&lon=2.35&alt=500&view=flat", "/atlas?lat=0&lon=0&alt=9000"], inArea: (p) => p.startsWith("/atlas") },
  timeline: { start: () => ["/timeline"], inArea: (p) => p.startsWith("/timeline") },
  tours: { start: () => ["/tours", "/tours/04257"], inArea: (p) => p.startsWith("/tours") },
  panoramas: { start: () => ["/tours"], inArea: (p) => /^\/(tours|item)/.test(p) },
  games: { start: () => ["/games", "/games/geoquiz", "/games/quizzes", "/games/mindmaze", "/games/city-climate", "/games/population", "/games/match"], inArea: (p) => p.startsWith("/games") },
  kids: { start: () => ["/kids"], inArea: (p) => p.startsWith("/kids") },
  options: { start: () => ["/"], inArea: () => true },
};

async function clickText(re) {
  const ok = await ctx.page.evaluate((src) => {
    const re = new RegExp(src, "i");
    const el = [...document.querySelectorAll("button,a,[role=menuitem],[role=tab],summary,label")].find((e) => re.test((e.getAttribute("aria-label") || e.textContent || "").trim()) && e.getBoundingClientRect().width > 0);
    if (!el) return false; el.click(); return true;
  }, re.source);
  step(`clickText /${re.source}/ -> ${ok}`);
  await settle(300);
  return ok;
}

async function journeys(area, r) {
  const p = ctx.page;
  const A = AREA[area];
  if (area === "atlas") {
    await goto("/atlas"); await settle(1500);
    for (let i = 0; i < 8; i++) { step(`wheel -300 @640,400`); await p.mouse.move(640, 400); await p.mouse.wheel(0, -300); await settle(300); }
    await health("zoom in");
    step("click 640,400"); await p.mouse.click(640, 400); await settle(500); await health("click map");
    for (let i = 0; i < 6; i++) { const x = 300 + Math.round(r() * 700), y = 200 + Math.round(r() * 400); step(`click ${x},${y}`); await p.mouse.click(x, y); await settle(350); }
    await health("clicks");
    step("wheel 400 @640,400 x8"); for (let i = 0; i < 8; i++) { await p.mouse.wheel(0, 400); await settle(120); }
    await health("zoom out");
  } else if (area === "tours" || area === "panoramas") {
    await goto("/tours");
    const hrefs = await p.evaluate(() => [...new Set([...document.querySelectorAll("a[href]")].map((a) => a.getAttribute("href")).filter((h) => h && /^\/(tours|item)\//.test(h)))]);
    const n = area === "tours" ? 3 : 20;
    const list = hrefs.sort(() => r() - 0.5).slice(0, n);
    for (const h of list) {
      await goto(h); await settle(2500);
      for (const k of ["w", "w", "a", "ArrowLeft", "d", "s"]) { step(`key ${k}`); await p.keyboard.down(k); await settle(250); await p.keyboard.up(k); }
      step("drag 640,400 by 200,60"); await p.mouse.move(640, 400); await p.mouse.down(); await p.mouse.move(840, 460, { steps: 6 }); await p.mouse.up(); await settle(300);
      await health(h);
    }
  } else if (area === "options") {
    await goto("/");
    step("key Control+o"); await p.keyboard.press("Control+o"); await settle(400); await health("options dialog");
    for (let i = 0; i < 10; i++) await act(r, area);
    step("key Escape"); await p.keyboard.press("Escape");
    await goto(`/item/${pick(r, ids.article)}`);
    step("key Control+p"); await p.keyboard.press("Control+p"); await settle(400); await health("print dialog");
    await p.keyboard.press("Escape");
    for (const re of [/options/i, /print/i, /copy/i, /favorites?/i, /font|size/i]) { await clickText(re); await health(String(re)); await p.keyboard.press("Escape"); }
  } else if (area === "kids") {
    await goto("/kids");
    await goto(`/kids/item/${pick(r, ids.article)}`);
    await goto("/kids/search?q=zebra");
    await goto("/kids/dictionary");
  } else if (area === "articles") {
    await goto(`/item/761575094`);
    for (const re of [/media|gallery|pictures/i, /copy/i, /print/i, /sidebar|related/i, /favorite/i, /contents|outline/i]) { await clickText(re); await health(String(re)); await p.keyboard.press("Escape"); }
    await goto(`/item/761575094/media`);
  } else if (area === "search") {
    await goto("/search?q=zebra"); await goto("/search?q=a"); await goto("/search?q=");
  } else if (area === "dictionary") {
    for (const u of A.start(r)) await goto(u);
  } else if (area === "games") {
    for (const u of A.start(r)) { await goto(u); await settle(800); }
  } else if (area === "timeline") {
    await goto("/timeline");
    for (const dx of [300, -600, 900, -900]) { step(`drag 640,400 by ${dx},0`); await p.mouse.move(640, 400); await p.mouse.down(); await p.mouse.move(640 + dx, 400, { steps: 8 }); await p.mouse.up(); await settle(300); await health("timeline drag"); }
    for (const dy of [-400, -400, 600]) { step(`wheel ${dy}`); await p.mouse.wheel(0, dy); await settle(300); }
  } else {
    for (const u of A.start(r)) await goto(u);
  }
  await health("journey end");
}

async function runArea(browser, area, budgetMs, n) {
  const r = rng(seed * 31 + n * 7919);
  const context = await browser.newContext({ viewport: { width: VW, height: VH }, acceptDownloads: false });
  await context.addInitScript(() => { window.print = () => {}; window.open = () => null; });
  const page = await context.newPage();
  ctx = { area, page, steps: [] };
  attach(page);
  const t0 = Date.now(); let actions = 0;
  console.log(`== ${area} (${Math.round(budgetMs / 1000)}s)`);
  try {
    await journeys(area, r);
    if (!opt("no-monkey")) {
      const A = AREA[area];
      await goto(pick(r, A.start(r)));
      while (Date.now() - t0 < budgetMs) {
        const path = new URL(page.url()).pathname + new URL(page.url()).search;
        if (!A.inArea(new URL(page.url()).pathname) || page.url() === "about:blank") await goto(pick(r, A.start(r)));
        else {
          await act(r, area); actions++;
          if (actions % 3 === 0) await health();
          // a blocking modal/overlay can trap the monkey; Escape now and then
          if (actions % 10 === 0) { step("key Escape"); await page.keyboard.press("Escape").catch(() => {}); }
          if (actions % 40 === 0) await goto(pick(r, A.start(r)));
        }
      }
    }
  } catch (e) { await report("explorer-error", `${area}: ${e.message.split("\n")[0]}`); }
  await context.close().catch(() => {});
  return { area, actions, seconds: Math.round((Date.now() - t0) / 1000) };
}

function write(summary) {
  const list = [...issues.values()];
  writeFileSync(join(outDir, "report.json"), JSON.stringify({ base, seed, minutes, areas: summary, issues: list }, null, 2));
  const md = [`# Encarta explorer report`, `seed ${seed}, ${minutes} min, areas: ${areas.join(", ")}  `, `rerun: \`node tools/explore/explore.mjs --seed ${seed} --area ${areas.join(",")} --minutes ${minutes}\``, "",
    `${list.length} unique issues`, "", ...summary.map((s) => `- ${s.area}: ${s.actions} monkey actions, ${s.seconds}s`), ""];
  const byType = {};
  for (const i of list) (byType[i.type] ||= []).push(i);
  for (const [t, l] of Object.entries(byType)) {
    md.push(`## ${t} (${l.length})`, "");
    for (const i of l) {
      md.push(`### #${i.id} [${i.area}] ${i.message.slice(0, 160)}`, `- count ${i.count}, page \`${i.url}\`${i.source ? `, source \`${i.source}\`` : ""}${i.after ? `, after ${i.after}` : ""}`, i.screenshot ? `- screenshot: ${i.screenshot}` : "", "- steps (last 25):", "```", ...i.steps.slice(-25), "```", "");
    }
  }
  writeFileSync(join(outDir, "report.md"), md.join("\n"));
}

function findChrome() {
  const dir = join(homedir(), ".cache/ms-playwright");
  const sorted = (p) => readdirSync(dir).filter((d) => d.startsWith(p)).sort((a, b) => Number(b.split("-")[1]) - Number(a.split("-")[1]));
  for (const d of sorted("chromium_headless_shell-")) { const f = join(dir, d, "chrome-headless-shell-linux64/chrome-headless-shell"); if (existsSync(f)) return f; }
  for (const d of sorted("chromium-")) { const f = join(dir, d, "chrome-linux64/chrome"); if (existsSync(f)) return f; }
  return undefined;
}

await ensureServer();
await loadIds();
console.log(`base ${base} seed ${seed} areas ${areas.join(",")} -> ${outDir}`);
const browser = await chromium.launch({ headless: true, executablePath: opt("chrome", findChrome()),
  args: ["--use-angle=swiftshader", "--enable-unsafe-swiftshader", "--ignore-gpu-blocklist", "--enable-webgl", "--autoplay-policy=no-user-gesture-required", "--mute-audio"] });
const summary = [];
const per = (minutes * 60000) / areas.length;
try {
  for (let i = 0; i < areas.length; i++) { summary.push(await runArea(browser, areas[i], per, i)); write(summary); }
} finally {
  await browser.close().catch(() => {});
  serverProc?.kill();
  write(summary);
}
console.log(`done: ${issues.size} unique issues -> ${join(outDir, "report.md")}`);
process.exit(0);
