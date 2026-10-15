import { useEffect, useRef, useState } from "react";

/** The name Add to Favorites suggests (VM: "Article - <title>" for articles, the plain title otherwise). */
export function defaultFavoriteName(title: string, isArticle: boolean): string {
  const t = title.trim();
  return isArticle && t ? `Article - ${t}` : t;
}

/**
 * Favorites › Add to Favorites… (adult and Kids alike in the VM): a light blue dialog with a
 * "favorites" banner, a Name box with the suggested name selected, and OK / Cancel. Encarta also
 * offered a folder tree and New Folder; favorites here have no folders.
 */
export function AddFavoriteDialog({ name, onAdd, onCancel }: { name: string; onAdd: (name: string) => void; onCancel: () => void }) {
  const [value, setValue] = useState(name);
  const box = useRef<HTMLInputElement>(null);
  useEffect(() => {
    box.current?.select();
    const key = (e: KeyboardEvent) => {
      if (e.key === "Escape") onCancel();
    };
    document.addEventListener("keydown", key);
    return () => document.removeEventListener("keydown", key);
  }, [onCancel]);
  return (
    <div className="dlg-backdrop">
      <form
        className="dlg fav-dlg"
        role="dialog"
        aria-modal="true"
        aria-label="Add to Favorites"
        onSubmit={(e) => {
          e.preventDefault();
          if (value.trim()) onAdd(value.trim());
        }}
      >
        <div className="fav-banner" aria-hidden="true">
          favorites
        </div>
        <label className="fav-label">
          Name:
          <input ref={box} type="text" value={value} onChange={(e) => setValue(e.target.value)} autoFocus />
        </label>
        <div className="dlg-buttons">
          <button type="submit" disabled={!value.trim()}>
            OK
          </button>
          <button type="button" onClick={onCancel}>
            Cancel
          </button>
        </div>
      </form>
    </div>
  );
}
