import { Fragment, useState } from "react";
import { Link } from "react-router-dom";
import { api } from "../api/client";
import type { AssocItem, DataTable, TreeNode } from "../api/types";
import { useApi } from "../api/useApi";
import { Inlines } from "../content/Inlines";
import { itemHref } from "../content/links";
import { useMediaViewer } from "../media/MediaViewer";
import { fileUrl, galleryImage } from "../media/pick";
import { Html } from "../components/Html";
import { archiveLabel, MODULE_ROWS, quickFactGroups, tourModuleTitle } from "./modules";

function Panel({
  title,
  count,
  children,
  more,
  tone = "orange",
}: {
  title: string;
  count?: number;
  children: React.ReactNode;
  more?: React.ReactNode;
  tone?: "orange" | "light" | "gray";
}) {
  return (
    <section className={`enc-panel enc-panel-${tone}`}>
      <h2 className="enc-panel-title">
        {title}
        {count !== undefined && <span className="enc-panel-count"> ({count})</span>}
      </h2>
      <div className="enc-panel-body">{children}</div>
      {more && <div className="enc-panel-more">{more}</div>}
    </section>
  );
}

/**
 * Quick Facts (VM): the table's first five entries, the fifth cut after its first value
 * (`quickFactGroups`); "More" opens the whole table.
 */
export function QuickFacts({ table, sequence, kids }: { table: DataTable & { ref_id?: number }; sequence?: AssocItem[]; kids: boolean }) {
  const viewer = useMediaViewer();
  const groups = quickFactGroups(table.json);
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
          {groups.map((g, i) => (
            <div key={i}>
              {g.map((r, j) => (
                <Fragment key={j}>
                  {r.name && (
                    <dt>
                      <Inlines c={r.name} opts={{ kids }} />
                    </dt>
                  )}
                  {r.value && (
                    <dd>
                      <Inlines c={r.value} opts={{ kids }} />
                    </dd>
                  )}
                </Fragment>
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

/**
 * Related Articles (the Premium `DLX` REL tree, content-schema §4): the first five top-level nodes
 * in stored order, headings included (VM); "More" shows the rest, headings expand.
 */
export function RelatedPanel({ tree, kids }: { tree: TreeNode[]; kids: boolean }) {
  const [all, setAll] = useState(false);
  const sorted = [...tree].sort((a, b) => Number(!!b.sort_top) - Number(!!a.sort_top));
  const shown = all ? sorted : sorted.slice(0, MODULE_ROWS.related);
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

function ShowAll({ total, rows, open, toggle }: { total: number; rows: number; open: boolean; toggle: () => void }) {
  if (total <= rows) return null;
  return (
    <button type="button" className="enc-panel-link" aria-expanded={open} onClick={toggle}>
      {open ? "Collapse" : "Show All"}
    </button>
  );
}

/**
 * A module listing items (Primary Sources, Sidebars, Further Reading, Archives): `rows` shown,
 * "Show All" for the rest (VM: the list grows in place), with the count in the title.
 */
export function ListModule({
  title,
  items,
  kids,
  rows = MODULE_ROWS.list,
  tone = "light",
  label = (m) => (m.title_html ? <Html as="span" html={m.title_html} /> : (m.title ?? `Item ${m.ref_id}`)),
}: {
  title: string;
  items: AssocItem[];
  kids: boolean;
  rows?: number;
  tone?: "orange" | "light" | "gray";
  label?: (m: AssocItem) => React.ReactNode;
}) {
  const [all, setAll] = useState(false);
  const shown = all ? items : items.slice(0, rows);
  return (
    <Panel title={title} count={items.length} tone={tone} more={<ShowAll total={items.length} rows={rows} open={all} toggle={() => setAll((v) => !v)} />}>
      <ul className="enc-related">
        {shown.map((m) => (
          <li key={m.ref_id}>
            <Link to={itemHref(m.ref_id, null, kids)}>{label(m)}</Link>
          </li>
        ))}
      </ul>
    </Panel>
  );
}

/** Web Sites (VM): title, description and address of each link, sorted by title, three shown. */
export function WebSitesModule({ items, kids }: { items: AssocItem[]; kids: boolean }) {
  const [all, setAll] = useState(false);
  const rows = MODULE_ROWS.list;
  const shown = all ? items : items.slice(0, rows);
  return (
    <Panel title="Web Sites" count={items.length} more={<ShowAll total={items.length} rows={rows} open={all} toggle={() => setAll((v) => !v)} />}>
      <ul className="enc-weblinks">
        {shown.map((m) => (
          <li key={m.ref_id}>
            <Link to={itemHref(m.ref_id, null, kids)} className="enc-weblink-title">
              {m.title ?? `Item ${m.ref_id}`}
            </Link>
            {m.caption_html && <Html as="div" className="enc-weblink-text" html={m.caption_html} />}
            {m.url && (
              <a className="enc-weblink-url" href={m.url} target="_blank" rel="noopener noreferrer">
                {m.url}
              </a>
            )}
          </li>
        ))}
      </ul>
    </Panel>
  );
}

/** Quotations (VM): the count, the first quotation (DATA order) with the article's title as credit. */
export function QuotationsModule({ items, credit, kids }: { items: AssocItem[]; credit: string; kids: boolean }) {
  const [all, setAll] = useState(false);
  const first = items[0]!;
  return (
    <Panel
      title="Quotations"
      count={items.length}
      tone="gray"
      more={
        <button type="button" className="enc-panel-link" aria-expanded={all} onClick={() => setAll((v) => !v)}>
          {all ? "Collapse" : "Show All"}
        </button>
      }
    >
      {all ? (
        <ul className="enc-related">
          {items.map((m) => (
            <li key={m.ref_id}>
              <Link to={itemHref(m.ref_id, null, kids)}>{m.title ?? `Item ${m.ref_id}`}</Link>
            </li>
          ))}
        </ul>
      ) : (
        <Link to={itemHref(first.ref_id, null, kids)} className="enc-quote-module">
          <span className="enc-quote-text">{first.title}</span>
          <span className="enc-quote-credit">{credit}</span>
        </Link>
      )}
    </Panel>
  );
}

/** A virtual tour module (VM): "Virtual Tour" or "3D Ancient Site", the tour's banner and "Take the Tour". */
export function TourModule({ tour, kids }: { tour: AssocItem; kids: boolean }) {
  const to = itemHref(tour.ref_id, null, kids);
  return (
    <Panel title={tourModuleTitle(tour)} tone="gray" more={<Link to={to} className="enc-panel-link">Take the Tour</Link>}>
      <Link to={to} className="enc-tour-module">
        <span className="enc-tour-title">{tour.title}</span>
        {tour.picon && <img src={fileUrl(tour.picon)} alt="" width={tour.picon.width ?? undefined} height={tour.picon.height ?? undefined} />}
      </Link>
    </Panel>
  );
}

/** Archives (VM): the yearbook articles by year, "1938: France", five shown. */
export function ArchivesModule({ items, kids }: { items: AssocItem[]; kids: boolean }) {
  return <ListModule title="Archives" items={items} kids={kids} rows={MODULE_ROWS.archives} label={archiveLabel} />;
}
