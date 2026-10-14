/**
 * Draws a cylindrical panorama into a canvas with the mapping of geometry.ts: per pixel in a
 * WebGL fragment shader, or in narrow vertical strips with Canvas 2D when WebGL (or a texture of
 * the picture's size) is not available.
 */
import { isFullTurn, radius, type PanoGeometry, type PanoView } from "./geometry";

export interface Renderer {
  /** `scale` = canvas CSS pixels per view pixel; `dpr` = device pixels per CSS pixel. */
  draw(view: PanoView, viewWidth: number, viewHeight: number, scale: number, dpr: number): void;
  dispose(): void;
  readonly kind: "webgl" | "canvas";
}

const VERT = `
attribute vec2 aPos;
void main() { gl_Position = vec4(aPos, 0.0, 1.0); }
`;

const FRAG = `
precision highp float;
uniform sampler2D uTex;
uniform vec2 uCanvas;      // canvas size in device pixels
uniform float uToView;     // view pixels per device pixel
uniform float uYaw;        // radians
uniform float uF;          // focal length, view pixels
uniform float uShift;      // view pixels
uniform float uR;          // cylinder radius, picture pixels
uniform float uHorizon;    // picture row
uniform vec2 uPic;         // picture size
uniform float uPart;       // radians covered by the picture
uniform float uWrap;       // 1 for a full turn
void main() {
  vec2 d = vec2(gl_FragCoord.x, uCanvas.y - gl_FragCoord.y) - 0.5 * uCanvas;
  vec2 p = d * uToView;
  float ang = uYaw + atan(p.x, uF);
  float u = ang / uPart;
  if (uWrap > 0.5) u = fract(u);
  float row = uHorizon + (p.y + uShift) * uR / length(vec2(p.x, uF));
  float v = row / uPic.y;
  if (u < 0.0 || u > 1.0 || v < 0.0 || v > 1.0) { gl_FragColor = vec4(0.0, 0.0, 0.0, 1.0); return; }
  gl_FragColor = texture2D(uTex, vec2(u, v));
}
`;

function compile(gl: WebGLRenderingContext, type: number, src: string): WebGLShader {
  const s = gl.createShader(type)!;
  gl.shaderSource(s, src);
  gl.compileShader(s);
  if (!gl.getShaderParameter(s, gl.COMPILE_STATUS)) throw new Error(gl.getShaderInfoLog(s) ?? "shader error");
  return s;
}

export interface Picture {
  source: HTMLImageElement | ImageBitmap;
  /** Decoded size (may differ from the geometry's declared picture size). */
  width: number;
  height: number;
}

function webglRenderer(canvas: HTMLCanvasElement, pic: Picture, g: PanoGeometry): Renderer | null {
  const gl = canvas.getContext("webgl", { antialias: false, alpha: false, preserveDrawingBuffer: false }) as WebGLRenderingContext | null;
  if (!gl) return null;
  const max = gl.getParameter(gl.MAX_TEXTURE_SIZE) as number;
  if (pic.width > max || pic.height > max) return null;
  const prog = gl.createProgram()!;
  gl.attachShader(prog, compile(gl, gl.VERTEX_SHADER, VERT));
  gl.attachShader(prog, compile(gl, gl.FRAGMENT_SHADER, FRAG));
  gl.linkProgram(prog);
  if (!gl.getProgramParameter(prog, gl.LINK_STATUS)) return null;
  gl.useProgram(prog);
  const buf = gl.createBuffer();
  gl.bindBuffer(gl.ARRAY_BUFFER, buf);
  gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([-1, -1, 1, -1, -1, 1, 1, 1]), gl.STATIC_DRAW);
  const aPos = gl.getAttribLocation(prog, "aPos");
  gl.enableVertexAttribArray(aPos);
  gl.vertexAttribPointer(aPos, 2, gl.FLOAT, false, 0, 0);
  const tex = gl.createTexture();
  gl.bindTexture(gl.TEXTURE_2D, tex);
  // Non-power-of-two pictures: no mipmaps, clamp; the shader wraps u itself.
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR);
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR);
  gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGB, gl.RGB, gl.UNSIGNED_BYTE, pic.source);
  const loc = (n: string) => gl.getUniformLocation(prog, n);
  const u = {
    canvas: loc("uCanvas"),
    toView: loc("uToView"),
    yaw: loc("uYaw"),
    f: loc("uF"),
    shift: loc("uShift"),
    r: loc("uR"),
    horizon: loc("uHorizon"),
    pic: loc("uPic"),
    part: loc("uPart"),
    wrap: loc("uWrap"),
  };
  gl.uniform1f(u.r, radius(g));
  gl.uniform1f(u.horizon, g.horizon);
  gl.uniform2f(u.pic, g.width, g.height);
  gl.uniform1f(u.part, (g.part * Math.PI) / 180);
  gl.uniform1f(u.wrap, isFullTurn(g) ? 1 : 0);
  return {
    kind: "webgl",
    draw(view, _vw, _vh, scale, dpr) {
      gl.viewport(0, 0, canvas.width, canvas.height);
      gl.uniform2f(u.canvas, canvas.width, canvas.height);
      gl.uniform1f(u.toView, 1 / (scale * dpr));
      gl.uniform1f(u.yaw, (view.yaw * Math.PI) / 180);
      gl.uniform1f(u.f, view.f);
      gl.uniform1f(u.shift, view.shift);
      gl.drawArrays(gl.TRIANGLE_STRIP, 0, 4);
    },
    dispose() {
      gl.deleteTexture(tex);
      gl.deleteBuffer(buf);
      gl.deleteProgram(prog);
      gl.getExtension("WEBGL_lose_context")?.loseContext();
    },
  };
}

