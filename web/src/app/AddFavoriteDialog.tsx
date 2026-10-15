import { useEffect, useRef, useState } from "react";
import { favorites, useFolders } from "./favorites";

/** The name Add to Favorites suggests (VM: "Article - <title>" for articles, the plain title otherwise). */
export function defaultFavoriteName(title: string, isArticle: boolean): string {
  const t = title.trim();
  return isArticle && t ? `Article - ${t}` : t;
}

/**
 * Favorites › Add to Favorites… (adult and Kids alike in the VM): a light blue dialog with a
 * "favorites" banner, a Name box with the suggested name selected, and OK / Cancel. a "Create in:" folder
 * list ("Favorites" is the top level) and New Folder, which asks for a name in "Create New Folder".
 */
export function AddFavoriteDialog({ name, onAdd, onCancel }: { name: string; onAdd: (name: string, folder?: string) => void; onCancel: () => void }) {
  const [value, setValue] = useState(name);
  const [folder, setFolder] = useState("");
  const [creating, setCreating] = useState<string | null>(null);
  const folders = useFolders();
  const box = useRef<HTMLInputElement>(null);
  useEffect(() => {
    box.current?.select();
    const key = (e: KeyboardEvent) => {
      if (e.key === "Escape" && creating === null) onCancel();
    };
    document.addEventListener("keydown", key);
    return () => document.removeEventListener("keydown", key);
  }, [onCancel, creating]);
  return (
    <div className="dlg-backdrop">
      <form
        className="dlg fav-dlg"
        role="dialog"
        aria-modal="true"
        aria-label="Add to Favorites"
        onSubmit={(e) => {
          e.preventDefault();
          if (value.trim()) onAdd(value.trim(), folder || undefined);
        }}
      >
        <div className="fav-banner" aria-hidden="true">
          favorites
        </div>
        <label className="fav-label">
          Name:
          <input ref={box} type="text" value={value} onChange={(e) => setValue(e.target.value)} autoFocus />
        </label>
        <fieldset className="fav-folders">
          <legend>Create in:</legend>
          <label>
            <input type="radio" name="fav-folder" checked={folder === ""} onChange={() => setFolder("")} /> Favorites
          </label>
          {folders.map((f) => (
            <label key={f} className="fav-folder">
              <input type="radio" name="fav-folder" checked={folder === f} onChange={() => setFolder(f)} /> {f}
            </label>
          ))}
        </fieldset>
        <div className="dlg-buttons">
          <button type="button" onClick={() => setCreating("")}>
            New Folder…
          </button>
          <button type="submit" disabled={!value.trim()}>
            OK
          </button>
          <button type="button" onClick={onCancel}>
            Cancel
          </button>
        </div>
      </form>
      {creating !== null && (
        <NewFolderDialog
          value={creating}
          onChange={setCreating}
          taken={creating.trim() !== "" && folders.some((f) => f.toLowerCase() === creating.trim().toLowerCase())}
          onCreate={() => {
            if (favorites.addFolder(creating)) setFolder(creating.trim());
            setCreating(null);
          }}
          onCancel={() => setCreating(null)}
        />
      )}
    </div>
  );
}

/** The "Create New Folder" sub-dialog of Add to Favorites and Organize Favorites. */
export function NewFolderDialog({
  value,
  onChange,
  taken,
  onCreate,
  onCancel,
}: {
  value: string;
  onChange: (v: string) => void;
  taken: boolean;
  onCreate: () => void;
  onCancel: () => void;
}) {
  return (
    <div className="dlg-backdrop">
      <form
        className="dlg fav-dlg"
        role="dialog"
        aria-modal="true"
        aria-label="Create New Folder"
        onSubmit={(e) => {
          e.preventDefault();
          if (value.trim() && !taken) onCreate();
        }}
        onKeyDown={(e) => {
          if (e.key === "Escape") {
            e.stopPropagation();
            onCancel();
          }
        }}
      >
        <label className="fav-label">
          Folder name:
          <input type="text" value={value} onChange={(e) => onChange(e.target.value)} autoFocus />
        </label>
        {taken && <p className="enc-muted" role="alert">A folder with that name already exists.</p>}
        <div className="dlg-buttons">
          <button type="submit" disabled={!value.trim() || taken}>
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
