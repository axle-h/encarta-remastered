/**
 * Materials for the tour meshes: the converter's unlit materials (texture × COLOR_0, no colour
 * space conversion) extended with the runtime lighting terms of docs/tour-scene.md:
 * flicker (`min(255, COLOR_0 + Σ trunc(current · w))`), `LIGHTS.OFF` and full-bright objects.
 */
import * as THREE from "three";

/**
 * Colour-keyed textures (tours-3d.md §5.5, §5.10; inferred, not observed). The control enables D3D colour
 * keying with alpha test and blending off. A D3D driver gives each texel that matches the key alpha 0 *before*
 * filtering and keeps its colour, filters RGBA, and then (alpha test being off) tests "alpha > 0" itself, so
 * only pixels whose whole bilinear footprint is key are dropped. Edge pixels keep the key colour at its
 * filter weight: a magenta fringe about one texel wide. Point sampling (the software device) gives 0 or 1.
 */
export const KEY_ALPHA_REF = 0.5 / 255;
/** The key colour as a filtered texel shows it: 0xF81F (565) or 0x7C1F (555) expanded to 8 bits. */
export const KEY_RGB: readonly [number, number, number] = [255, 0, 255];

/**
 * Gives every transparent texel of RGBA data the key colour, as the original's texture surface holds it
 * (the browser's image decoding may have zeroed the colour of alpha-0 pixels). Returns the count.
 */
export function fillKeyTexels(rgba: Uint8Array | Uint8ClampedArray): number {
  let n = 0;
  for (let i = 0; i + 3 < rgba.length; i += 4) {
    if (rgba[i + 3] !== 0) continue;
    rgba[i] = KEY_RGB[0];
    rgba[i + 1] = KEY_RGB[1];
    rgba[i + 2] = KEY_RGB[2];
    n++;
  }
  return n;
}

/** Uniforms shared by every tour material. */
export interface SharedUniforms {
  /** Current colour of each flicker slot, 0–255 per channel. */
  uFlicker: { value: THREE.Vector3[] };
  /** 1 when `LIGHTS.OFF` makes everything full-bright. */
  uLightsOff: { value: number };
}

export function sharedUniforms(slots: number): SharedUniforms {
  return {
    uFlicker: { value: Array.from({ length: Math.max(1, slots) }, () => new THREE.Vector3()) },
    uLightsOff: { value: 0 },
  };
}

export interface MaterialVariant {
  /** Number of `_FLICKER_*` attribute pairs this variant reads (0 = none). */
  flickerSets: number;
  fullBright: boolean;
}

/**
 * Clones a loader material into a tour material variant. The clone keeps map, alpha test and
 * sidedness; the shader gets the flicker sum and the full-bright switches.
 */
export function tourMaterial(base: THREE.Material, shared: SharedUniforms, variant: MaterialVariant): THREE.MeshBasicMaterial {
  const src = base as THREE.MeshBasicMaterial;
  const m = new THREE.MeshBasicMaterial({
    map: src.map,
    vertexColors: true,
    // The converter marks keyed textures alphaMode MASK; the test is the driver's "alpha > 0".
    alphaTest: src.alphaTest > 0 ? KEY_ALPHA_REF : 0,
    transparent: false,
    side: src.side,
    fog: true,
  });
  m.name = src.name;
  m.userData = { ...src.userData, tourVariant: variant };
  const slots = shared.uFlicker.value.length;
  m.onBeforeCompile = (shader) => {
    shader.uniforms.uFlicker = shared.uFlicker;
    shader.uniforms.uLightsOff = shared.uLightsOff;
    let decl = `uniform float uLightsOff;\n`;
    let body = "";
    if (variant.flickerSets > 0) {
      decl += `uniform vec3 uFlicker[${slots}];\n`;
      body += `{\n  vec3 c = floor(vColor.rgb * 255.0 + 0.5);\n`;
      for (let i = 0; i < variant.flickerSets; i++) {
        decl += `attribute vec4 aFlickerSlots${i};\nattribute vec4 aFlickerWeights${i};\n`;
        for (const comp of ["x", "y", "z", "w"]) {
          body += `  c += floor(uFlicker[int(aFlickerSlots${i}.${comp} + 0.5)] * aFlickerWeights${i}.${comp});\n`;
        }
      }
      body += `  vColor.rgb = min(c, vec3(255.0)) / 255.0;\n}\n`;
    }
    body += variant.fullBright ? `vColor.rgb = vec3(1.0);\n` : `vColor.rgb = mix(vColor.rgb, vec3(1.0), uLightsOff);\n`;
    shader.vertexShader = decl + shader.vertexShader.replace("#include <color_vertex>", `#include <color_vertex>\n${body}`);
  };
  m.customProgramCacheKey = () => `tour:${variant.flickerSets}:${variant.fullBright ? 1 : 0}:${slots}`;
  return m;
}

