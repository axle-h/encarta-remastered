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
};

/** `@font-face` rules for the known font URLs among `urls`. */
export function fontFaceCss(urls: string[]): string {
  const rules: string[] = [];
  for (const url of urls) {
    const f = FACES[(url.split("/").pop() ?? "").toLowerCase()];
    if (!f) continue;
    rules.push(
      `@font-face{font-family:"${f.family}";src:local("${f.family}"),url("${url}") format("truetype");font-weight:${f.weight};font-style:${f.style};font-display:swap}`,
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
