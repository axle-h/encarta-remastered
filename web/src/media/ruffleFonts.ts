/**
 * Device fonts for Ruffle. The IAF players and their controls draw text in device fonts
 * ("Trebuchet MS" in 4,229 `.iax` text styles, Tahoma 2,757, Georgia 1,228, Arial 1,179, Verdana 351,
 * Times New Roman), which Ruffle does not have: it substitutes its own wider Noto Sans, so single-line
 * fields clip ("…a harmonic struct" in Counterpoint). The server lists the disc's `AREF/FONTS` files
 * it has (`/api/meta` `fonts`); each is handed to Ruffle under the family name the players ask for.
 *
 * Ruffle registers a TrueType file under its *full* name, so "Tahoma Bold" would never match a bold
 * "Tahoma" request, and the disc's Verdana and Georgia are only present as their Reference variants.
 * So each face is wrapped in a minimal SWF holding one `DefineFont4` tag (SWF 10: font name, bold and
 * italic flags, the font file as data), which Ruffle registers under the tag's name and flags.
 */

interface Face {
  names: string[];
  bold: boolean;
  italic: boolean;
}

const R = { bold: false, italic: false };
const B = { bold: true, italic: false };
const I = { bold: false, italic: true };
const BI = { bold: true, italic: true };

/**
 * Disc font file (lower case) → the device font names it answers. Trebuchet MS is not on the disc;
 * Tahoma, its closest sibling there, sets about as wide (Counterpoint's caption fits, as it must
 * have in Trebuchet). Tahoma has no italic and Ruffle does not slant fonts, so Arial Italic, as
 * wide, stands in. MS Reference Sans Serif and Serif have Verdana's and Georgia's metrics.
 */
export const DEVICE_FACES: Record<string, Face> = {
  "tahoma.ttf": { names: ["Tahoma", "Trebuchet MS"], ...R },
  "tahomabd.ttf": { names: ["Tahoma", "Trebuchet MS"], ...B },
  "refsan.ttf": { names: ["Verdana"], ...R },
  "refsanb.ttf": { names: ["Verdana"], ...B },
  "refsani.ttf": { names: ["Verdana"], ...I },
  "refsanbi.ttf": { names: ["Verdana"], ...BI },
  "refser.ttf": { names: ["Georgia"], ...R },
  "refserb.ttf": { names: ["Georgia"], ...B },
  "refseri.ttf": { names: ["Georgia"], ...I },
  "refserbi.ttf": { names: ["Georgia"], ...BI },
  "times.ttf": { names: ["Times New Roman"], ...R },
  "timesbd.ttf": { names: ["Times New Roman"], ...B },
  "timesi.ttf": { names: ["Times New Roman"], ...I },
  "timesbi.ttf": { names: ["Times New Roman"], ...BI },
  "arial.ttf": { names: ["Arial"], ...R },
  "arialbd.ttf": { names: ["Arial"], ...B },
  "ariali.ttf": { names: ["Arial", "Tahoma", "Trebuchet MS"], ...I },
};

/** Flash's own generic fonts as Windows resolves them (`_sans`, `_serif`), and Ruffle's fallback. */
export const DEFAULT_FONTS = { sans: ["Arial", "Tahoma"], serif: ["Times New Roman", "Georgia"] };

/**
 * An uncompressed SWF 10 whose only tag is `DefineFont4` (code 91) carrying `font` under `name`.
 * Layout (SWF 10 specification): "FWS", version, file length (u32 LE), an empty RECT (one byte),
 * frame rate 8.8 and frame count (u16 LE each); then the tag in long form (u16 code << 6 | 0x3f,
 * u32 length): font id u16, flags u8 (0x04 has data, 0x02 italic, 0x01 bold), name (UTF-8, NUL),
 * the font bytes; then the End tag.
 */
export function fontSwf(font: Uint8Array, name: string, bold: boolean, italic: boolean): Uint8Array {
  const nameBytes = new TextEncoder().encode(name);
  const body = 2 + 1 + nameBytes.length + 1 + font.length;
  const total = 8 + 1 + 4 + 6 + body + 2;
  const out = new Uint8Array(total);
  const dv = new DataView(out.buffer);
  out.set([0x46, 0x57, 0x53, 10]);
  dv.setUint32(4, total, true);
  let p = 8;
  out[p++] = 0; // RECT with 0-bit fields
  dv.setUint16(p, 24 << 8, true); // 24 fps
  dv.setUint16(p + 2, 1, true); // one frame
  p += 4;
  dv.setUint16(p, (91 << 6) | 0x3f, true);
  dv.setUint32(p + 2, body, true);
  p += 6;
  dv.setUint16(p, 1, true);
  out[p + 2] = 0x04 | (italic ? 0x02 : 0) | (bold ? 0x01 : 0);
  p += 3;
  out.set(nameBytes, p);
  p += nameBytes.length + 1; // NUL already zero
  out.set(font, p);
  p += font.length;
  dv.setUint16(p, 0, true); // End
  return out;
}

/** The `/fonts/…` URLs among `urls` that Ruffle gets, with their faces. */
export function deviceFontFiles(urls: string[]): { url: string; face: Face }[] {
  return urls.flatMap((url) => {
    const face = DEVICE_FACES[(url.split("/").pop() ?? "").toLowerCase()];
    return face ? [{ url, face }] : [];
  });
}

let sources: Promise<string[]> | null = null;

/**
 * Blob URLs of the wrapped device fonts, for Ruffle's `fontSources` (fetched once per page; empty
 * when the server has none, and Ruffle keeps its own fonts).
 */
export function ruffleFontSources(): Promise<string[]> {
  sources ??= (async () => {
    try {
      const res = await fetch("/api/meta", { headers: { Accept: "application/json" } });
      if (!res.ok) return [];
      const meta = (await res.json()) as { fonts?: string[] };
      const files = deviceFontFiles(meta.fonts ?? []);
      const loaded = await Promise.all(
        files.map(async ({ url, face }) => {
          try {
            const r = await fetch(url);
            if (!r.ok) return [];
            const bytes = new Uint8Array(await r.arrayBuffer());
            return face.names.map((n) =>
              URL.createObjectURL(new Blob([fontSwf(bytes, n, face.bold, face.italic)], { type: "application/x-shockwave-flash" })),
            );
          } catch {
            return [];
          }
        }),
      );
      return loaded.flat();
    } catch {
      return [];
    }
  })();
  return sources;
}
