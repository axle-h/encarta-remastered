import { useState } from "react";
import { Link } from "react-router-dom";
import { api } from "../api/client";
import type { TreeNode } from "../api/types";
import { useApi } from "../api/useApi";
import { Inlines } from "../content/Inlines";
import { itemHref } from "../content/links";
import "./IndexTerm.css";

/**
 * An index term's entries (DATAF, content-schema §4) under its search result. Encarta 2009 has no
 * index browser: a term is a search result that expands to the places it occurs.
 */
export function IndexTermEntries({ id, kids }: { id: number; kids: boolean }) {
  const { data, error, loading } = useApi((s) => api.indexTerm(id, kids ? "kids" : undefined, s), [id, kids]);
  if (loading) return <p className="index-term-status">Loading…</p>;
  if (error || !data) return <p className="index-term-status">The entries could not be loaded.</p>;
  if (data.entries.length === 0) return <p className="index-term-status">No entries.</p>;
  return <EntryList nodes={data.entries} kids={kids} />;
}

function EntryList({ nodes, kids }: { nodes: TreeNode[]; kids: boolean }) {
  return (
    <ul className="index-term-entries">
      {nodes.map((n, i) => (
        <li key={`${n.ref ?? "h"}-${i}`}>
          {n.ref ? (
            <Link to={itemHref(n.ref, n.para, kids)}>
              <Inlines c={n.title} opts={{ kids }} />
            </Link>
          ) : (
            <span className="index-term-sub">
              <Inlines c={n.title} opts={{ kids }} />
            </span>
          )}
          {n.children && n.children.length > 0 && <EntryList nodes={n.children} kids={kids} />}
        </li>
      ))}
    </ul>
  );
}

/** The expand/collapse control of an index-term search result. */
export function useExpanded(): [boolean, () => void] {
  const [open, setOpen] = useState(false);
  return [open, () => setOpen((o) => !o)];
}
