/**
 * The statistics strip under the map (V14 shot): the chosen indicator, its source, and a chart of
 * every country's value sorted, in the map's colours.
 */
import { useMemo } from "react";
import type { StatIndicator, StatsIndex, StatValue } from "../data";

interface Props {
  index: StatsIndex | null;
  error: string | null;
  indicator: StatIndicator | null;
  values: StatValue[];
  colours: Map<number, string>;
  hoverCountry: number | null;
  onChoose: (id: number | null) => void;
}

export function StatsPanel({ index, error, indicator, values, colours, hoverCountry, onChoose }: Props) {
  const sorted = useMemo(() => values.filter((v) => typeof v.value === "number").sort((a, b) => (a.value as number) - (b.value as number)), [values]);
  const max = sorted.length ? Math.max(...sorted.map((v) => Math.abs(v.value as number))) : 1;
  const byCategory = useMemo(() => {
    const m = new Map<string, StatIndicator[]>();
    for (const ind of index?.indicators ?? []) {
      const c = ind.categories?.[0] ?? "Other";
      if (!m.has(c)) m.set(c, []);
      m.get(c)!.push(ind);
    }
    return [...m.entries()];
  }, [index]);
  const hovered = hoverCountry !== null ? values.find((v) => v.country_id === hoverCountry) : undefined;

  return (
    <div className="enc-atlas-stats">
      <svg className="enc-atlas-stats__chart" viewBox={`0 0 ${Math.max(1, sorted.length)} 100`} preserveAspectRatio="none" aria-hidden="true">
        {sorted.map((v, i) => {
          const h = (Math.abs(v.value as number) / (max || 1)) * 100;
          return <rect key={v.country_id} x={i} y={100 - h} width={1.02} height={h} fill={v.country_id === hoverCountry ? "#fff" : colours.get(v.country_id) ?? "#888"} />;
        })}
      </svg>
      <div className="enc-atlas-stats__text">
        {indicator ? (
          <>
            <strong>
              {indicator.name}
              {indicator.unit?.unit ? ` (${indicator.unit.unit})` : ""}
            </strong>
            {indicator.source && <span>[{indicator.source}]</span>}
          </>
        ) : (
          <strong>Choose a statistic to colour the countries.</strong>
        )}
        {hovered && (
          <span className="enc-atlas-stats__value">
            {hovered.country}: {hovered.value_text ?? hovered.value}
            {hovered.rank ? ` (rank ${hovered.rank})` : ""}
            {hovered.period ? `, ${hovered.period}` : hovered.year ? `, ${hovered.year}` : ""}
          </span>
        )}
        {error && <span className="enc-atlas-note">{error}</span>}
        <label className="enc-atlas-field">
          Choose Statistic
          <select value={indicator?.id ?? ""} onChange={(e) => onChoose(e.target.value ? Number(e.target.value) : null)}>
            <option value="">None</option>
            {byCategory.map(([cat, list]) => (
              <optgroup key={cat} label={cat}>
                {list.map((ind) => (
                  <option key={ind.id} value={ind.id}>
                    {ind.name}
                  </option>
                ))}
              </optgroup>
            ))}
          </select>
        </label>
      </div>
    </div>
  );
}
