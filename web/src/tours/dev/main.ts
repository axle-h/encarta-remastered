/**
 * Development page for the tour runtime (web/tours.html): tour picker, a stand-in for the host's
 * site map and position readout, the host's "Show Labels and Hot Spots" toggle (2D.ON / 2D.OFF),
 * a `do()` console and the event log. The Step 4 client replaces this page.
 */
import { mountTour, type TourEvent, type TourHandle } from "../runtime";
import { parseWorldLoaded, siteMapMarker, type SiteMapConfig } from "../sitemap";
import type { GroundMode } from "../sim/camera";

const BASE = "/out/tours";
const $ = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;
const params = new URLSearchParams(location.search);

const tourSel = $<HTMLSelectElement>("tour");
const sizeSel = $<HTMLSelectElement>("size");
const groundSel = $<HTMLSelectElement>("ground");
const deviceSel = $<HTMLSelectElement>("device");
const fovSel = $<HTMLSelectElement>("fov");
const pvsBox = $<HTMLInputElement>("pvs");
const labelsBox = $<HTMLInputElement>("labels");
const stage = $<HTMLDivElement>("stage");
const logEl = $<HTMLPreElement>("log");
const posEl = $<HTMLPreElement>("pos");
const statsEl = $<HTMLPreElement>("stats");
const floorEl = $<HTMLParagraphElement>("floor");
const mapCanvas = $<HTMLCanvasElement>("sitemap");

let handle: TourHandle | null = null;
let siteMap: SiteMapConfig | null = null;
let marker: { px: number; py: number; heading: number } | null = null;
let floor = "";

declare global {
  interface Window {
    tour?: TourHandle | null;
    tourEvents?: TourEvent[];
  }
}
window.tourEvents = [];

function log(line: string): void {
  logEl.textContent = `${line}\n${logEl.textContent ?? ""}`.slice(0, 20000);
}

function drawSiteMap(): void {
  const g = mapCanvas.getContext("2d");
  if (!g) return;
  g.clearRect(0, 0, mapCanvas.width, mapCanvas.height);
  if (!siteMap?.rect) return;
  const { left, top, right, bottom } = siteMap.rect;
  g.strokeStyle = "#8a8a84";
  g.strokeRect(left + 0.5, top + 0.5, right - left, bottom - top);
  if (!marker) return;
  const half = ((siteMap.fieldOfViewDeg / 2) * Math.PI) / 180;
  const h = (marker.heading * Math.PI) / 180;
  g.fillStyle = "rgba(255, 220, 90, 0.35)";
  g.beginPath();
  g.moveTo(marker.px, marker.py);
  g.arc(marker.px, marker.py, 28, -h - half, -h + half);
  g.closePath();
  g.fill();
  g.fillStyle = "#ffd84a";
  g.beginPath();
  g.arc(marker.px, marker.py, 3, 0, Math.PI * 2);
  g.fill();
}

function onEvent(ev: TourEvent): void {
  window.tourEvents?.push(ev);
  log(ev.text);
  if (ev.name === "WORLDLOADED") {
    siteMap = parseWorldLoaded(ev.pairs);
    drawSiteMap();
  } else if (ev.name === "USERLOCUPDATE") {
    // The host's handling (§4): query camera.pos? and move the "you are here" marker.
    const pos = handle?.do("camera.pos?").split(",").map(Number);
    if (siteMap && pos && pos.length >= 4) {
      marker = siteMapMarker(siteMap, pos[0] ?? 0, pos[2] ?? 0, pos[3] ?? 0);
      drawSiteMap();
    }
  } else if (ev.name === "USERLEVELCHANGE") {
    floor = ev.pairs.find(([k]) => k.toUpperCase() === "QTSREFID")?.[1] ?? "";
  } else if (ev.name === "HOTSPOTCLICKED") {
    const id = ev.pairs.find(([k]) => k.toUpperCase() === "HOTSPOTID")?.[1];
    const link = handle?.scene.hotspots.find((h) => h.contentIdText === id);
    log(`  → media ${id}${link ? ` “${link.title}”` : " (no callback)"}`);
  }
}

