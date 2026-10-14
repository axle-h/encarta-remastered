import { useEffect, useId, useRef, useState, type KeyboardEvent as ReactKeyboardEvent, type ReactNode } from "react";
import { Link, useLocation } from "react-router-dom";

/** One menu entry: a link, a command, a separator or a group heading. */
export interface MenuEntry {
  label?: ReactNode;
  to?: string;
  onSelect?: () => void;
  disabled?: boolean;
  separator?: boolean;
  heading?: boolean;
  /** Shown at the right ("Ctrl+P"). */
  shortcut?: string;
  key?: string;
}

/**
 * A menu button (WAI-ARIA menu pattern): Enter, Space or ArrowDown open it on the first item,
 * ArrowUp on the last; arrows, Home and End move between items; Escape closes it and returns
 * focus to the button; Tab or a click outside closes it. Inside a `role="menubar"`, ArrowLeft and
 * ArrowRight move to the neighbouring menu. Entries are computed when the menu opens.
 */
export function MenuButton({
  label,
  entries,
  className = "",
  buttonClassName = "",
  menuClassName = "",
  ariaLabel,
}: {
  label: ReactNode;
  /** The plain name when the label marks an access key (`F<u>a</u>vorites`). */
  ariaLabel?: string;
  entries: () => MenuEntry[];
  className?: string;
  buttonClassName?: string;
  menuClassName?: string;
}) {
  const [open, setOpen] = useState<false | "first" | "last" | "none">(false);
  const [items, setItems] = useState<MenuEntry[]>([]);
  const wrap = useRef<HTMLDivElement>(null);
  const button = useRef<HTMLButtonElement>(null);
  const menu = useRef<HTMLUListElement>(null);
  const location = useLocation();
  const id = useId();

  const show = (focus: "first" | "last" | "none") => {
    setItems(entries());
    setOpen(focus);
  };
  const close = (refocus = false) => {
    setOpen(false);
    if (refocus) button.current?.focus();
  };

  useEffect(() => {
    setOpen(false);
  }, [location.pathname, location.search]);

  useEffect(() => {
    if (!open) return;
    const all = focusables(menu.current);
    if (open === "first") all[0]?.focus();
    else if (open === "last") all[all.length - 1]?.focus();
    const outside = (e: MouseEvent) => {
      if (!wrap.current?.contains(e.target as Node)) setOpen(false);
    };
    // Escape also closes a menu opened with the mouse, when focus is not inside it.
    const esc = (e: globalThis.KeyboardEvent) => {
      if (e.key === "Escape" && !menu.current?.contains(document.activeElement)) {
        setOpen(false);
        if (wrap.current?.contains(document.activeElement)) button.current?.focus();
      }
    };
    document.addEventListener("mousedown", outside);
    document.addEventListener("keydown", esc);
    return () => {
      document.removeEventListener("mousedown", outside);
      document.removeEventListener("keydown", esc);
    };
  }, [open]);

  const onButtonKey = (e: ReactKeyboardEvent) => {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      show("first");
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      show("last");
    } else if (e.key === "ArrowRight" || e.key === "ArrowLeft") {
      if (neighbour(wrap.current, e.key === "ArrowRight" ? 1 : -1, false)) e.preventDefault();
    }
  };

  const onMenuKey = (e: ReactKeyboardEvent) => {
    const all = focusables(menu.current);
    const i = all.indexOf(document.activeElement as HTMLElement);
    const go = (j: number) => all[(j + all.length) % all.length]?.focus();
    switch (e.key) {
      case "ArrowDown":
        e.preventDefault();
        go(i + 1);
        break;
      case "ArrowUp":
        e.preventDefault();
        go(i - 1);
        break;
      case "Home":
        e.preventDefault();
        go(0);
        break;
      case "End":
        e.preventDefault();
        go(all.length - 1);
        break;
      case "Escape":
        e.preventDefault();
        close(true);
        break;
      case "Tab":
        setOpen(false);
        break;
      case "ArrowRight":
      case "ArrowLeft":
        if (neighbour(wrap.current, e.key === "ArrowRight" ? 1 : -1, true)) {
          e.preventDefault();
          setOpen(false);
        }
        break;
    }
  };

  const pick = (entry: MenuEntry) => {
    setOpen(false);
    entry.onSelect?.();
  };

  return (
    <div className={`menu ${className}`} ref={wrap}>
      <button
        type="button"
        ref={button}
        className={buttonClassName}
        aria-haspopup="menu"
        aria-expanded={!!open}
        aria-controls={open ? id : undefined}
        aria-label={ariaLabel}
        onClick={() => (open ? close() : show("none"))}
        onKeyDown={onButtonKey}
      >
        {label}
      </button>
      {open && (
        <ul id={id} ref={menu} className={`menu-popup ${menuClassName}`} role="menu" aria-label={ariaLabel} onKeyDown={onMenuKey}>
          {items.map((m, i) => {
            const key = m.key ?? `${i}`;
            if (m.separator) return <li key={key} role="separator" className="menu-sep" />;
            if (m.heading) {
              return (
                <li key={key} role="presentation" className="menu-heading">
                  {m.label}
                </li>
              );
            }
            const body = (
              <>
                <span className="menu-label">{m.label}</span>
                {m.shortcut && <span className="menu-shortcut">{m.shortcut}</span>}
              </>
            );
            return (
              <li key={key} role="none">
                {m.to && !m.disabled ? (
                  <Link role="menuitem" tabIndex={-1} to={m.to} onClick={() => pick(m)}>
                    {body}
                  </Link>
                ) : (
                  <button type="button" role="menuitem" tabIndex={-1} disabled={m.disabled} aria-disabled={m.disabled} onClick={() => pick(m)}>
                    {body}
                  </button>
                )}
              </li>
            );
          })}
        </ul>
      )}
    </div>
  );
}

function focusables(menu: HTMLElement | null): HTMLElement[] {
  return menu ? Array.from(menu.querySelectorAll<HTMLElement>('[role="menuitem"]:not([disabled])')) : [];
}

/** Focuses the next or previous menu button of the enclosing menubar (and opens it when `open`). */
function neighbour(from: HTMLElement | null, by: 1 | -1, open: boolean): boolean {
  const bar = from?.closest('[role="menubar"]');
  if (!bar || !from) return false;
  const buttons = Array.from(bar.querySelectorAll<HTMLButtonElement>(':scope > .menu > button[aria-haspopup="menu"]'));
  const i = buttons.findIndex((b) => from.contains(b));
  const next = buttons[(i + by + buttons.length) % buttons.length];
  if (!next || i < 0) return false;
  next.focus();
  if (open) next.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowDown", bubbles: true }));
  return true;
}
