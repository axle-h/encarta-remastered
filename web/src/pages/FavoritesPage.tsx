import { useState } from "react";
import { Link } from "react-router-dom";
import { favorites, useFavorites } from "../app/favorites";
import { useDocumentTitle } from "../components/useDocumentTitle";

/** Favorites › Organize Favorites: rename, reorder and remove the saved pages. */
export function FavoritesPage() {
  useDocumentTitle("Organize Favorites");
  const list = useFavorites();
  const [editing, setEditing] = useState<string | null>(null);
  const [draft, setDraft] = useState("");
  return (
    <div className="favs">
      <h1 className="page-title">Organize Favorites</h1>
      <p className="enc-muted">
        Favorites are kept in this browser. Add the page you are reading with Favorites › Add to Favorites.
      </p>
      {list.length === 0 ? (
        <p>You have no favorites yet.</p>
      ) : (
        <ol className="favs-list">
          {list.map((f, i) => (
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
                <button type="button" disabled={i === list.length - 1} onClick={() => favorites.move(f.path, 1)} aria-label={`Move ${f.title} down`}>
                  Down
                </button>
                <button type="button" onClick={() => favorites.remove(f.path)} aria-label={`Remove ${f.title}`}>
                  Remove
                </button>
              </span>
            </li>
          ))}
        </ol>
      )}
    </div>
  );
}
