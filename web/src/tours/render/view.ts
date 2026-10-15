/**
 * The 3D view: loads scene.glb and pvs.bin and draws the control's state with three.js
 * (tours-3d.md §6.6 PVS, §7.2 lighting/flicker, §7.3 cloud impostors, §7.4 sprites, film strips
 * and water, §7.6 LOD). Rendering is done without colour-space conversion: texel bytes × COLOR_0
 * bytes as in the original's display-space modulate (docs/tour-scene.md).
 */
import * as THREE from "three";
import { GLTFLoader, type GLTF } from "three/examples/jsm/loaders/GLTFLoader.js";
import type { ObjectRt, TourControl } from "../control";
import { cloudPhi } from "../sim/effects";
import type { DrawUnit, TourScene } from "../types";
import { chunkKeys, impostorMaterial, sharedUniforms, tourMaterial, type SharedUniforms } from "./materials";

export interface ViewOptions {
  /** Honour the walk-map PVS (§6.6). */
  pvs?: boolean;
  /** Draw sprites even when the PVS does not list them (VM: they are drawn; default on). */
  alwaysDrawSprites?: boolean;
  /** Re-render cloud impostors only every ~5° (hardware mode, §7.3.4). */
  cloudHardwareThreshold?: boolean;
  /** Draw faces from both sides (back-face culling is a VM item, §5.9). */
  doubleSided?: boolean;
  pixelRatio?: number;
}

interface UnitRt {
  def: DrawUnit;
  mesh: THREE.Mesh;
  /** Original index buffer (triangles in face order). */
  orig: Uint32Array | Uint16Array;
  index: THREE.BufferAttribute;
  /** Allowed by the current PVS cell. */
  pvsOn: boolean;
  /** The index buffer holds every triangle (not a PVS subset). */
  full: boolean;
}

interface NodeRt {
  node: string;
  root: THREE.Object3D;
  obj: ObjectRt | null;
  units: UnitRt[];
  lodLow: NodeRt | null;
  isLodMesh: boolean;
  centre: THREE.Vector3;
  /** Saved U coordinates for film strips, per mesh. */
  savedU: Map<THREE.Mesh, Float32Array> | null;
  filmOffset: number;
  fullBright: boolean;
}

interface CloudRt {
  node: NodeRt;
  rt: THREE.WebGLRenderTarget;
  points: THREE.Points;
  w: number;
  h: number;
  lastPhi: number | null;
  rendered: boolean;
}

interface WaterTex {
  tex: THREE.DataTexture;
  src: Uint8ClampedArray;
  version: number;
}

export class TourView {
  readonly renderer: THREE.WebGLRenderer;
  readonly scene3d = new THREE.Scene();
  readonly camera = new THREE.PerspectiveCamera(80, 4 / 3, 0.01, 100);
  ready = false;
  error: string | null = null;
  readonly options: Required<ViewOptions>;

  private nodes: NodeRt[] = [];
  private nodeByName = new Map<string, NodeRt>();
  private units: UnitRt[] = [];
  private shared: SharedUniforms;
  /** Texture record (glTF material index) → every material variant using it. */
  private recordMaterials = new Map<number, Set<THREE.MeshBasicMaterial>>();
  private recordOriginal = new Map<number, THREE.Texture | null>();
  private recordAlpha = new Map<number, number>();
  private appliedVersion = -1;
  private lightsVersion = -1;
  private textureCache = new Map<string, THREE.Texture>();
  private pvs: Uint8Array | null = null;
  private pvsCell: number | null | undefined = undefined;
  private clouds: CloudRt[] = [];
  private pointsScene = new THREE.Scene();
  private pointsCamera = new THREE.OrthographicCamera(-1, 1, 1, -1, 0, 1);
  private water = new Map<number, WaterTex>();
  private flickerSlots = 0;

