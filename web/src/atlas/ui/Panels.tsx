import { useMemo, useState, type ReactNode } from "react";
import type { StyleSpecification } from "maplibre-gl";
import type { Look } from "../data";
import { classGroups, classShown, setShown, type ClassGroup } from "../groups";
import { cssFont, legendEntries, type LegendEntry } from "../legend";
import type { ClassOverrides } from "../layers";
import type { Projection } from "../style";
import { SpriteIcon } from "./Sprite";

export function Panel({ title, onClose, children, className }: { title: string; onClose: () => void; children: ReactNode; className?: string }) {
  return (
    <section className={`enc-atlas-panel ${className ?? ""}`} aria-label={title}>
      <header>
        <h2>{title}</h2>
        <button type="button" className="enc-atlas-panel__close" aria-label="Close" onClick={onClose}>
          ×
        </button>
      </header>
      <div className="enc-atlas-panel__body">{children}</div>
    </section>
  );
}

// ---------------------------------------------------------------------------------------------
// Map Legend

function LineSwatch({ e }: { e: LegendEntry }) {
  const h = 14;
  return (
    <svg width="34" height={h} aria-hidden="true">
      {e.lines.map((l, i) => (
        <line
          key={i}
          x1="1"
          x2="33"
          y1={h / 2 + (l.offset ?? 0)}
          y2={h / 2 + (l.offset ?? 0)}
          stroke={l.color}
          strokeWidth={Math.min(l.width, 9)}
          strokeDasharray={l.dash ? l.dash.map((d) => d * Math.max(1, l.width)).join(" ") : undefined}
        />
      ))}
    </svg>
  );
}

function Swatch({ e }: { e: LegendEntry }) {
  if (e.icon) return <SpriteIcon name={e.icon} />;
  if (e.circle) return <span className="enc-atlas-swatch enc-atlas-swatch--circle" style={{ background: e.circle.color }} />;
  if (e.lines.length) return <LineSwatch e={e} />;
  if (e.fill?.pattern) return <span className="enc-atlas-swatch enc-atlas-swatch--pattern"><SpriteIcon name={e.fill.pattern} /></span>;
  if (e.fill?.color) return <span className="enc-atlas-swatch" style={{ background: e.fill.color, opacity: e.fill.opacity ?? 1 }} />;
  return (
    <span className="enc-atlas-swatch enc-atlas-swatch--text" style={{ color: e.text?.color, font: cssFont(e.text?.font, 11) }}>
      Aa
    </span>
  );
}

export function LegendPanel({
  style,
  zoom,
  overrides,
  look,
  lookName,
  stats,
  inView,
  onClose,
}: {
  style: StyleSpecification;
  zoom: number;
  overrides: ClassOverrides;
  look: Look | undefined;
  lookName: string | undefined;
  stats: boolean;
  /** Class rows with a feature drawn in the current view (null while unknown). */
  inView: ReadonlySet<number> | null;
  onClose: () => void;
}) {
  const [onlyInView, setOnlyInView] = useState(true);
  const rounded = Math.round(zoom * 4) / 4;
  const all = useMemo(() => legendEntries(style, rounded, overrides, { stats }), [style, rounded, overrides, stats]);
  const entries = onlyInView && inView ? all.filter((e) => e.classes.some((c) => inView.has(c))) : all;
  const ramp = look?.legend?.filter((c, i, a) => a.indexOf(c) === i) ?? [];
  return (
    <Panel title="Map Legend" onClose={onClose} className="enc-atlas-legend">
      {ramp.length > 1 && (
        <div className="enc-atlas-legend__ramp" title={`${lookName ?? ""} colours (${ramp.length})`}>
          {ramp.slice(0, 200).map((c, i) => (
            <span key={i} style={{ background: c }} />
          ))}
        </div>
      )}
      <label className="enc-atlas-check">
        <input type="checkbox" checked={onlyInView} onChange={(e) => setOnlyInView(e.target.checked)} />
        Only what is on the map now ({entries.length} of {all.length})
      </label>
      {entries.length === 0 && <p className="enc-atlas-note">No map features at this altitude.</p>}
      <ul>
        {entries.map((e) => (
          <li key={e.name}>
            <span className="enc-atlas-legend__sym">
              <Swatch e={e} />
            </span>
            <span className="enc-atlas-legend__name">{e.name}</span>
          </li>
        ))}
      </ul>
    </Panel>
  );
}

