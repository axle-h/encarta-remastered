import { useEffect } from "react";
import { SearchBox } from "../components/SearchBox";
import type { QuickKind } from "./featuresData";
import { QUICK_LABEL } from "./searchPanel";

/** Quick Search (Features menu): a small modal with one text box and Go; Enter hands the term back. */
export function QuickSearchDialog({ kind, onSearch, onCancel }: { kind: QuickKind; onSearch: (term: string) => void; onCancel: () => void }) {
  useEffect(() => {
    const key = (e: KeyboardEvent) => {
      if (e.key === "Escape") onCancel();
    };
    document.addEventListener("keydown", key);
    return () => document.removeEventListener("keydown", key);
  }, [onCancel]);
  return (
    <div className="dlg-backdrop">
      <div className="dlg quick-dlg" role="dialog" aria-modal="true" aria-label="Quick Search">
        <h2 className="quick-title">Quick Search</h2>
        <p className="quick-kind">{QUICK_LABEL[kind]}</p>
        <SearchBox autoFocus label={`Quick Search: ${QUICK_LABEL[kind]}`} placeholder={`Search ${QUICK_LABEL[kind].toLowerCase()}`} onSearch={onSearch} />
        <div className="dlg-buttons">
          <button type="button" onClick={onCancel}>
            Cancel
          </button>
        </div>
      </div>
    </div>
  );
}