  constructor(
    readonly canvas: HTMLCanvasElement,
    private readonly control: TourControl,
    private readonly baseUrl: string,
    options: ViewOptions = {},
  ) {
    this.options = {
      pvs: options.pvs ?? true,
      alwaysDrawSprites: options.alwaysDrawSprites ?? true,
      cloudHardwareThreshold: options.cloudHardwareThreshold ?? control.options.device === "hardware",
      doubleSided: options.doubleSided ?? false,
      pixelRatio: options.pixelRatio ?? (typeof devicePixelRatio === "number" ? devicePixelRatio : 1),
    };
    // The drawing buffer is kept between frames for CAMERA.CLEARBACKGROUND=0 (§7.4.3).
    this.renderer = new THREE.WebGLRenderer({ canvas, antialias: false, alpha: false, powerPreference: "high-performance", preserveDrawingBuffer: true });
    this.renderer.outputColorSpace = THREE.LinearSRGBColorSpace;
    this.renderer.setPixelRatio(this.options.pixelRatio);
    this.renderer.autoClear = true;
    this.camera.rotation.order = "YXZ";
    this.flickerSlots = control.scene.lights?.flickerSlots.reduce((m, s) => Math.max(m, s.slot + 1), 0) ?? 0;
    this.shared = sharedUniforms(this.flickerSlots);
  }

  private url(path: string): string {
    return `${this.baseUrl.replace(/\/$/, "")}/${path}`;
  }

  /** Fetches and builds the scene; `progress` gets 0…1. */
  async load(progress?: (f: number) => void): Promise<void> {
    const scene = this.control.scene;
    const [glbBuf, pvsBuf] = await Promise.all([
      fetchWithProgress(this.url(scene.files.glb), progress),
      fetch(this.url(scene.files.pvs)).then((r) => (r.ok ? r.arrayBuffer() : new ArrayBuffer(0))),
    ]);
    this.pvs = new Uint8Array(pvsBuf);
    const gltf = await new GLTFLoader().parseAsync(glbBuf, "");
    this.build(gltf, scene);
    await this.preloadTextures();
    this.ready = true;
  }

