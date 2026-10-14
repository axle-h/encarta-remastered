/**
 * City Climate, rebuilt from the Director movie's data (docs/formats/shockwave.md §2.2, §7): a bar
 * chart comparing up to eight cities on one of eight climate measures, with the movie's controls:
 * the Temperature and Precipitation measure buttons, the imperial/metric switch, an
 * "< add a city >" / "< remove a city >" menu per slot, the "na" legend and the intro narration.
 */
import { useMemo, useRef, useState } from "react";
import { Link } from "react-router-dom";
import { useApi } from "../../api/useApi";
import { useDocumentTitle } from "../../components/useDocumentTitle";
import { getChart, playableAudio, type ClimateData, type Units } from "../api";
import { BAR, formatNumber, linear, niceTicks } from "./chart";

export function CityClimate() {
  const data = useApi((s) => getChart<ClimateData>("city_climate", s), []);
  useDocumentTitle("City Climate");
  return (
    <div className="games-page chart-page">
      <div className="games-crumbs">
        <Link to="/games">Games &amp; Fun Stuff</Link> › City Climate
      </div>
      {data.error && <p className="games-error">{data.error.message}</p>}
      {data.loading && <div className="enc-loading"><div className="enc-spinner" /></div>}
      {data.data && <ClimateChart data={data.data} />}
    </div>
  );
}

const DEFAULT_CITIES = ["Washington, D.C., United States", "London, Great Britain", "Tokyo, Japan"];

