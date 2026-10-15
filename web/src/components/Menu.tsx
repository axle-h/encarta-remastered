import { useEffect, useId, useRef, useState, type KeyboardEvent as ReactKeyboardEvent, type ReactNode } from "react";
import { Link, useLocation } from "react-router-dom";

/** One menu entry: a link, a command, a separator or a group heading. */
export interface MenuEntry {
  label?: ReactNode;
  /** The plain name when the label marks an access key (`E<u>x</u>it`). */
  ariaLabel?: string;
  to?: string;
  onSelect?: () => void;
  disabled?: boolean;
  separator?: boolean;
  heading?: boolean;
  /** Shown at the right ("Ctrl+P"). */
  shortcut?: string;
  /** Makes this entry a parent: a cascading submenu (one level), opened with click, hover or ArrowRight. */
  submenu?: MenuEntry[];
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
  contextFor,
}: {
  label?: ReactNode;
  /**
   * Makes this a context menu instead: no button; a right-click inside an element matching this
   * selector opens the menu at the pointer (focus on the first item when opened from the keyboard).
   */
  contextFor?: string;
  /** The plain name when the label marks an access key (`F<u>a</u>vorites`). */
  ariaLabel?: string;
  entries: () => MenuEntry[];
  className?: string;
  buttonClassName?: string;
  menuClassName?: string;
}) {
  const [open, setOpen] = useState<false | "first" | "last" | "none">(false);
  const [items, setItems] = useState<MenuEntry[]>([]);
  const [sub, setSub] = useState<{ key: string; focus: boolean } | null>(null);
  const [at, setAt] = useState<{ x: number; y: number } | null>(null);
  const wrap = useRef<HTMLDivElement>(null);
  const button = useRef<HTMLButtonElement>(null);
  const menu = useRef<HTMLUListElement>(null);
  const location = useLocation();
  const id = useId();

  const show = (focus: "first" | "last" | "none") => {
    setItems(entries());
    setSub(null);
    setOpen(focus);
  };
  const close = (refocus = false) => {
    setOpen(false);
    setSub(null);
    if (refocus) button.current?.focus();
  };

  useEffect(() => {
    setOpen(false);
  }, [location.pathname, location.search]);

  useEffect(() => {
    if (!contextFor) return;
    const on = (e: MouseEvent) => {
      const t = e.target;
      if (!(t instanceof Element) || !t.closest(contextFor)) return;
      e.preventDefault();
      setAt({ x: e.clientX, y: e.clientY });
      setItems(entries());
      setSub(null);
      // The context-menu key fires at (0, 0): put focus in the menu for the keyboard.
      setOpen(e.clientX === 0 && e.clientY === 0 ? "first" : "none");
    };
    document.addEventListener("contextmenu", on);
    return () => document.removeEventListener("contextmenu", on);
  });

  useEffect(() => {
    if (sub?.focus) focusables(menu.current?.querySelector<HTMLElement>('ul[role="menu"]') ?? null)[0]?.focus();
  }, [sub]);

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
    const active = document.activeElement as HTMLElement | null;
    const list = (active?.closest('ul[role="menu"]') as HTMLElement | null) ?? menu.current;
    const inSub = list !== menu.current;
    const all = focusables(list);
    const i = all.indexOf(active as HTMLElement);
    const go = (j: number) => all[(j + all.length) % all.length]?.focus();
    const parentKey = inSub ? (list?.parentElement?.dataset.sub ?? null) : null;
    const closeSub = () => {
      setSub(null);
      list?.parentElement?.querySelector<HTMLElement>(':scope > [aria-haspopup="menu"]')?.focus();
    };
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
        if (inSub && parentKey) closeSub();
        else close(true);
        break;
      case "Tab":
        setOpen(false);
        setSub(null);
        break;
      case "ArrowRight":
        if (active?.getAttribute("aria-haspopup") === "menu") {
          e.preventDefault();
          if (!active.hasAttribute("disabled")) setSub({ key: active.closest("li")?.dataset.sub ?? "", focus: true });
        } else if (inSub) e.preventDefault();
        else if (neighbour(wrap.current, 1, true)) {
          e.preventDefault();
          setOpen(false);
        }
        break;
      case "ArrowLeft":
        if (inSub) {
          e.preventDefault();
          closeSub();
        } else if (neighbour(wrap.current, -1, true)) {
          e.preventDefault();
          setOpen(false);
        }
        break;
    }
  };

  const pick = (entry: MenuEntry) => {
    setOpen(false);
    setSub(null);
    entry.onSelect?.();
  };

  const renderEntries = (list: MenuEntry[], nested: boolean): ReactNode =>
    list.map((m, i) => {
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
          {m.submenu && <span className="menu-arrow" aria-hidden="true">▸</span>}
        </>
      );
      if (m.submenu) {
        const isOpen = sub?.key === key;
        return (
          <li key={key} role="none" data-sub={key} className="menu-parent" onMouseEnter={() => !m.disabled && setSub({ key, focus: false })}>
            <button
              type="button"
              role="menuitem"
              tabIndex={-1}
              aria-haspopup="menu"
              aria-label={m.ariaLabel}
              aria-expanded={isOpen}
              disabled={m.disabled}
              aria-disabled={m.disabled}
              onClick={() => setSub({ key, focus: true })}
            >
              {body}
            </button>
            {isOpen && (
              <ul className="menu-popup menu-sub" role="menu" aria-label={m.ariaLabel ?? (typeof m.label === "string" ? m.label : undefined)}>
                {renderEntries(m.submenu, true)}
              </ul>
            )}
          </li>
        );
      }
      return (
        <li key={key} role="none" onMouseEnter={() => !nested && setSub(null)}>
          {m.to && !m.disabled ? (
            <Link role="menuitem" tabIndex={-1} to={m.to} aria-label={m.ariaLabel} onClick={() => pick(m)}>
              {body}
            </Link>
          ) : (
            <button type="button" role="menuitem" tabIndex={-1} disabled={m.disabled} aria-disabled={m.disabled} aria-label={m.ariaLabel} onClick={() => pick(m)}>
              {body}
            </button>
          )}
        </li>
      );
    });

  if (contextFor) {
    return (
      <div className={`menu ${className}`} ref={wrap}>
        {open && at && (
          <ul ref={menu} className={`menu-popup menu-context ${menuClassName}`} role="menu" aria-label={ariaLabel} style={{ position: "fixed", left: at.x, top: at.y }} onKeyDown={onMenuKey}>
            {renderEntries(items, false)}
          </ul>
        )}
      </div>
    );
  }
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
          {renderEntries(items, false)}
        </ul>
      )}
    </div>
  );
}

function focusables(menu: HTMLElement | null): HTMLElement[] {
  return menu ? Array.from(menu.querySelectorAll<HTMLElement>('[role="menuitem"]:not([disabled])')).filter((e) => e.closest('ul[role="menu"]') === menu) : [];
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
