/**
 * Population Chart, rebuilt from the Director movie's data (docs/formats/shockwave.md §2.2, §7): a
 * line chart of census figures for up to four US states or Canadian provinces, with the movie's
 * controls: the US/Canada switch, the three measures, imperial/metric (density), an
 * "< add a state >" menu per series, the preset y scales (smallest that fits) and the footnote.
 */
import { useMemo, useRef, useState } from "react";
import { Link } from "react-router-dom";
import { useApi } from "../../api/useApi";
import { useDocumentTitle } from "../../components/useDocumentTitle";
import { getChart, playableAudio, type PopulationData, type Units } from "../api";
import { SERIES, axisLabel, formatNumber, linear, measureLabel, pickScale, seriesMax } from "./chart";

export function PopulationChart() {
  const data = useApi((s) => getChart<PopulationData>("population", s), []);
  useDocumentTitle("Population Chart");
  return (
    <div className="games-page chart-page">
      <div className="games-crumbs">
        <Link to="/games">Games &amp; Fun Stuff</Link> › Population Chart
      </div>
      {data.error && <p className="games-error">{data.error.message}</p>}
      {data.loading && <div className="enc-loading"><div className="enc-spinner" /></div>}
      {data.data && <PopChart data={data.data} />}
    </div>
  );
}

const DEFAULTS: Record<string, string[]> = { us: ["California", "New York", "Texas"], canada: ["Ontario", "Québec", "British Columbia"] };

