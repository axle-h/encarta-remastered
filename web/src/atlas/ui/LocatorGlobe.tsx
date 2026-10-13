/**
 * The small locator globe in the top-left corner (Encarta draws the LOCATOR raster with a cross-hair
 * and the view's outline). Drawn by inverse orthographic projection of the LOCATOR LOOK's
 * equirectangular texture.
 */
import { useEffect, useRef, useState } from "react";
import { dataFile, type Endpoints } from "../data";

interface Props {
  ep: Endpoints;
  lat: number;
  lon: number;
  /** View outline as lon/lat points (map corners and edge midpoints), or null when the whole globe is visible. */
  outline: [number, number][] | null;
  size?: number;
  body: "earth" | "moon";
}

interface Texture {
  w: number;
  h: number;
  px: Uint8ClampedArray;
}

const RAD = Math.PI / 180;

export function project(lat: number, lon: number, lat0: number, lon0: number): { x: number; y: number; visible: boolean } {
  const φ = lat * RAD;
  const λ = (lon - lon0) * RAD;
  const φ0 = lat0 * RAD;
  const x = Math.cos(φ) * Math.sin(λ);
  const y = Math.cos(φ0) * Math.sin(φ) - Math.sin(φ0) * Math.cos(φ) * Math.cos(λ);
  const c = Math.sin(φ0) * Math.sin(φ) + Math.cos(φ0) * Math.cos(φ) * Math.cos(λ);
  return { x, y, visible: c >= 0 };
}

export function LocatorGlobe({ ep, lat, lon, outline, size = 76, body }: Props) {
  const canvas = useRef<HTMLCanvasElement>(null);
  const [tex, setTex] = useState<Texture | null>(null);

  useEffect(() => {
    let alive = true;
    const img = new Image();
    img.crossOrigin = "anonymous";
    img.onload = () => {
      if (!alive) return;
      const w = Math.min(img.naturalWidth, 1024);
      const h = Math.round((w * img.naturalHeight) / img.naturalWidth);
      const c = document.createElement("canvas");
      c.width = w;
      c.height = h;
      const g = c.getContext("2d", { willReadFrequently: true });
      if (!g) return;
      g.drawImage(img, 0, 0, w, h);
      setTex({ w, h, px: g.getImageData(0, 0, w, h).data });
    };
    img.src = body === "moon" ? dataFile("raster/moon/moon.equirect.webp", ep) : dataFile("raster/locator/landwater.equirect.webp", ep);
    return () => {
      alive = false;
    };
  }, [ep, body]);

  useEffect(() => {
    const cv = canvas.current;
    const g = cv?.getContext("2d");
    if (!cv || !g) return;
    const dpr = typeof window !== "undefined" ? Math.min(2, window.devicePixelRatio || 1) : 1;
    const S = Math.round(size * dpr);
    if (cv.width !== S) {
      cv.width = S;
      cv.height = S;
    }
    g.clearRect(0, 0, S, S);
    const r = S / 2 - 1;
    const c0 = S / 2;
    const φ0 = lat * RAD;
    if (tex) {
      const out = g.createImageData(S, S);
      for (let py = 0; py < S; py++) {
        for (let px = 0; px < S; px++) {
          const x = (px + 0.5 - c0) / r;
          const y = (c0 - py - 0.5) / r;
          const rho2 = x * x + y * y;
          if (rho2 > 1) continue;
          const z = Math.sqrt(1 - rho2);
          const la = Math.asin(z * Math.sin(φ0) + y * Math.cos(φ0));
          const lo = lon * RAD + Math.atan2(x, z * Math.cos(φ0) - y * Math.sin(φ0));
          let u = ((lo / RAD + 180) / 360) % 1;
          if (u < 0) u += 1;
          const v = (90 - la / RAD) / 180;
          const ti = (Math.min(tex.h - 1, Math.floor(v * tex.h)) * tex.w + Math.min(tex.w - 1, Math.floor(u * tex.w))) * 4;
          const shade = 0.55 + 0.45 * z;
          const o = (py * S + px) * 4;
          out.data[o] = tex.px[ti]! * shade;
          out.data[o + 1] = tex.px[ti + 1]! * shade;
          out.data[o + 2] = tex.px[ti + 2]! * shade;
          out.data[o + 3] = 255;
        }
      }
      g.putImageData(out, 0, 0);
    } else {
      g.fillStyle = "#3a5f8f";
      g.beginPath();
      g.arc(c0, c0, r, 0, Math.PI * 2);
      g.fill();
    }
    // Cross-hair through the view centre, and the view outline.
    g.strokeStyle = "rgba(0,0,0,0.85)";
    g.lineWidth = Math.max(1, dpr);
    g.beginPath();
    g.moveTo(c0, 0);
    g.lineTo(c0, S);
    g.moveTo(0, c0);
    g.lineTo(S, c0);
    g.stroke();
    if (outline && outline.length > 2) {
      g.strokeStyle = "#ffffff";
      g.beginPath();
      let first = true;
      for (const [lo, la] of outline) {
        const p = project(la, lo, lat, lon);
        const X = c0 + p.x * r;
        const Y = c0 - p.y * r;
        if (first) g.moveTo(X, Y);
        else g.lineTo(X, Y);
        first = false;
      }
      g.closePath();
      g.stroke();
    }
    g.strokeStyle = "rgba(255,255,255,0.5)";
    g.beginPath();
    g.arc(c0, c0, r, 0, Math.PI * 2);
    g.stroke();
  }, [tex, lat, lon, outline, size]);

  return <canvas ref={canvas} className="enc-atlas-locator" style={{ width: size, height: size }} aria-label="Locator globe" role="img" />;
}
