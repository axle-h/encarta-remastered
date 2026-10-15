import { useEffect, useState } from "react";
import { useLocation } from "react-router-dom";
import { LONG_PRINT_PAGES, PRINT_EVENT, estimatePrintPages, printPage, printWith, selectedText, type PrintScope } from "./commands";

/** The four Print dialogs Encarta shows (fixtures/ui/print2): article, picture, timeline and Kids. */
export type PrintKind = "article" | "picture" | "timeline" | "kids";

interface Choice {
  scope: PrintScope;
  label: string;
}

/** Radios per dialog, as captured. Selected Text appears only while something is selected. */
export function printChoices(kind: PrintKind, hasSelection: boolean): Choice[] {
  switch (kind) {
    case "picture":
      return [
        { scope: "picture", label: "Picture" },
        { scope: "caption", label: "Picture plus caption" },
      ];
    case "timeline":
      return [
        { scope: "timeline-page", label: "All events on this page" },
        { scope: "timeline-all", label: "All events in current timeline" },
      ];
    case "kids":
      return [{ scope: "text", label: "Whole Article Text" }];
    default:
      return [
        { scope: "text", label: "Whole Article Text" },
        { scope: "media", label: "Whole Article With Media" },
        ...(hasSelection ? [{ scope: "selection" as const, label: "Selected Text" }] : []),
      ];
  }
}

/** Which dialog File › Print opens for what is on screen; null prints the page straight away. */
export function printKindOf(pathname: string, kids: boolean): PrintKind | null {
  if (document.querySelector(".mv-stage")) return "picture";
  if (pathname.startsWith("/timeline")) return "timeline";
  if (document.querySelector(".art-text")) return kids ? "kids" : "article";
  return null;
}

/**
 * Encarta's Print dialog: "What do you want to print?", the radios of its kind and Print Setup / Print /
 * Cancel. Print Setup opens the browser's print flow (the same as Print): a web page cannot open the OS
 * page-setup dialog, and the browser's print dialog holds the paper, margins and printer settings.
 * An article estimated at more than 5 pages first asks "longer than 5 pages" (Yes prints, No cancels),
 * as Encarta does for both buttons.
 */
export function PrintDialog({
  kind,
  hasSelection,
  onPrint,
  onCancel,
}: {
  kind: PrintKind;
  hasSelection: boolean;
  onPrint: (scope: PrintScope) => void;
  onCancel: () => void;
}) {
  const choices = printChoices(kind, hasSelection);
  const [scope, setScope] = useState<PrintScope>(hasSelection && kind === "article" ? "selection" : choices[0]!.scope);
  const [asking, setAsking] = useState<PrintScope | null>(null);
  const go = () => {
    const long = (kind === "article" || kind === "kids") && estimatePrintPages(scope) > LONG_PRINT_PAGES;
    if (long) setAsking(scope);
    else onPrint(scope);
  };
  useEffect(() => {
    const key = (e: KeyboardEvent) => {
      if (e.key === "Escape") onCancel();
    };
    document.addEventListener("keydown", key);
    return () => document.removeEventListener("keydown", key);
  }, [onCancel]);
  return (
    <div className="dlg-backdrop print-backdrop">
      {asking ? (
        <div className="print-prompt" role="alertdialog" aria-modal="true" aria-label="Print" aria-describedby="print-prompt-text">
          <p id="print-prompt-text">The article you are printing is longer than {LONG_PRINT_PAGES} pages. Do you still want to print the whole article?</p>
          <div className="print-prompt-buttons">
            <button type="button" autoFocus onClick={() => onPrint(asking)}>
              Yes
            </button>
            <button type="button" onClick={onCancel}>
              No
            </button>
          </div>
        </div>
      ) : (
        <div className="print-dlg" role="dialog" aria-modal="true" aria-label="Print">
          <div className="print-titlebar">
            <span>Print</span>
            <button type="button" className="print-close" aria-label="Close" onClick={onCancel}>
              ×
            </button>
          </div>
          <div className="print-banner" aria-hidden="true">
            print
          </div>
          <fieldset className="print-group">
            <legend>What do you want to print?</legend>
            {choices.map((c) => (
              <label key={c.scope}>
                <input type="radio" name="print-scope" autoFocus={scope === c.scope} checked={scope === c.scope} onChange={() => setScope(c.scope)} /> {c.label}
              </label>
            ))}
          </fieldset>
          <div className="print-buttons">
            <button type="button" title="Opens the browser's print dialog, where the page setup is" onClick={go}>
              Print Setup
            </button>
            <button type="button" onClick={go}>
              Print
            </button>
            <button type="button" onClick={onCancel}>
              Cancel
            </button>
          </div>
        </div>
      )}
    </div>
  );
}

/** Listens for File › Print / Ctrl+P / the Kids Print button and shows the matching dialog. */
export function PrintHost({ kids = false }: { kids?: boolean }) {
  const location = useLocation();
  const [open, setOpen] = useState<{ kind: PrintKind; selection: boolean } | null>(null);
  useEffect(() => {
    const on = () => {
      const kind = printKindOf(location.pathname, kids);
      if (kind) setOpen({ kind, selection: !!selectedText().trim() });
      else printPage();
    };
    window.addEventListener(PRINT_EVENT, on);
    return () => window.removeEventListener(PRINT_EVENT, on);
  }, [location.pathname, kids]);
  if (!open) return null;
  return (
    <PrintDialog
      kind={open.kind}
      hasSelection={open.selection}
      onCancel={() => setOpen(null)}
      onPrint={(scope) => {
        setOpen(null);
        window.setTimeout(() => printWith(scope), 50);
      }}
    />
  );
}
