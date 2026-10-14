import { describe, expect, it } from "vitest";
import { DEVICE_FACES, deviceFontFiles, fontSwf } from "./ruffleFonts";

describe("Ruffle device fonts", () => {
  it("wraps a font file in a SWF 10 DefineFont4 tag", () => {
    const font = new Uint8Array([0x00, 0x01, 0x00, 0x00, 0xaa, 0xbb]);
    const swf = fontSwf(font, "Tahoma", true, false);
    const dv = new DataView(swf.buffer);
    expect(new TextDecoder().decode(swf.slice(0, 3))).toBe("FWS");
    expect(swf[3]).toBe(10);
    expect(dv.getUint32(4, true)).toBe(swf.length);
    expect(swf[8]).toBe(0); // empty RECT
    expect(dv.getUint16(11, true)).toBe(1); // frames
    const head = dv.getUint16(13, true);
    expect(head >> 6).toBe(91);
    expect(head & 0x3f).toBe(0x3f);
    const len = dv.getUint32(15, true);
    expect(len).toBe(2 + 1 + "Tahoma".length + 1 + font.length);
    expect(dv.getUint16(19, true)).toBe(1); // font id
    expect(swf[21]).toBe(0x05); // has data | bold
    expect(new TextDecoder().decode(swf.slice(22, 28))).toBe("Tahoma");
    expect(swf[28]).toBe(0);
    expect([...swf.slice(29, 29 + font.length)]).toEqual([...font]);
    expect(19 + len).toBe(swf.length - 2);
    expect(dv.getUint16(swf.length - 2, true)).toBe(0); // End
    expect(fontSwf(font, "Georgia", false, true)[21]).toBe(0x06);
  });

  it("picks the disc fonts the players name", () => {
    const urls = ["/fonts/REFSAN.TTF", "/fonts/TAHOMABD.TTF", "/fonts/ARIALN.TTF", "/fonts/TIMESBI.TTF"];
    expect(deviceFontFiles(urls).map((f) => [f.url, f.face.names, f.face.bold, f.face.italic])).toEqual([
      ["/fonts/REFSAN.TTF", ["Verdana"], false, false],
      ["/fonts/TAHOMABD.TTF", ["Tahoma", "Trebuchet MS"], true, false],
      ["/fonts/TIMESBI.TTF", ["Times New Roman"], true, true],
    ]);
    expect(Object.keys(DEVICE_FACES)).toHaveLength(17);
  });
});