  private build(gltf: GLTF, scene: TourScene): void {
    const assoc = gltf.parser.associations;
    // glTF node index → root object, (mesh, primitive) → THREE.Mesh.
    const byNodeIndex = new Map<number, THREE.Object3D>();
    const byPrim = new Map<string, THREE.Mesh>();
    const pointsByMesh = new Map<number, THREE.Points>();
    gltf.scene.traverse((o) => {
      const a = assoc.get(o) as { nodes?: number; meshes?: number; primitives?: number } | undefined;
      if (a?.nodes !== undefined) byNodeIndex.set(a.nodes, o);
      if ((o as THREE.Mesh).isMesh && a?.meshes !== undefined) byPrim.set(`${a.meshes}:${a.primitives ?? 0}`, o as THREE.Mesh);
      if ((o as THREE.Points).isPoints && a?.meshes !== undefined) pointsByMesh.set(a.meshes, o as THREE.Points);
    });
    const glbJson = gltf.parser.json as { nodes: { name?: string; mesh?: number }[]; meshes: { name?: string }[] };
    const nodeIndexByName = new Map<string, number>();
    glbJson.nodes.forEach((n, i) => n.name !== undefined && !nodeIndexByName.has(n.name) && nodeIndexByName.set(n.name, i));

    // Material variants per draw unit.
    const variants = new Map<string, THREE.MeshBasicMaterial>();
    const variantFor = (base: THREE.Material, record: number, flickerSets: number, fullBright: boolean) => {
      const key = `${record}:${flickerSets}:${fullBright}`;
      let m = variants.get(key);
      if (!m) {
        m = tourMaterial(base, this.shared, { flickerSets, fullBright });
        if (m.map) {
          m.map.colorSpace = THREE.NoColorSpace;
          this.deviceFilter(m.map);
        }
        if (this.options.doubleSided) m.side = THREE.DoubleSide;
        variants.set(key, m);
        let set = this.recordMaterials.get(record);
        if (!set) this.recordMaterials.set(record, (set = new Set()));
        set.add(m);
        if (!this.recordOriginal.has(record)) {
          this.recordOriginal.set(record, m.map);
          this.recordAlpha.set(record, m.alphaTest);
        }
      }
      return m;
    };

    for (const du of scene.model.drawUnits) {
      const mesh = byPrim.get(`${du.mesh}:${du.primitive}`);
      if (!mesh) continue;
      const geom = mesh.geometry;
      let sets = 0;
      for (let i = 0; ; i++) {
        const suffix = i === 0 ? "" : `_${i}`;
        const slots = geom.getAttribute(`_flicker_slots${suffix}`);
        const weights = geom.getAttribute(`_flicker_weights${suffix}`);
        if (!slots || !weights) break;
        geom.setAttribute(`aFlickerSlots${i}`, slots);
        geom.setAttribute(`aFlickerWeights${i}`, weights);
        sets++;
      }
      if (this.flickerSlots === 0) sets = 0;
      mesh.material = variantFor(mesh.material as THREE.Material, du.material, sets, false);
      mesh.frustumCulled = true;
      const idx = geom.getIndex();
      if (!idx) continue;
      const orig = (idx.array as Uint32Array | Uint16Array).slice();
      const index = new THREE.BufferAttribute(orig.slice(), 1);
      index.setUsage(THREE.DynamicDrawUsage);
      geom.setIndex(index);
      const unit: UnitRt = { def: du, mesh, orig, index, pvsOn: true, full: true };
      this.units.push(unit);
    }

    // Root nodes (separate objects and merged nodes).
    const unitsByNode = new Map<string, UnitRt[]>();
    for (const u of this.units) {
      const list = unitsByNode.get(u.def.node) ?? [];
      list.push(u);
      unitsByNode.set(u.def.node, list);
    }
    for (const [name, units] of unitsByNode) {
      const idx = units[0]?.def.nodeIndex ?? nodeIndexByName.get(name);
      const root = (idx !== undefined ? byNodeIndex.get(idx) : undefined) ?? units[0]?.mesh;
      if (!root) continue;
      const obj = this.control.objectByNode(name);
      const nrt: NodeRt = {
        node: name,
        root,
        obj,
        units,
        lodLow: null,
        isLodMesh: name.startsWith("lod_"),
        centre: root.position.clone(),
        savedU: null,
        filmOffset: -1,
        fullBright: false,
      };
      this.nodes.push(nrt);
      this.nodeByName.set(name, nrt);
    }
    for (const pair of scene.model.lod.pairs) {
      if (!pair.base) continue;
      const base = this.nodeByName.get(pair.base);
      const low = this.nodeByName.get(pair.lowDetail);
      if (base && low) base.lodLow = low;
    }

    // Cloud impostors: one render target per placement, the shared POINTS data per file.
    const cloudGeoms = new Map<string, THREE.BufferGeometry>();
    const impostor = impostorMaterial();
    for (const placement of scene.clouds) {
      const host = this.nodeByName.get(placement.placeholder);
      const meshIndex = glbJson.meshes.findIndex((m) => m.name === `cloud:${placement.file}`);
      const pts = pointsByMesh.get(meshIndex);
      if (!host || !pts) continue;
      let geom = cloudGeoms.get(placement.file);
      if (!geom) {
        const src = pts.geometry;
        geom = new THREE.BufferGeometry();
        const pos = src.getAttribute("position");
        const col = src.getAttribute("color");
        const key = src.getAttribute("_key");
        if (!pos || !col || !key) continue;
        geom.setAttribute("position", pos);
        geom.setAttribute("color", col);
        geom.setAttribute("aChunk", new THREE.BufferAttribute(chunkKeys(key.array as Float32Array), 2));
        geom.setDrawRange(0, Math.max(0, pos.count - 1));
        cloudGeoms.set(placement.file, geom);
      }
      const [w, h] = placement.textureSize;
      const rt = new THREE.WebGLRenderTarget(w, h, {
        depthBuffer: true,
        minFilter: THREE.LinearFilter,
        magFilter: THREE.LinearFilter,
        generateMipmaps: false,
      });
      rt.texture.colorSpace = THREE.NoColorSpace;
      this.deviceFilter(rt.texture);
      const points = new THREE.Points(geom, impostor.clone());
      points.frustumCulled = false;
      // The placeholder quad shows the impostor; give it its own material (the original draws each
      // cloud into the placeholder's texture right before drawing the quad).
      for (const u of host.units) {
        const m = (u.mesh.material as THREE.MeshBasicMaterial).clone();
        m.onBeforeCompile = (u.mesh.material as THREE.MeshBasicMaterial).onBeforeCompile;
        m.customProgramCacheKey = (u.mesh.material as THREE.MeshBasicMaterial).customProgramCacheKey;
        m.map = rt.texture;
        m.alphaTest = 0.5;
        u.mesh.material = m;
      }
      this.clouds.push({ node: host, rt, points, w, h, lastPhi: null, rendered: false });
    }
    // The static point-cloud nodes of the glb are replaced by the impostors.
    for (const p of pointsByMesh.values()) p.visible = false;
    gltf.scene.traverse((o) => {
      if ((o as THREE.Points).isPoints) o.visible = false;
    });

    this.scene3d.add(gltf.scene);
    for (const c of this.clouds) this.pointsScene.add(c.points);
    for (const c of this.clouds) c.points.visible = false;
  }

