import { useEffect, useState } from "react";
import type { ArticleScope } from "./commands";

export interface ScopeChoice {
  scope: ArticleScope;
  label: string;
}

export const COPY_CHOICES: ScopeChoice[] = [
  { scope: "text", label: "Whole Article Text" },
  { scope: "media", label: "Whole Article With Media" },
  { scope: "selection", label: "Selected Text" },
];

/** The "what to copy" dialog (Print has its own, `PrintDialog.tsx`); Selected Text is enabled only with a selection. */
export function ScopeDialog({
  title,
  action,
  choices,
  hasSelection,
  onChoose,
  onCancel,
}: {
  title: string;
  action: string;
  choices: ScopeChoice[];
  hasSelection: boolean;
  onChoose: (scope: ArticleScope) => void;
  onCancel: () => void;
}) {
  const [scope, setScope] = useState<ArticleScope>(hasSelection ? "selection" : "text");
  useEffect(() => {
    const key = (e: KeyboardEvent) => {
      if (e.key === "Escape") onCancel();
    };
    document.addEventListener("keydown", key);
    return () => document.removeEventListener("keydown", key);
  }, [onCancel]);
  return (
    <div className="dlg-backdrop">
      <div className="dlg" role="dialog" aria-modal="true" aria-label={title}>
        <h2 className="dlg-title">
          <span>{title.replace(/ ?Options$/, "")}</span> <b>Options</b>
        </h2>
        <fieldset className="dlg-group">
          <legend>{action}</legend>
          {choices.map((c) => {
            const disabled = c.scope === "selection" && !hasSelection;
            return (
              <label key={c.scope} className={disabled ? "dlg-disabled" : ""}>
                <input type="radio" name="scope" disabled={disabled} checked={scope === c.scope} onChange={() => setScope(c.scope)} /> {c.label}
              </label>
            );
          })}
        </fieldset>
        <div className="dlg-buttons">
          <button type="button" autoFocus onClick={() => onChoose(scope)}>
            {action}
          </button>
          <button type="button" onClick={onCancel}>
            Cancel
          </button>
        </div>
      </div>
    </div>
  );
}