export function PopChart({ data }: { data: PopulationData }) {
  const [dsId, setDsId] = useState(data.datasets[0]?.id ?? "us");
  const ds = data.datasets.find((d) => d.id === dsId) ?? data.datasets[0]!;
  const [unitsBy, setUnitsBy] = useState<Record<string, Units>>({});
  const units = unitsBy[ds.id] ?? ds.default_units;
  const [measureId, setMeasureId] = useState(ds.measures[0]?.id ?? "total");
  const [slotsBy, setSlotsBy] = useState<Record<string, (string | null)[]>>({});
  const slots = slotsBy[ds.id] ?? Array.from({ length: data.max_selection }, (_, i) => (DEFAULTS[ds.id] ?? []).filter((n) => ds.regions.some((r) => r.name === n))[i] ?? null);
  const setSlots = (s: (string | null)[]) => setSlotsBy((m) => ({ ...m, [ds.id]: s }));
  const [hover, setHover] = useState<number | null>(null);
  const [table, setTable] = useState(false);
  const audio = useRef<HTMLAudioElement>(null);
  const narration = playableAudio(data.narration?.media);
  const measure = ds.measures.find((m) => m.id === measureId) ?? ds.measures[0]!;
  const series = slots.map((name) => (name ? ds.regions.find((r) => r.name === name) ?? null : null));
  const values = series.map((r) => (r ? r.values[units][measure.id] ?? [] : []));
  const scale = useMemo(() => pickScale(measure.units[units].scales, seriesMax(values)), [measure, units, values]);
  const top = scale[scale.length - 1] ?? 1;

  const W = 640;
  const H = 320;
  const M = { l: 62, r: 16, t: 30, b: 30 };
  const x = linear(ds.axis.chart_start, ds.axis.chart_end, M.l, W - M.r);
  const y = linear(0, top, H - M.b, M.t);
  const decimals = measure.id === "total" ? 0 : 1;
  const unitName = measure.units[units].name;

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
          <fieldset>
            <legend>Country</legend>
            {data.datasets.map((d) => (
              <label key={d.id} className={d.id === ds.id ? "is-on" : ""}>
                <input type="radio" name="pop-ds" checked={d.id === ds.id} onChange={() => setDsId(d.id)} />
                {d.country === "US" ? "United States" : d.country}
              </label>
            ))}
          </fieldset>
          <fieldset>
            <legend>{ds.category_label}</legend>
            {ds.measures.map((m) => (
              <label key={m.id} className={m.id === measure.id ? "is-on" : ""}>
                <input type="radio" name="pop-measure" checked={m.id === measure.id} onChange={() => setMeasureId(m.id)} />
                {measureLabel(m.label)}
              </label>
            ))}
          </fieldset>
          <fieldset>
            <legend>Units</legend>
            {data.unit_systems.map((s) => (
              <label key={s} className={s === units ? "is-on" : ""}>
                <input type="radio" name="pop-units" checked={s === units} onChange={() => setUnitsBy((m) => ({ ...m, [ds.id]: s }))} />
                {s === "imperial" ? "Imperial" : "Metric"}
              </label>
            ))}
          </fieldset>
          <button type="button" className="games-btn" onClick={() => setTable(!table)}>
            {table ? "Hide table" : "Show table"}
          </button>
        </div>
        <div className="chart-main">
          <svg
            className="chart-svg"
            viewBox={`0 0 ${W} ${H}`}
            role="img"
            aria-label={`${measureLabel(measure.label)}${unitName ? ` (${unitName})` : ""}`}
            onMouseLeave={() => setHover(null)}
            onMouseMove={(e) => {
              const svg = e.currentTarget.getBoundingClientRect();
              const px = ((e.clientX - svg.left) / svg.width) * W;
              let best = 0;
              ds.years.forEach((yr, i) => {
                if (Math.abs(x(yr) - px) < Math.abs(x(ds.years[best]!) - px)) best = i;
              });
              setHover(best);
            }}
          >
            <text x={M.l} y={16} className="chart-title">
              {measureLabel(measure.label)}
              {unitName ? ` (${unitName})` : ""}
            </text>
            {scale.map((t) => (
              <g key={t}>
                <line x1={M.l} x2={W - M.r} y1={y(t)} y2={y(t)} className={t === 0 ? "chart-zero" : "chart-grid"} />
                <text x={M.l - 6} y={y(t) + 4} className="chart-tick" textAnchor="end">
                  {axisLabel(t)}
                </text>
              </g>
            ))}
            {ds.years.map((yr) => (
              <text key={yr} x={x(yr)} y={H - M.b + 16} className="chart-tick" textAnchor="middle">
                {yr}
              </text>
            ))}
            {hover !== null && <line className="chart-crosshair" x1={x(ds.years[hover]!)} x2={x(ds.years[hover]!)} y1={M.t} y2={H - M.b} />}
            {values.map((vals, si) => {
              if (!series[si]) return null;
              const pts = vals.map((v, i) => (v == null ? null : ([x(ds.years[i]!), y(Math.min(v, top))] as const)));
              const d = pts.reduce((acc, p, i) => (p ? `${acc}${acc && pts[i - 1] ? "L" : "M"}${p[0].toFixed(1)},${p[1].toFixed(1)}` : acc), "");
              return (
                <g key={si}>
                  <path d={d} fill="none" stroke={SERIES[si]} strokeWidth={2} strokeLinejoin="round" />
                  {pts.map((p, i) =>
                    p ? <circle key={i} cx={p[0]} cy={p[1]} r={hover === i ? 5 : 4} fill={SERIES[si]} stroke="#fcfcfb" strokeWidth={2} /> : null,
                  )}
                </g>
              );
            })}
          </svg>
          {hover !== null && series.some(Boolean) && (
            <div className="chart-tooltip" role="status">
              <b>{ds.years[hover]}</b>
              {series.map((r, si) =>
                r ? (
                  <span key={si}>
                    <i style={{ background: SERIES[si] }} /> {r.name}: {formatNumber(values[si]?.[hover] ?? null, decimals)}
                  </span>
                ) : null,
              )}
            </div>
          )}
          <div className="chart-slots chart-series">
            {slots.map((name, i) => (
              <label key={i} className="chart-series-slot">
                <i style={{ background: name ? SERIES[i] : "transparent" }} aria-hidden="true" />
                <select aria-label={`Series ${i + 1}`} value={name ?? ""} onChange={(e) => setSlots(slots.map((s, j) => (j === i ? e.target.value || null : s)))} className={name ? "is-set" : ""}>
                  <option value="">{name ? ds.strings.remove : ds.strings.add}</option>
                  {ds.regions.map((r) => (
                    <option key={r.name} value={r.name} disabled={slots.includes(r.name) && r.name !== name}>
                      {r.name}
                    </option>
                  ))}
                </select>
              </label>
            ))}
          </div>
          <p className="chart-note">{ds.footnote}</p>
          {table && (
            <table className="chart-table">
              <thead>
                <tr>
                  <th>{ds.country === "US" ? "State" : "Province"}</th>
                  {ds.years.map((yr) => (
                    <th key={yr}>{yr}</th>
                  ))}
                </tr>
              </thead>
              <tbody>
                {series.map((r, si) =>
                  r ? (
                    <tr key={r.name}>
                      <th>{r.name}</th>
                      {values[si]!.map((v, i) => (
                        <td key={i}>{v == null ? "" : formatNumber(v, decimals)}</td>
                      ))}
                    </tr>
                  ) : null,
                )}
              </tbody>
            </table>
          )}
        </div>
      </div>
    </div>
  );
}
