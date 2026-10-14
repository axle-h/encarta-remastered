/** Panorama player helpers on synthetic data: geometry, parameters, the image-SWF reader. */
import { describe, expect, it } from "vitest";
import { quadrantRect } from "../pages/panorama";
import { yawParam } from "../pages/PanoramaTour";
import { clampView, deltaDeg, focalFromFov, fovFromFocal, minFocal, project, projectRect, radius, shiftRange, tourHfov, unproject, wedgeAngle, wrapDeg, yawRange, type PanoGeometry } from "./geometry";
import { initialView } from "./PanoramaView";
import { singleParams, slideImageUrl, tourParams, vrSlide } from "./params";
import { jpegSize, readPanoramaSwf } from "./swf";
import { fakeJpeg, imageSwf } from "./syntheticSwf";

const g: PanoGeometry = { width: 2000, height: 400, horizon: 220, part: 360 };

describe("cylinder geometry", () => {
  it("wraps angles", () => {
    expect(wrapDeg(-10)).toBe(350);
    expect(wrapDeg(725)).toBe(5);
    expect(deltaDeg(350)).toBe(-10);
    expect(deltaDeg(180)).toBe(180);
  });

  it("converts field of view and focal length", () => {
    const f = focalFromFov(80, 544);
    expect(f).toBeCloseTo(272 / Math.tan((40 * Math.PI) / 180), 6);
    expect(fovFromFocal(f, 544)).toBeCloseTo(80, 6);
  });

  it("projects and unprojects consistently", () => {
    const v = { yaw: 90, f: 300, shift: 12 };
    for (const [x, y] of [
      [0, 0],
      [-200, 150],
      [250, -170],
    ] as const) {
      const p = unproject(g, v, x, y);
      const q = project(g, v, p.px, p.py)!;
      expect(q.x).toBeCloseTo(x, 6);
      expect(q.y).toBeCloseTo(y, 6);
    }
    // The view centre sees the picture column at the yaw, at the horizon when unshifted.
    const c = unproject(g, { yaw: 90, f: 300, shift: 0 }, 0, 0);
    expect(c.px).toBeCloseTo(500, 6);
    expect(c.py).toBeCloseTo(220, 6);
    // Behind the viewer.
    expect(project(g, v, 1500, 200)).toBeNull();
  });

  it("wraps across the seam of a full turn", () => {
    const v = { yaw: 2, f: 300, shift: 0 };
    const p = project(g, v, 1990, 220)!; // 358.2°, just left of the centre
    expect(p.x).toBeLessThan(0);
    expect(p.angle).toBeCloseTo(-3.8, 6);
    expect(unproject(g, v, p.x, p.y).px).toBeCloseTo(1990, 6);
  });

  it("limits zoom and shift so the picture always fills the view", () => {
    const r = radius(g);
    expect(r).toBeCloseTo(2000 / (2 * Math.PI), 9);
    const fmin = minFocal(g, 300);
    expect(fmin).toBeCloseTo((300 * r) / 400, 9);
    // At the widest view the shift is fixed so the picture's top and bottom meet the view's edges.
    const [lo, hi] = shiftRange(g, fmin, 300);
    expect(lo).toBeCloseTo(hi, 9);
    expect(unproject(g, { yaw: 0, f: fmin, shift: lo }, 0, -150).py).toBeCloseTo(0, 6);
    expect(unproject(g, { yaw: 0, f: fmin, shift: lo }, 0, 150).py).toBeCloseTo(400, 6);
    const v = clampView(g, { yaw: -30, f: 10, shift: 999 }, { minF: fmin, maxF: fmin * 2 }, 544, 300);
    expect(v.f).toBe(fmin);
    expect(v.shift).toBeCloseTo(lo, 9);
    expect(v.yaw).toBe(330);
  });

  it("clamps the yaw of a partial panorama", () => {
    const part = { ...g, part: 180 };
    const f = focalFromFov(60, 400);
    expect(yawRange(part, f, 400)![0]).toBeCloseTo(30, 6);
    expect(yawRange(part, f, 400)![1]).toBeCloseTo(150, 6);
    expect(yawRange(g, f, 400)).toBeNull();
    expect(clampView(part, { yaw: 170, f, shift: 0 }, { minF: 1, maxF: 1e4 }, 400, 100).yaw).toBeCloseTo(150, 6);
  });

  it("projects hotspot rectangles to view boxes and hides those off-screen", () => {
    const v = { yaw: 90, f: 300, shift: 0 };
    const box = projectRect(g, v, { x: 480, y: 200, w: 40, h: 40 }, 544, 300)!;
    expect(box.x).toBeLessThan(0);
    expect(box.x + box.w).toBeGreaterThan(0);
    expect(box.y).toBeLessThan(0);
    expect(projectRect(g, v, { x: 1400, y: 200, w: 40, h: 40 }, 544, 300)).toBeNull();
  });
});