export function ClimateChart({ data, initial }: { data: ClimateData; initial?: string[] }) {
  const [units, setUnits] = useState<Units>(data.default_units);
  const [measureId, setMeasureId] = useState(data.measures[0]?.id ?? "");
  const [slots, setSlots] = useState<(string | null)[]>(() => {
    const want = (initial ?? DEFAULT_CITIES).filter((l) => data.cities.some((c) => c.label === l));
    return Array.from({ length: data.max_selection }, (_, i) => want[i] ?? null);
  });
  const [table, setTable] = useState(false);
  const audio = useRef<HTMLAudioElement>(null);
  const narration = playableAudio(data.narration?.media);
  const measure = data.measures.find((m) => m.id === measureId) ?? data.measures[0]!;
  const u = measure.units[units];
  const byLabel = useMemo(() => new Map(data.cities.map((c) => [c.label, c])), [data.cities]);
  const chosen = slots.map((l) => (l ? byLabel.get(l) ?? null : null));

  // Chart geometry.
  const W = 640;
  const H = 300;
  const M = { l: 52, r: 12, t: 30, b: 10 };
  const lo = Math.min(u.axis_min, 0);
  const hi = u.axis_max;
  const y = linear(lo, hi, H - M.b, M.t);
  const slotW = (W - M.l - M.r) / data.max_selection;
  const barW = Math.min(44, slotW * 0.56);
  const ticks = niceTicks(lo, hi, 8);

  return (
    <div className="chart-window" role="region" aria-label={data.title}>
      <div className="chart-titlebar">
        <span>{data.title}</span>
        {narration && (
          <>
            <audio ref={audio} src={narration} preload="none" />
            <button type="button" className="games-btn" onClick={() => void audio.current?.play()}>
              ▶ Introduction
            </button>
          </>
        )}
      </div>
      <div className="chart-body">
        <div className="chart-controls">
          {data.categories.map((cat) => (
            <fieldset key={cat.id}>
              <legend>{cat.label}</legend>
              {data.measures
                .filter((m) => m.category === cat.id)
                .map((m) => (
                  <label key={m.id} className={m.id === measure.id ? "is-on" : ""}>
                    <input type="radio" name="clim-measure" checked={m.id === measure.id} onChange={() => setMeasureId(m.id)} />
                    {m.label}
                  </label>
                ))}
            </fieldset>
          ))}
          <fieldset>
            <legend>Units</legend>
            {data.unit_systems.map((s) => (
              <label key={s} className={s === units ? "is-on" : ""}>
                <input type="radio" name="clim-units" checked={s === units} onChange={() => setUnits(s)} />
                {s === "imperial" ? "Imperial" : "Metric"}
              </label>
            ))}
          </fieldset>
          <p className="chart-note">{data.strings.not_available}</p>
          <button type="button" className="games-btn" onClick={() => setTable(!table)}>
            {table ? "Hide table" : "Show table"}
          </button>
        </div>
        <div className="chart-main">
          <svg className="chart-svg" viewBox={`0 0 ${W} ${H}`} role="img" aria-label={`${measure.label} (${u.abbr})`}>
            <text x={M.l} y={16} className="chart-title">
              {data.categories.find((c) => c.id === measure.category)?.label}: {measure.label} ({u.abbr})
            </text>
            {ticks.map((t) => (
              <g key={t}>
                <line x1={M.l} x2={W - M.r} y1={y(t)} y2={y(t)} className={t === 0 ? "chart-zero" : "chart-grid"} />
                <text x={M.l - 6} y={y(t) + 4} className="chart-tick" textAnchor="end">
                  {t.toLocaleString("en-US")}
                </text>
              </g>
            ))}
            {chosen.map((c, i) => {
              const cx = M.l + slotW * (i + 0.5);
              if (!c) return null;
              const v = c.values[units][measure.id] ?? null;
              if (v == null)
                return (
                  <text key={i} x={cx} y={y(0) - 6} className="chart-value" textAnchor="middle">
                    na
                  </text>
                );
              const clamped = Math.max(lo, Math.min(hi, v));
              const top = Math.min(y(clamped), y(0));
              const h = Math.max(1, Math.abs(y(clamped) - y(0)));
              const r = Math.min(4, h / 2, barW / 2);
              const up = v >= 0;
              // Rounded at the data end only, anchored square on the zero line.
              const d = up
                ? `M${cx - barW / 2},${top + h} V${top + r} Q${cx - barW / 2},${top} ${cx - barW / 2 + r},${top} H${cx + barW / 2 - r} Q${cx + barW / 2},${top} ${cx + barW / 2},${top + r} V${top + h} Z`
                : `M${cx - barW / 2},${top} V${top + h - r} Q${cx - barW / 2},${top + h} ${cx - barW / 2 + r},${top + h} H${cx + barW / 2 - r} Q${cx + barW / 2},${top + h} ${cx + barW / 2},${top + h - r} V${top} Z`;
              return (
                <g key={i} className="chart-bar">
                  <path d={d} fill={BAR}>
                    <title>
                      {c.label}: {formatNumber(v)} {u.abbr}
                    </title>
                  </path>
                  <text x={cx} y={up ? top - 5 : top + h + 13} className="chart-value" textAnchor="middle">
                    {formatNumber(v)}
                  </text>
                </g>
              );
            })}
          </svg>
          <div className="chart-slots" style={{ paddingLeft: M.l, paddingRight: M.r }}>
            {slots.map((label, i) => (
              <select
                key={i}
                aria-label={`City ${i + 1}`}
                value={label ?? ""}
                onChange={(e) => setSlots((s) => s.map((x, j) => (j === i ? e.target.value || null : x)))}
                className={label ? "is-set" : ""}
              >
                <option value="">{label ? data.strings.remove : data.strings.add}</option>
                {data.cities.map((c) => (
                  <option key={c.label} value={c.label} disabled={slots.includes(c.label) && c.label !== label}>
                    {c.label}
                  </option>
                ))}
              </select>
            ))}
          </div>
          {table && (
            <table className="chart-table">
              <thead>
                <tr>
                  <th>City</th>
                  {data.measures.map((m) => (
                    <th key={m.id}>
                      {m.label} ({m.units[units].abbr})
                    </th>
                  ))}
                </tr>
              </thead>
              <tbody>
                {chosen.filter(Boolean).map((c) => (
                  <tr key={c!.label}>
                    <th>{c!.label}</th>
                    {data.measures.map((m) => (
                      <td key={m.id}>{formatNumber(c!.values[units][m.id])}</td>
                    ))}
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </div>
      </div>
    </div>
  );
}