  /** Loads every image the scripts may apply as a texture (APPLYTEXTURE / FILMSTRIP). */
  private async preloadTextures(): Promise<void> {
    const paths = new Set<string>();
    const walk = (steps: readonly { op: string; image?: string | null; steps?: unknown }[]) => {
      for (const s of steps) {
        if ((s.op === "applyTexture" || s.op === "filmstrip") && s.image) paths.add(s.image);
      }
    };
    walk(this.control.scene.loadSequence);
    for (const m of this.control.scene.macros) walk(m.steps);
    await Promise.all([...paths].map((p) => this.texture(p)));
  }

  /** The software device samples the nearest texel without mipmaps; hardware filters (§5.10; VM pass 4). */
  private deviceFilter(t: THREE.Texture): void {
    if (this.control.options.device !== "software") return;
    t.minFilter = t.magFilter = THREE.NearestFilter;
    t.generateMipmaps = false;
    t.needsUpdate = true;
  }

  private texture(path: string): Promise<THREE.Texture> {
    const cached = this.textureCache.get(path);
    if (cached) return Promise.resolve(cached);
    return new THREE.TextureLoader().loadAsync(this.url(path)).then((t) => {
      t.flipY = false;
      t.colorSpace = THREE.NoColorSpace;
      t.wrapS = t.wrapT = THREE.RepeatWrapping;
      if (path.endsWith(".key.png")) {
        t.generateMipmaps = false;
        t.minFilter = THREE.LinearFilter;
      }
      this.deviceFilter(t);
      t.needsUpdate = true;
      this.textureCache.set(path, t);
      return t;
    });
  }

  // ------------------------------------------------------------------ per frame

  resize(w: number, h: number): void {
    this.renderer.setSize(w, h, false);
  }

