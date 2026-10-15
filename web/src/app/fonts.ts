/**
 * The disc's MS Reference fonts, when the server has them (`/api/meta` → `fonts`, served from
 * `out/fonts/` at `/fonts/{file}`). The CSS names "MS Reference Serif" / "MS Reference Sans Serif"
 * first in its stacks, so locally installed copies are used anyway; this adds the disc's files.
 */

const FACES: Record<string, { family: string; weight: number; style: "normal" | "italic" }> = {
  "refser.ttf": { family: "MS Reference Serif", weight: 400, style: "normal" },
  "refserb.ttf": { family: "MS Reference Serif", weight: 700, style: "normal" },
  "refseri.ttf": { family: "MS Reference Serif", weight: 400, style: "italic" },
  "refserbi.ttf": { family: "MS Reference Serif", weight: 700, style: "italic" },
  "refsan.ttf": { family: "MS Reference Sans Serif", weight: 400, style: "normal" },
  "refsanb.ttf": { family: "MS Reference Sans Serif", weight: 700, style: "normal" },
  "refsani.ttf": { family: "MS Reference Sans Serif", weight: 400, style: "italic" },
  "refsanbi.ttf": { family: "MS Reference Sans Serif", weight: 700, style: "italic" },
  // Under its own name, so only the Kids chrome (its menus, drawn in Tahoma 8 pt) switches to it.
  "tahoma.ttf": { family: "Encarta Tahoma", weight: 400, style: "normal" },
  "tahomabd.ttf": { family: "Encarta Tahoma", weight: 700, style: "normal" },
};

/**
 * `@font-face` rules for the known fonts among `urls`. A face with a WOFF2 copy lists it first
 * (smaller); the TTF stays as the fallback.
 */
export function fontFaceCss(urls: string[]): string {
  const sources = new Map<string, { woff2?: string; ttf?: string }>();
  for (const url of urls) {
    const file = (url.split("/").pop() ?? "").toLowerCase();
    const m = /^(.+)\.(ttf|woff2)$/.exec(file);
    if (!m || !m[1] || !m[2] || !FACES[`${m[1]}.ttf`]) continue;
    const s = sources.get(m[1]) ?? {};
    s[m[2] as "ttf" | "woff2"] = url;
    sources.set(m[1], s);
  }
  const rules: string[] = [];
  for (const [stem, s] of sources) {
    const f = FACES[`${stem}.ttf`];
    if (!f) continue;
    const src = [`local("${f.family === "Encarta Tahoma" ? "Tahoma" : f.family}")`];
    if (s.woff2) src.push(`url("${s.woff2}") format("woff2")`);
    if (s.ttf) src.push(`url("${s.ttf}") format("truetype")`);
    rules.push(
      `@font-face{font-family:"${f.family}";src:${src.join(",")};font-weight:${f.weight};font-style:${f.style};font-display:swap}`,
    );
  }
  return rules.join("\n");
}

export async function loadDiscFonts(): Promise<void> {
  try {
    const res = await fetch("/api/meta", { headers: { Accept: "application/json" } });
    if (!res.ok) return;
    const meta = (await res.json()) as { fonts?: string[] };
    const css = fontFaceCss(meta.fonts ?? []);
    if (!css) return;
    const style = document.createElement("style");
    style.dataset.discFonts = "";
    style.textContent = css;
    document.head.appendChild(style);
  } catch {
    /* no server metadata: keep the fallback fonts */
  }
}
