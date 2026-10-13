import { useState } from "react";
import { Link } from "react-router-dom";
import { api } from "../api/client";
import type { AssocItem, DataTable, TreeNode } from "../api/types";
import { useApi } from "../api/useApi";
import { Inlines } from "../content/Inlines";
import { itemHref } from "../content/links";
import type { Inline } from "../content/model";
import { useMediaViewer } from "../media/MediaViewer";
import { fileUrl, galleryImage } from "../media/pick";

function Panel({ title, children, more, tone = "orange" }: { title: string; children: React.ReactNode; more?: React.ReactNode; tone?: "orange" | "gray" }) {
  return (
    <section className={`enc-panel enc-panel-${tone}`}>
      <h2 className="enc-panel-title">{title}</h2>
      <div className="enc-panel-body">{children}</div>
      {more && <div className="enc-panel-more">{more}</div>}
    </section>
  );
}

interface FactEntry {
  name: Inline[];
  value: Inline[];
  quick?: boolean;
  entries?: FactEntry[];
}

/**
 * Entries for the Quick Facts box: a facts table's entries, or for a Facts and Figures table
 * (sections of entries) the entries marked `quick`, each followed by its continuation values.
 */
export function quickEntries(json: unknown): FactEntry[] {
  const j = json as { entries?: FactEntry[]; sections?: { entries?: FactEntry[] }[] } | undefined;
  if (j?.entries) return j.entries;
  const out: FactEntry[] = [];
  for (const sec of j?.sections ?? []) {
    for (const e of sec.entries ?? []) {
      if (!e.quick) continue;
      out.push(e);
      for (const sub of e.entries ?? []) out.push({ name: [], value: sub.value });
    }
  }
  return out;
}

/** Quick Facts: the first entries of a facts table, "More" opens the whole table. */
export function QuickFacts({ table, sequence, kids }: { table: DataTable & { ref_id?: number }; sequence?: AssocItem[]; kids: boolean }) {
  const viewer = useMediaViewer();
  const entries = quickEntries(table.json);
  // Group continuation rows (empty name) with their entry, then show the first five entries.
  const groups: FactEntry[][] = [];
  for (const e of entries) {
    if (e.name.length === 0 && groups.length > 0) groups[groups.length - 1]!.push(e);
    else groups.push([e]);
  }
  const shown = groups.slice(0, 5);
  return (
    <Panel
      title="Quick Facts"
      more={
        table.ref_id !== undefined && groups.length > 0 ? (
          <button type="button" className="enc-panel-link" onClick={() => viewer.open(table.ref_id!, sequence)}>
            More
          </button>
        ) : undefined
      }
    >
      {groups.length === 0 ? (
        <div className="enc-qf-html" dangerouslySetInnerHTML={{ __html: table.html }} />
      ) : (
        <dl className="enc-qf">
          {shown.map((g, i) => (
            <div key={i}>
              <dt>
                <Inlines c={g[0]!.name} opts={{ kids }} />
              </dt>
              {g.map((e, j) => (
                <dd key={j}>
                  <Inlines c={e.value} opts={{ kids }} />
                </dd>
              ))}
            </div>
          ))}
        </dl>
      )}
    </Panel>
  );
}

export function AtlasPanel({ map }: { map: AssocItem }) {
  const item = useApi((s) => api.item(map.ref_id, s), [map.ref_id]);
  const pic = galleryImage(item.data);
  const to = item.data?.geocode ? `/atlas/place/${item.data.geocode}` : "/atlas";
  return (
    <Panel title="World Atlas" tone="gray" more={<Link to={to} className="enc-panel-link">Full Map</Link>}>
      <Link to={to} className="enc-atlas-thumb">
        {pic ? <img src={fileUrl(pic)} alt={map.title ?? "Map"} width={pic.width} height={pic.height} /> : <span>{map.title}</span>}
      </Link>
    </Panel>
  );
}

function TreeList({ nodes, kids, depth }: { nodes: TreeNode[]; kids: boolean; depth: number }) {
  return (
    <ul className={`enc-tree enc-tree-${depth}`}>
      {nodes.map((n, i) => (
        <TreeItem key={`${n.node ?? n.ref ?? i}-${i}`} n={n} kids={kids} depth={depth} />
      ))}
    </ul>
  );
}

function TreeItem({ n, kids, depth }: { n: TreeNode; kids: boolean; depth: number }) {
  const [open, setOpen] = useState(false);
  const title = <Inlines c={n.title} opts={{ kids }} />;
  const hasKids = (n.children?.length ?? 0) > 0;
  return (
    <li>
      {n.ref ? (
        <Link to={itemHref(n.ref, n.para, kids)}>{title}</Link>
      ) : hasKids ? (
        <button type="button" className="enc-tree-toggle" aria-expanded={open} onClick={() => setOpen(!open)}>
          {title}
        </button>
      ) : (
        <span>{title}</span>
      )}
      {hasKids && (open || n.ref !== undefined || depth > 0) && <TreeList nodes={n.children!} kids={kids} depth={depth + 1} />}
    </li>
  );
}

/** Related Articles (the REL tree, content-schema §4): top-level nodes, headings expand. */
export function RelatedPanel({ tree, kids }: { tree: TreeNode[]; kids: boolean }) {
  const [all, setAll] = useState(false);
  const sorted = [...tree].sort((a, b) => Number(!!b.sort_top) - Number(!!a.sort_top));
  const shown = all ? sorted : sorted.slice(0, 8);
  return (
    <Panel
      title="Related Articles"
      more={
        sorted.length > shown.length || all ? (
          <button type="button" className="enc-panel-link" onClick={() => setAll(!all)}>
            {all ? "Fewer" : "More"}
          </button>
        ) : undefined
      }
    >
      <ul className="enc-related">
        {shown.map((n, i) => (
          <RelatedNode key={`${n.node ?? n.ref ?? i}`} n={n} kids={kids} />
        ))}
      </ul>
    </Panel>
  );
}

function RelatedNode({ n, kids }: { n: TreeNode; kids: boolean }) {
  const [open, setOpen] = useState(false);
  const title = <Inlines c={n.title} opts={{ kids }} />;
  const children = n.children ?? [];
  // A heading with exactly one "main article" child is a link to that article.
  return (
    <li>
      {n.ref ? (
        <Link to={itemHref(n.ref, n.para, kids)}>{title}</Link>
      ) : (
        <button type="button" className="enc-tree-toggle" aria-expanded={open} onClick={() => setOpen(!open)}>
          {title}
        </button>
      )}
      {open && children.length > 0 && <TreeList nodes={children} kids={kids} depth={1} />}
    </li>
  );
}

export function LinksPanel({ title, items, kids }: { title: string; items: AssocItem[]; kids: boolean }) {
  return (
    <Panel title={title} tone="gray">
      <ul className="enc-related">
        {items.map((m) => (
          <li key={m.ref_id}>
            <Link to={itemHref(m.ref_id, null, kids)}>{m.title ?? `Item ${m.ref_id}`}</Link>
          </li>
        ))}
      </ul>
    </Panel>
  );
}