// ---------------------------------------------------------------------------------------------
// Map Customizer

export interface CustomizerProps {
  raw: StyleSpecification;
  projection: Projection;
  onProjection: (p: Projection) => void;
  overrides: ClassOverrides;
  onOverrides: (o: ClassOverrides) => void;
  labelScale: number;
  onLabelScale: (s: number) => void;
  looks: string[];
  look: string | undefined;
  defaultLook: string | undefined;
  onLook: (l: string | undefined) => void;
  units: "imperial" | "metric";
  onUnits: (u: "imperial" | "metric") => void;
  onReset: () => void;
  onClose: () => void;
}

function GroupRow({ g, overrides, onOverrides }: { g: ClassGroup; overrides: ClassOverrides; onOverrides: (o: ClassOverrides) => void }) {
  const shown = g.classes.filter((c) => classShown(c, overrides.on, overrides.off)).length;
  const all = shown === g.classes.length;
  return (
    <details className="enc-atlas-group">
      <summary>
        <input
          type="checkbox"
          checked={shown > 0}
          ref={(el) => {
            if (el) el.indeterminate = shown > 0 && !all;
          }}
          onChange={(e) => onOverrides(setShown(g.classes, e.target.checked, overrides.on, overrides.off))}
          onClick={(e) => e.stopPropagation()}
          aria-label={g.label}
        />
        <span>{g.label}</span>
        <span className="enc-atlas-group__count">
          {shown}/{g.classes.length}
        </span>
      </summary>
      <ul>
        {g.classes.map((c) => (
          <li key={c.id}>
            <label>
              <input
                type="checkbox"
                checked={classShown(c, overrides.on, overrides.off)}
                onChange={(e) => onOverrides(setShown([c], e.target.checked, overrides.on, overrides.off))}
              />
              <span className={c.byDefault ? undefined : "enc-atlas-optional"}>{c.name}</span>
            </label>
          </li>
        ))}
      </ul>
    </details>
  );
}

export function CustomizerPanel(p: CustomizerProps) {
  const groups = useMemo(() => classGroups(p.raw), [p.raw]);
  return (
    <Panel title="Map Customizer" onClose={p.onClose} className="enc-atlas-customizer">
      <div className="enc-atlas-seg" role="group" aria-label="View">
        <button type="button" aria-pressed={p.projection === "flat"} onClick={() => p.onProjection("flat")}>
          Flat Map View
        </button>
        <button type="button" aria-pressed={p.projection === "globe"} onClick={() => p.onProjection("globe")}>
          Globe View
        </button>
      </div>
      <p className="enc-atlas-note">Use the checkboxes to choose the features you want to show on your map. Grey names are hidden by default.</p>
      {groups.map((g) => (
        <GroupRow key={g.key} g={g} overrides={p.overrides} onOverrides={p.onOverrides} />
      ))}
      <fieldset>
        <legend>Labels</legend>
        {[
          [1.25, "Large Labels"],
          [0, "No Labels"],
          [1, "Normal Labels"],
        ].map(([v, label]) => (
          <label key={label as string}>
            <input type="radio" name="enc-atlas-labels" checked={p.labelScale === v} onChange={() => p.onLabelScale(v as number)} />
            {label}
          </label>
        ))}
      </fieldset>
      {p.looks.length > 1 && (
        <label className="enc-atlas-field">
          Terrain look
          <select value={p.look ?? ""} onChange={(e) => p.onLook(e.target.value || undefined)}>
            <option value="">{p.defaultLook ? `${p.defaultLook} (style default)` : "Style default"}</option>
            {p.looks
              .filter((l) => l !== p.defaultLook)
              .map((l) => (
                <option key={l} value={l}>
                  {l}
                </option>
              ))}
          </select>
        </label>
      )}
      <label className="enc-atlas-field">
        Scale units
        <select value={p.units} onChange={(e) => p.onUnits(e.target.value as "imperial" | "metric")}>
          <option value="imperial">Miles</option>
          <option value="metric">Kilometres</option>
        </select>
      </label>
      <div className="enc-atlas-panel__foot">
        <button type="button" onClick={p.onReset}>
          Reset
        </button>
      </div>
    </Panel>
  );
}