const definition = {
  slides: {
    slide: {
      standardslide: {
        "@vrslide": "1",
        image: "PANO.swf",
        imagewidth: "544",
        imageheight: "340",
        vrdata: {
          scene: { "@fov": "AUTO", "@initangle": "157", "@maxzoom": "200", "@part": "360", "@tilt": "on", "@auto": "3", "@maxspeed": "50", "@displaywidth": "544", "@displayheight": "340" },
        },
      },
    },
  },
};

describe("panorama parameters", () => {
  it("reads the FlashVR scene of a vrslide", () => {
    const vr = vrSlide(definition)!;
    expect(vr.image).toBe("PANO.swf");
    const p = singleParams(vr);
    expect(p).toMatchObject({ fov: "fit", yaw: 157, maxZoom: 200, part: 360, tilt: true, autoPan: 3, autoStart: true, maxSpeed: 50, viewWidth: 544, viewHeight: 340 });
    const fixed = singleParams({ ...vr, scene: { ...vr.scene, fov: "80", tilt: "off" } });
    expect(fixed.fov).toBe(80);
    expect(fixed.tilt).toBe(false);
    expect(vrSlide({ slides: { slide: [{ standardslide: { "@vrslide": "0", image: "a.jpg" } }] } })).toBeNull();
    expect(vrSlide(undefined)).toBeNull();
  });

  it("finds the image SWF from the resolved refs or the IAF folder", () => {
    expect(slideImageUrl({ file_name: "00aa.iax", edition: "deluxe", refs: [{ element: "image", ref: "pano.swf", url: "/media/disc/x/pano.swf" }] }, "PANO.swf")).toBe("/media/disc/x/pano.swf");
    expect(slideImageUrl({ file_name: "00aa.iax", edition: "kids" }, "PANO.swf")).toBe("/media/disc/kids/dswMedia/IAF/a/00aa/PANO.swf");
  });

  it("opens tour scenes with picture x = 0 at the view centre whatever panangle is", () => {
    for (const panangle of [0, 30, 250]) {
      const pic = { width: 1824, height: 380 };
      const t = tourParams({ panorama: { panangle } }, pic);
      expect(t).toMatchObject({ viewWidth: 624, viewHeight: 289, tilt: true, autoStart: true });
      const geo: PanoGeometry = { ...pic, horizon: 190, part: 360 };
      const { view } = initialView(geo, t, null);
      expect(view.yaw).toBe(0);
      expect(project(geo, view, 0, 190)).toMatchObject({ x: 0, y: 0 });
    }
  });

  it("sets the tour field of view from the picture width and auto-pans one turn in 18.3 s", () => {
    expect(tourHfov(1824)).toBeCloseTo(94, 0);
    expect(tourHfov(1440)).toBeCloseTo(107, 0);
    const pic = { width: 1824, height: 380 };
    const t = tourParams(undefined, pic);
    expect(t.fov).toBeCloseTo(tourHfov(1824), 9);
    const geo: PanoGeometry = { ...pic, horizon: 190, part: 360 };
    // Focal length = cylinder radius, so the view centre is 1:1 with the picture.
    expect(initialView(geo, t, null).view.f).toBeCloseTo(1824 / (2 * Math.PI), 6);
    expect(t.autoPan * 18.3).toBeCloseTo(360, 9);
  });

  it("points the site-map wedge at view yaw minus panangle", () => {
    expect(wedgeAngle(0, 0)).toBe(0);
    expect(wedgeAngle(10.1, 0)).toBeCloseTo(10.1, 9);
    expect(wedgeAngle(0, 250)).toBeCloseTo(110, 9);
    expect(wedgeAngle(0, 30)).toBeCloseTo(-30, 9);
    expect(wedgeAngle(65.3, 0)).toBeCloseTo(65.3, 9);
  });

  it("opens at the given fov, never narrower than the fill limit, zooming to maxzoom", () => {
    const geo: PanoGeometry = { width: 2485, height: 500, horizon: 226, part: 360 };
    const p = singleParams({ ...vrSlide(definition)!, scene: { fov: "80", initangle: "180", maxzoom: "200", displaywidth: "544", displayheight: "380" } });
    const { view, limits } = initialView(geo, p, null);
    expect(view.f).toBeCloseTo(focalFromFov(80, 544), 6);
    expect(view.yaw).toBe(180);
    expect(limits.maxF).toBeCloseTo(view.f * 2, 6);
    expect(limits.minF).toBeCloseTo(minFocal(geo, 380), 6);
    // A short picture cannot be shown 80° wide: the view opens at the fill limit.
    const short: PanoGeometry = { width: 2462, height: 340, horizon: 170, part: 360 };
    const s = initialView(short, { ...p, viewHeight: 340 }, 12);
    expect(s.view.f).toBeCloseTo(minFocal(short, 340), 6);
    expect(s.view.yaw).toBe(12);
  });
});