/** Points → impostor texture (tours-3d.md §7.3.3), one texel per point, depth-tested. */
export function impostorMaterial(): THREE.RawShaderMaterial {
  return new THREE.RawShaderMaterial({
    glslVersion: THREE.GLSL3,
    uniforms: {
      uS: { value: 1 },
      uCos: { value: 1 },
      uSin: { value: 0 },
      uW: { value: 128 },
      uH: { value: 256 },
      uViewDeg: { value: 0 },
    },
    vertexShader: /* glsl */ `
      precision highp float;
      in vec3 position;
      in vec4 color;
      in vec2 aChunk;
      uniform float uS, uCos, uSin, uW, uH, uViewDeg;
      out vec3 vColor;
      float angleDiff(float a, float b) { float d = abs(a - b); return min(d, abs(d - 360.0)); }
      void main() {
        bool visible = aChunk.x < 0.0 || aChunk.y < 0.0
          || angleDiff(uViewDeg, aChunk.x) <= 95.0 || angleDiff(uViewDeg, aChunk.y) <= 95.0;
        float u0 = position.x * uS;
        float z0 = -position.z * uS;          // stored z is negated (glTF); the cloud uses the disc's z
        float col = floor(u0 * uCos + z0 * uSin);
        float ys = position.y * uS;
        float row = sign(ys) * floor(abs(ys));  // trunc
        float index = row * uW + floor(uW * 0.5) + col;
        if (!visible || index < 0.0 || index >= uW * uH) {
          gl_Position = vec4(2.0, 2.0, 2.0, 1.0);
          gl_PointSize = 1.0;
          vColor = vec3(0.0);
          return;
        }
        float ty = floor(index / uW);
        float tx = index - ty * uW;
        float depth = (z0 * uCos - u0 * uSin) * 64.0 + 65536.0;
        // Surface row 0 is the bottom row of the image (tour-scene.md: v' = 1 − v), so row ty goes
        // to texture row H − 1 − ty of the render target, whose first row is v = 0.
        gl_Position = vec4((tx + 0.5) / uW * 2.0 - 1.0, 1.0 - (ty + 0.5) / uH * 2.0, clamp(depth / 131072.0, 0.0, 1.0) * 2.0 - 1.0, 1.0);
        gl_PointSize = 1.0;
        vColor = color.rgb;
      }`,
    fragmentShader: /* glsl */ `
      precision highp float;
      in vec3 vColor;
      out vec4 outColor;
      void main() { outColor = vec4(vColor, 1.0); }`,
    depthTest: true,
    depthWrite: true,
    depthFunc: THREE.LessDepth,
  });
}

/** Per-point chunk boundary keys (§7.3.3: chunks of 500, end bound exclusive, last point never drawn). */
export function chunkKeys(keys: ArrayLike<number>): Float32Array {
  const n = keys.length;
  const out = new Float32Array(n * 2);
  for (let i = 0; i < n - 1; i += 500) {
    const j = Math.min(i + 500, n - 1);
    const k0 = keys[i] ?? -1, k1 = keys[j] ?? -1;
    for (let p = i; p < j; p++) {
      out[p * 2] = Number.isNaN(k0) ? -1 : k0;
      out[p * 2 + 1] = Number.isNaN(k1) ? -1 : k1;
    }
  }
  return out;
}
