import { useState } from "react";
import { Link } from "react-router-dom";
import { NewFolderDialog } from "../app/AddFavoriteDialog";
import { favorites, useFavorites, useFolders, type Favorite } from "../app/favorites";
import { useDocumentTitle } from "../components/useDocumentTitle";

/** Favorites › Organize Favorites: folders (new, rename, delete), and for each page rename, reorder, move, remove. */
export function FavoritesPage() {
  useDocumentTitle("Organize Favorites");
  const list = useFavorites();
  const folders = useFolders();
  const [editing, setEditing] = useState<string | null>(null);
  const [draft, setDraft] = useState("");
  const [folderEdit, setFolderEdit] = useState<string | null>(null);
  const [folderDraft, setFolderDraft] = useState("");
  const [creating, setCreating] = useState<string | null>(null);
  const taken = (n: string, except?: string) =>
    n.trim() !== "" && folders.some((f) => f.toLowerCase() === n.trim().toLowerCase() && f !== except);

  const row = (f: Favorite, i: number, group: Favorite[]) => (
    <li key={f.path}>
      {editing === f.path ? (
        <form
          className="favs-edit"
          onSubmit={(e) => {
            e.preventDefault();
            favorites.rename(f.path, draft);
            setEditing(null);
          }}
        >
          <label>
            <span className="sr-only">Name</span>
            <input autoFocus value={draft} onChange={(e) => setDraft(e.target.value)} onKeyDown={(e) => e.key === "Escape" && setEditing(null)} />
          </label>
          <button type="submit" className="enc-button">
            Save
          </button>
        </form>
      ) : (
        <Link to={f.path} className="favs-title">
          {f.title}
        </Link>
      )}
      <span className="favs-tools">
        <button type="button" onClick={() => (setEditing(f.path), setDraft(f.title))} aria-label={`Rename ${f.title}`}>
          Rename
        </button>
        <button type="button" disabled={i === 0} onClick={() => favorites.move(f.path, -1)} aria-label={`Move ${f.title} up`}>
          Up
        </button>
        <button type="button" disabled={i === group.length - 1} onClick={() => favorites.move(f.path, 1)} aria-label={`Move ${f.title} down`}>
          Down
        </button>
        {folders.length > 0 && (
          <select
            aria-label={`Folder of ${f.title}`}
            value={f.folder ?? ""}
            onChange={(e) => favorites.moveToFolder(f.path, e.target.value || undefined)}
          >
            <option value="">Favorites</option>
            {folders.map((n) => (
              <option key={n} value={n}>
                {n}
              </option>
            ))}
          </select>
        )}
        <button type="button" onClick={() => favorites.remove(f.path)} aria-label={`Remove ${f.title}`}>
          Remove
        </button>
      </span>
    </li>
  );

  const top = list.filter((f) => !f.folder);
  return (
    <div className="favs">
      <h1 className="page-title">Organize Favorites</h1>
      <p className="enc-muted">
        Favorites are kept in this browser. Add the page you are reading with Favorites › Add to Favorites.
      </p>
      <p className="favs-new">
        <button type="button" className="enc-button" onClick={() => setCreating("")}>
          New Folder…
        </button>
      </p>
      {list.length === 0 && folders.length === 0 && <p>You have no favorites yet.</p>}
      {folders.map((name) => {
        const inside = list.filter((f) => f.folder === name);
        return (
          <section key={name} aria-label={`Folder ${name}`}>
            <div className="favs-folder">
              {folderEdit === name ? (
                <form
                  className="favs-edit"
                  onSubmit={(e) => {
                    e.preventDefault();
                    if (!taken(folderDraft, name) && favorites.renameFolder(name, folderDraft)) setFolderEdit(null);
                  }}
                >
                  <label>
                    <span className="sr-only">Folder name</span>
                    <input autoFocus value={folderDraft} onChange={(e) => setFolderDraft(e.target.value)} onKeyDown={(e) => e.key === "Escape" && setFolderEdit(null)} />
                  </label>
                  <button type="submit" className="enc-button" disabled={taken(folderDraft, name)}>
                    Save
                  </button>
                </form>
              ) : (
                <h2>{name}</h2>
              )}
              <span className="favs-tools">
                <button type="button" onClick={() => (setFolderEdit(name), setFolderDraft(name))} aria-label={`Rename folder ${name}`}>
                  Rename
                </button>
                <button
                  type="button"
                  aria-label={`Delete folder ${name}`}
                  onClick={() => {
                    if (inside.length === 0 || window.confirm(`Delete the folder "${name}" and the ${inside.length} favorite${inside.length === 1 ? "" : "s"} in it?`)) favorites.removeFolder(name);
                  }}
                >
                  Delete
                </button>
              </span>
            </div>
            {inside.length === 0 ? <p className="enc-muted">This folder is empty.</p> : <ol className="favs-list">{inside.map((f, i) => row(f, i, inside))}</ol>}
          </section>
        );
      })}
      {top.length > 0 && <ol className="favs-list">{top.map((f, i) => row(f, i, top))}</ol>}
      {creating !== null && (
        <NewFolderDialog
          value={creating}
          onChange={setCreating}
          taken={taken(creating)}
          onCreate={() => {
            favorites.addFolder(creating);
            setCreating(null);
          }}
          onCancel={() => setCreating(null)}
        />
      )}
    </div>
  );
}