async function mount(): Promise<void> {
  handle?.destroy();
  handle = null;
  window.tour = null;
  siteMap = null;
  marker = null;
  floor = "";
  drawSiteMap();
  logEl.textContent = "";
  const size = sizeSel.value;
  if (size === "fill") {
    stage.className = "fill";
    stage.style.width = stage.style.height = "";
  } else {
    const [w, h] = size.split("x").map(Number);
    stage.className = "";
    stage.style.width = `${w}px`;
    stage.style.height = `${h}px`;
  }
  const id = tourSel.value;
  const qs = new URLSearchParams({ tour: id, size, device: deviceSel.value, ground: groundSel.value, pvs: pvsBox.checked ? "1" : "0", fov: fovSel.value });
  history.replaceState(null, "", `?${qs}`);
  try {
    handle = await mountTour(stage, {
      baseUrl: BASE,
      tourId: id,
      onEvent,
      control: {
        device: deviceSel.value as "hardware" | "software",
        groundMode: (groundSel.value || undefined) as GroundMode | undefined,
        fovAxis: fovSel.value as "horizontal" | "vertical",
      },
      view: { pvs: pvsBox.checked },
    });
    window.tour = handle;
    await handle.loaded;
    handle.do(labelsBox.checked ? "2D.ON" : "2D.OFF");
    (stage.querySelector(".encarta-tour") as HTMLElement | null)?.focus();
  } catch (e) {
    log(`error: ${String(e)}`);
    console.error(e);
  }
}

async function init(): Promise<void> {
  const res = await fetch(`${BASE}/index.json`);
  const index = (await res.json()) as { tours: { id: string; name: string }[] };
  for (const t of index.tours) tourSel.add(new Option(`${t.name} (${t.id})`, t.id));
  const want = params.get("tour");
  if (want) tourSel.value = want;
  if (params.get("size")) sizeSel.value = params.get("size") ?? sizeSel.value;
  if (params.get("device") !== null) deviceSel.value = params.get("device") ?? "hardware";
  if (params.get("ground") !== null) groundSel.value = params.get("ground") ?? "";
  if (params.get("pvs") !== null) pvsBox.checked = params.get("pvs") !== "0";
  if (params.get("fov")) fovSel.value = params.get("fov") ?? "horizontal";
  for (const el of [tourSel, sizeSel, deviceSel, groundSel, pvsBox, fovSel]) el.addEventListener("change", () => void mount());
  labelsBox.addEventListener("change", () => handle?.do(labelsBox.checked ? "2D.ON" : "2D.OFF"));
  $<HTMLFormElement>("cmdform").addEventListener("submit", (e) => {
    e.preventDefault();
    const cmd = $<HTMLInputElement>("cmd").value;
    $<HTMLDivElement>("cmdout").textContent = handle ? `→ ${handle.do(cmd)}` : "no tour";
  });
  setInterval(() => {
    if (!handle) return;
    const c = handle.control;
    const s = handle.stats();
    posEl.textContent =
      `camera.pos? ${handle.do("camera.pos?")}\n` +
      `pitch ${c.camera.pitchDeg.toFixed(1)}°  walk map ${c.walk.active?.name ?? "–"}\n` +
      `hotspot cell ${c.walk.active?.current?.index ?? "–"}  floor ${floor || "–"}`;
    statsEl.textContent = `${s.fps.toFixed(0)} fps  ${s.drawCalls} calls  ${s.triangles} tris  pvs cell ${s.pvsCell ?? "–"}`;
    floorEl.textContent = siteMap ? `XOFFSET ${siteMap.xOffset} ZOFFSET ${siteMap.zOffset} rotate ${siteMap.rotateFrame}` : "";
  }, 200);
  await mount();
}

void init();