  render(): void {
    const c = this.control;
    const cam = c.camera;
    const bg = cam.backgroundColor;
    this.renderer.setClearColor(new THREE.Color(bg[0] / 255, bg[1] / 255, bg[2] / 255), 1);
    const contentReady = this.ready && c.modelLoaded && !c.loading;
    // With CLEARBACKGROUND=0 (every tour) only depth is cleared: wherever no visible object draws, the
    // last frame stays, and with 3D off the last 3D picture stays under the 2D layer (§7.4.3).
    const keepColour = contentReady && cam.clearBackground === 0;
    this.renderer.autoClearColor = !keepColour;
    if (!contentReady || !c.modelOn) {
      if (!keepColour) this.renderer.clear();
      return;
    }
    const v = c.view();
    this.camera.position.set(cam.x, cam.y, cam.z);
    this.camera.rotation.set((cam.pitchDeg * Math.PI) / 180, (-cam.yawDeg * Math.PI) / 180, 0, "YXZ");
    this.camera.fov = v.fovY;
    this.camera.aspect = c.viewport.w / Math.max(1, c.viewport.h);
    this.camera.near = cam.near > 0 ? cam.near : 0.01;
    const worldFar = 2 * c.scene.model.worldSize;
    this.camera.far = cam.far > 0 ? cam.far : worldFar;
    this.camera.updateProjectionMatrix();
    this.camera.updateMatrixWorld();

    this.scene3d.fog = c.fog.enabled ? new THREE.Fog(new THREE.Color(...c.fog.color.map((x) => x / 255)), c.fog.start, c.fog.end) : null;

    this.syncTextures();
    this.syncLights();
    this.syncPvs();
    this.syncNodes();
    this.syncWater();
    this.renderClouds();
    this.renderer.setRenderTarget(null);
    this.renderer.render(this.scene3d, this.camera);
  }

  /** APPLYTEXTURE / FILMSTRIP replace the shared texture record of the object's first face (§7.4.5). */
  private syncTextures(): void {
    const c = this.control;
    if (c.textureVersion === this.appliedVersion) return;
    this.appliedVersion = c.textureVersion;
    for (const [record, mats] of this.recordMaterials) {
      if (this.water.has(record)) continue;
      const path = c.textureRecords.get(record);
      const tex = path ? (this.textureCache.get(path) ?? null) : (this.recordOriginal.get(record) ?? null);
      if (path && !tex) void this.texture(path).then(() => (this.appliedVersion = -1));
      for (const m of mats) {
        if (this.clouds.some((cl) => cl.rt.texture === m.map)) continue;
        if (tex && m.map !== tex) {
          m.map = tex;
          m.alphaTest = path?.endsWith(".key.png") ? 0.5 : (this.recordAlpha.get(record) ?? 0);
          m.needsUpdate = true;
        }
      }
    }
  }

  /** Flicker slot colours and `LIGHTS.OFF` (§7.2.4, §7.2.6). */
  private syncLights(): void {
    const c = this.control;
    if (c.lightsVersion === this.lightsVersion) return;
    this.lightsVersion = c.lightsVersion;
    this.shared.uLightsOff.value = c.lightsOn === 0 || c.lights.length === 0 ? 1 : 0;
    for (const l of c.lights) {
      if (l.slot === null) continue;
      const v = this.shared.uFlicker.value[l.slot];
      if (!v) continue;
      if (!l.on || !l.flicker) v.set(0, 0, 0);
      else v.set(l.flicker.current[0], l.flicker.current[1], l.flicker.current[2]);
    }
  }

  /** The visibility cell under the eye selects the triangles drawn (§6.6). */
  private syncPvs(): void {
    const c = this.control;
    let cell: number | null = null;
    let runs: { first: number; count: number } | null = null;
    if (this.options.pvs && c.walk.enabled && c.walk.active?.hasKind("visibility")) {
      const v = c.walk.active.find("visibility", c.camera.x, c.camera.z);
      if (v) {
        cell = v.index + 1_000_000 * c.walk.maps().indexOf(c.walk.active);
        runs = { first: v.pvs.firstRun, count: v.pvs.runCount };
      }
    }
    if (cell === this.pvsCell) return;
    this.pvsCell = cell;
    if (!runs || !this.pvs) {
      for (const u of this.units) {
        u.pvsOn = true;
        this.restoreFull(u);
      }
      return;
    }
    const dv = new DataView(this.pvs.buffer, this.pvs.byteOffset, this.pvs.byteLength);
    const fill = new Map<number, number>();
    for (const u of this.units) u.pvsOn = false;
    const unitByIndex = this.unitIndex();
    for (let r = 0; r < runs.count; r++) {
      const off = (runs.first + r) * 8;
      if (off + 8 > dv.byteLength) break;
      const firstTri = dv.getUint32(off, true);
      const du = dv.getUint16(off + 4, true);
      const count = dv.getUint16(off + 6, true);
      const u = unitByIndex[du];
      if (!u) continue;
      const at = fill.get(du) ?? 0;
      (u.index.array as Uint32Array).set(u.orig.subarray(firstTri * 3, (firstTri + count) * 3), at);
      fill.set(du, at + count * 3);
      u.pvsOn = true;
      u.full = false;
    }
    for (const [du, n] of fill) {
      const u = unitByIndex[du];
      if (!u) continue;
      u.index.clearUpdateRanges();
      u.index.addUpdateRange(0, n);
      u.index.needsUpdate = true;
      u.mesh.geometry.setDrawRange(0, n);
    }
  }