describe("tour records", () => {
  it("reads quadrant markers", () => {
    expect(quadrantRect({ quadrant: [[750, 92], [740, 92], [750, 116], [740, 116]] })).toEqual({ x: 740, y: 92, w: 10, h: 24 });
    expect(quadrantRect({ quadrant: [[1824, 206], [1824, 206], [1824, 230], [1824, 230]] })).toEqual({ x: 1820, y: 206, w: 4, h: 24 });
    expect(quadrantRect({ quadrant: [[120010, 1], [0, 1], [120010, 25], [0, 25]] })).toBeNull();
    expect(quadrantRect({})).toBeNull();
  });

  it("reads the yaw parameter", () => {
    expect(yawParam("12.5")).toBe(12.5);
    expect(yawParam("")).toBeNull();
    expect(yawParam(null)).toBeNull();
    expect(yawParam("x")).toBeNull();
  });
});

describe("image SWF reader", () => {
  it("reads a JPEG's size", () => {
    expect(jpegSize(new Uint8Array(fakeJpeg(2485, 500)))).toEqual({ width: 2485, height: 500 });
    expect(jpegSize(new Uint8Array([0xff, 0xd8, 0xff, 0xd9]))).toEqual({ width: 0, height: 0 });
  });

  it("extracts the picture and the horizon row from the bitmap fill", async () => {
    const p = await readPanoramaSwf(imageSwf(2485, 500, 226));
    expect(p.width).toBe(2485);
    expect(p.height).toBe(500);
    expect(p.horizon).toBe(226);
    expect(Array.from(p.jpeg.subarray(0, 2))).toEqual([0xff, 0xd8]);
  });

  it("drops the erroneous JPEG header", async () => {
    const p = await readPanoramaSwf(imageSwf(100, 50, 25, true));
    expect(Array.from(p.jpeg.subarray(0, 4))).toEqual([0xff, 0xd8, 0xff, 0xe0]);
  });

  it("reads compressed SWFs and rejects other files", async () => {
    const plain = imageSwf(300, 120, 70);
    const cs = new CompressionStream("deflate");
    const z = new Uint8Array(await new Response(new Blob([plain.subarray(8)]).stream().pipeThrough(cs)).arrayBuffer());
    const cws = new Uint8Array(8 + z.length);
    cws.set([0x43, 0x57, 0x53, 6, ...plain.subarray(4, 8)]);
    cws.set(z, 8);
    expect((await readPanoramaSwf(cws)).horizon).toBe(70);
    await expect(readPanoramaSwf(new Uint8Array([1, 2, 3, 4, 5, 6, 7, 8, 9]))).rejects.toThrow(/not a SWF/);
  });
});