/** Canvas 2D: each strip of `STRIP` device pixels is one column of the picture scaled vertically. */
const STRIP = 2;

function canvasRenderer(canvas: HTMLCanvasElement, pic: Picture, g: PanoGeometry): Renderer | null {
  const ctx = canvas.getContext("2d");
  if (!ctx) return null;
  const img = pic.source;
  // Geometry is in declared picture pixels; the decoded picture may be another size.
  const kx = pic.width / g.width;
  const ky = pic.height / g.height;
  const r = radius(g);
  const wrap = isFullTurn(g);
  return {
    kind: "canvas",
    draw(view, _vw, _vh, scale, dpr) {
      const W = canvas.width;
      const H = canvas.height;
      const toView = 1 / (scale * dpr);
      ctx.fillStyle = "#000";
      ctx.fillRect(0, 0, W, H);
      ctx.imageSmoothingEnabled = true;
      const halfH = (H / 2) * toView;
      for (let sx = 0; sx < W; sx += STRIP) {
        const x0 = (sx - W / 2) * toView;
        const x1 = (sx + STRIP - W / 2) * toView;
        const xm = (x0 + x1) / 2;
        const a0 = view.yaw + (Math.atan2(x0, view.f) * 180) / Math.PI;
        const a1 = view.yaw + (Math.atan2(x1, view.f) * 180) / Math.PI;
        let u0 = (a0 / g.part) * g.width;
        const uw = ((a1 - a0) / g.part) * g.width;
        if (wrap) u0 = ((u0 % g.width) + g.width) % g.width;
        else if (u0 < 0 || u0 + uw > g.width) continue;
        const k = r / Math.hypot(xm, view.f);
        const top = g.horizon + (-halfH + view.shift) * k;
        const hgt = 2 * halfH * k;
        const first = Math.min(uw, g.width - u0);
        ctx.drawImage(img, u0 * kx, top * ky, Math.max(first * kx, 0.5), hgt * ky, sx, 0, (STRIP * first) / uw, H);
        if (first < uw) ctx.drawImage(img, 0, top * ky, (uw - first) * kx, hgt * ky, sx + (STRIP * first) / uw, 0, (STRIP * (uw - first)) / uw, H);
      }
    },
    dispose() {},
  };
}

/**
 * A renderer of the requested kind, or null. A canvas keeps the first context type it is asked
 * for, so after a WebGL failure the caller retries Canvas 2D on a fresh canvas.
 */
export function createRenderer(canvas: HTMLCanvasElement, pic: Picture, g: PanoGeometry, prefer: "webgl" | "canvas" = "webgl"): Renderer | null {
  if (prefer === "webgl") {
    try {
      const r = webglRenderer(canvas, pic, g);
      if (r) return r;
    } catch {
      // the caller falls back to Canvas 2D
    }
    return null;
  }
  return canvasRenderer(canvas, pic, g);
}