  private unitIndexCache: (UnitRt | undefined)[] | null = null;
  private unitIndex(): (UnitRt | undefined)[] {
    if (!this.unitIndexCache) {
      const arr: (UnitRt | undefined)[] = [];
      const defs = this.control.scene.model.drawUnits;
      const byDef = new Map(this.units.map((u) => [u.def, u]));
      defs.forEach((d, i) => (arr[i] = byDef.get(d)));
      this.unitIndexCache = arr;
    }
    return this.unitIndexCache;
  }

  private restoreFull(u: UnitRt): void {
    if (u.full) return;
    (u.index.array as Uint32Array).set(u.orig);
    u.index.clearUpdateRanges();
    u.index.needsUpdate = true;
    u.mesh.geometry.setDrawRange(0, Infinity);
    u.full = true;
  }

  /** Visibility, LOD, sprites and film strips per node. */
  private syncNodes(): void {
    const c = this.control;
    const cam = c.camera;
    const yaw = (-cam.yawDeg * Math.PI) / 180;
    const lodDistance = c.scene.model.lod.switchDistance ?? 13;
    const inPvs = this.pvsCell !== null && this.pvsCell !== undefined;
    const shown = new Set<NodeRt>();
    for (const n of this.nodes) {
      const visible = n.obj ? n.obj.visible : true;
      if (!visible) continue;
      const choice = lodChoice(inPvs, n.isLodMesh, !!n.lodLow, Math.hypot(n.centre.x - cam.x, n.centre.z - cam.z), lodDistance);
      if (choice === "self") shown.add(n);
      else if (choice === "partner" && n.lodLow) shown.add(n.lodLow);
    }
    for (const n of this.nodes) {
      const on = shown.has(n);
      n.root.visible = on;
      const sprite = !!n.obj?.sprite;
      const always = sprite && this.options.alwaysDrawSprites;
      if (on && always) for (const u of n.units) this.restoreFull(u);
      for (const u of n.units) u.mesh.visible = on && (u.pvsOn || always);
      if (!on) continue;
      if (sprite) n.root.rotation.set(0, yaw, 0);
      else if (n.root.rotation.y !== 0) n.root.rotation.set(0, 0, 0);
      this.syncFilmstrip(n);
      this.syncFullBright(n);
    }
  }

  private syncFullBright(n: NodeRt): void {
    const want = !!n.obj && (n.obj.sprite || n.obj.noIllumination) && !n.obj.info.state.sprite && !n.obj.info.state.noIllumination;
    if (want === n.fullBright) return;
    n.fullBright = want;
    for (const u of n.units) {
      const cur = u.mesh.material as THREE.MeshBasicMaterial;
      const m = tourMaterial(cur, this.shared, { flickerSets: 0, fullBright: want });
      m.map = cur.map;
      u.mesh.material = m;
    }
  }

  /** U = savedU / N + (frame − 1) / N on the object's own vertices (§7.4.4). */
  private syncFilmstrip(n: NodeRt): void {
    const fs = n.obj?.filmstrip ?? null;
    const offset = fs ? fs.offset : -1;
    if (offset === n.filmOffset) return;
    n.filmOffset = offset;
    if (!n.savedU) {
      n.savedU = new Map();
      for (const u of n.units) {
        const uv = u.mesh.geometry.getAttribute("uv") as THREE.BufferAttribute | undefined;
        if (!uv) continue;
        const saved = new Float32Array(uv.count);
        for (let i = 0; i < uv.count; i++) saved[i] = uv.getX(i);
        n.savedU.set(u.mesh, saved);
      }
    }
    for (const u of n.units) {
      const uv = u.mesh.geometry.getAttribute("uv") as THREE.BufferAttribute | undefined;
      const saved = n.savedU.get(u.mesh);
      if (!uv || !saved) continue;
      for (let i = 0; i < uv.count; i++) uv.setX(i, fs ? (saved[i] ?? 0) / fs.frames + fs.offset : (saved[i] ?? 0));
      uv.needsUpdate = true;
    }
  }

  /** The water ripple replaces its texture record while on (§7.4.6). */
  private syncWater(): void {
    const c = this.control;
    for (const [record, w] of c.water) {
      let wt = this.water.get(record);
      if (!wt) {
        const orig = this.recordOriginal.get(record);
        const img = orig?.image as (CanvasImageSource & { width: number; height: number }) | undefined;
        if (!img || !img.width) continue;
        const cv = document.createElement("canvas");
        cv.width = img.width;
        cv.height = img.height;
        const g = cv.getContext("2d", { willReadFrequently: true });
        if (!g) continue;
        g.drawImage(img, 0, 0);
        const src = g.getImageData(0, 0, cv.width, cv.height).data;
        const tex = new THREE.DataTexture(new Uint8Array(src.length), cv.width, cv.height, THREE.RGBAFormat);
        tex.colorSpace = THREE.NoColorSpace;
        tex.wrapS = tex.wrapT = THREE.RepeatWrapping;
        tex.magFilter = THREE.LinearFilter;
        tex.minFilter = THREE.LinearFilter;
        this.deviceFilter(tex);
        tex.flipY = false;
        wt = { tex, src, version: -1 };
        this.water.set(record, wt);
        w.size = [cv.width, cv.height];
        for (const m of this.recordMaterials.get(record) ?? []) {
          m.map = tex;
          m.needsUpdate = true;
        }
      }
      if (w.ripple && w.version !== wt.version) {
        wt.version = w.version;
        w.ripple.render(wt.src, wt.tex.image.data as Uint8Array);
        wt.tex.needsUpdate = true;
      }
    }
    for (const [record, wt] of this.water) {
      if (c.water.has(record)) continue;
      this.water.delete(record);
      wt.tex.dispose();
      const orig = this.recordOriginal.get(record) ?? null;
      const path = c.textureRecords.get(record);
      for (const m of this.recordMaterials.get(record) ?? []) {
        m.map = (path ? this.textureCache.get(path) : null) ?? orig;
        m.needsUpdate = true;
      }
    }
  }

  /** Cloud impostors (§7.3.3–§7.3.4): re-projected into the placeholder texture before drawing. */
  private renderClouds(): void {
    const c = this.control;
    const cam = c.camera;
    const v = c.view();
    for (const cl of this.clouds) {
      const n = cl.node;
      const obj = n.obj;
      if (!obj?.cloud || !n.root.visible || !n.units.some((u) => u.mesh.visible)) continue;
      const cx = n.centre.x, cz = n.centre.z, cy = n.centre.y;
      const phi = cloudPhi(cx, -cz, cam.x, -cam.z, obj.cloudOffset);
      const dx = cx - cam.x, dy = cy - cam.y, dz = cz - cam.z;
      const dist = Math.hypot(dx, dy, dz);
      if (cl.rendered) {
        if (this.options.cloudHardwareThreshold && cl.lastPhi !== null && Math.abs(phi - cl.lastPhi) < 0.09) continue;
        if (dist > 0 && (dx * v.fwd[0] + dy * v.fwd[1] + dz * v.fwd[2]) / dist < 0) continue;
        if (dist >= 0.9 * c.fog.end) continue;
      }
      cl.lastPhi = phi;
      cl.rendered = true;
      const u = (cl.points.material as THREE.RawShaderMaterial).uniforms;
      const s = obj.cloud.scale;
      (u.uS as THREE.IUniform).value = s;
      (u.uCos as THREE.IUniform).value = Math.cos(phi);
      (u.uSin as THREE.IUniform).value = Math.sin(phi);
      (u.uW as THREE.IUniform).value = cl.w;
      (u.uH as THREE.IUniform).value = cl.h;
      (u.uViewDeg as THREE.IUniform).value = phi * 57.29578 - 90;
      for (const other of this.clouds) other.points.visible = other === cl;
      this.renderer.setRenderTarget(cl.rt);
      this.renderer.setClearColor(0x000000, 0);
      this.renderer.clear(true, true, false);
      this.renderer.render(this.pointsScene, this.pointsCamera);
    }
    const bg = c.camera.backgroundColor;
    this.renderer.setClearColor(new THREE.Color(bg[0] / 255, bg[1] / 255, bg[2] / 255), 1);
  }

  /** Rendering statistics for the dev page. */
  stats(): { drawCalls: number; triangles: number; pvsCell: number | null } {
    const info = this.renderer.info.render;
    return { drawCalls: info.calls, triangles: info.triangles, pvsCell: this.pvsCell ?? null };
  }

  dispose(): void {
    for (const cl of this.clouds) cl.rt.dispose();
    for (const t of this.textureCache.values()) t.dispose();
    this.renderer.dispose();
  }
}

async function fetchWithProgress(url: string, progress?: (f: number) => void): Promise<ArrayBuffer> {
  const res = await fetch(url);
  if (!res.ok) throw new Error(`${url}: HTTP ${res.status}`);
  const total = Number(res.headers.get("Content-Length") ?? 0);
  if (!res.body || !total || !progress) return res.arrayBuffer();
  const reader = res.body.getReader();
  const buf = new Uint8Array(total);
  let at = 0;
  for (;;) {
    const { done, value } = await reader.read();
    if (done) break;
    if (at + value.length > buf.length) return concatRest(buf.subarray(0, at), value, reader);
    buf.set(value, at);
    at += value.length;
    progress(at / total);
  }
  return buf.buffer.slice(0, at);
}

async function concatRest(head: Uint8Array, chunk: Uint8Array, reader: ReadableStreamDefaultReader<Uint8Array>): Promise<ArrayBuffer> {
  const parts = [head, chunk];
  for (;;) {
    const { done, value } = await reader.read();
    if (done) break;
    parts.push(value);
  }
  const n = parts.reduce((s, p) => s + p.length, 0);
  const out = new Uint8Array(n);
  let at = 0;
  for (const p of parts) {
    out.set(p, at);
    at += p.length;
  }
  return out.buffer;
}

/**
 * Which mesh of a LOD pair draws (§6.6, §7.6, VM). Outside the PVS a `lod_` mesh never draws on its
 * own and a base mesh with a partner is swapped for it beyond the XZ switch distance. Over a
 * visibility cell there is no distance swap: the cell's lists name base or `lod_` meshes (the
 * authoring tool chose per cell) and exactly the listed faces draw, so every visible mesh is a
 * candidate and the PVS runs decide.
 */
export function lodChoice(inPvs: boolean, isLodMesh: boolean, hasPartner: boolean, xzDistance: number, switchDistance: number): "self" | "partner" | "none" {
  if (inPvs) return "self";
  if (isLodMesh) return "none";
  if (!hasPartner) return "self";
  return xzDistance <= switchDistance ? "self" : "partner";
}
