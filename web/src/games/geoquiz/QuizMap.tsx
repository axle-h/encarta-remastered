/**
 * The Geography Quiz map: the shared `<Atlas />` in quiz mode on Encarta's "Atlas Quiz" style (no
 * place names, no click menu), so the locator globe and zoom slider come from the atlas. The place
 * asked is filled yellow (a country or continent) or marked with a target ring (a city), with the
 * view fitted to it. A hand tool toggles dragging; the pointer tool leaves the map still.
 */
import { useMemo, useState } from "react";
import { Atlas } from "../../atlas";
import type { GeoPlace } from "../api";
import { viewFor } from "./view";

/** The highlight yellow of the VM's quiz map. */
export const HIGHLIGHT = "#fbf06c";

export function QuizMap({ place, hand = true, onHand }: { place: GeoPlace | null; hand?: boolean; onHand?(on: boolean): void }) {
  const [localHand, setLocalHand] = useState(true);
  const handOn = onHand ? hand : localHand;
  const focus = useMemo(() => {
    if (!place) return null;
    const v = viewFor(place);
    return v.bounds
      ? { lat: place.lat, lon: place.lon, bounds: v.bounds, maxZoom: place.kind === 2 ? 2.6 : 5.5 }
      : { lat: place.lat, lon: place.lon, zoom: v.zoom };
  }, [place]);
  const area = !!place && place.kind !== 1;
  const markerAt = useMemo(() => (place && !area ? { lat: place.lat, lon: place.lon } : null), [place, area]);
  const toggle = (on: boolean) => (onHand ? onHand(on) : setLocalHand(on));

  return (
    <div className="gq-map" aria-label="Quiz map">
      <Atlas
        className="gq-atlas"
        quizMode
        initialStyle="atlas-quiz"
        initialPlace={{ lat: 20, lon: 10 }}
        highlightEntity={area ? place!.entity : null}
        highlightColor={HIGHLIGHT}
        focus={focus}
        markerAt={markerAt}
        dragPan={handOn}
      />
      <div className="gq-tools" role="group" aria-label="Map tools">
        <button type="button" aria-pressed={handOn} title="Hand: drag to pan" onClick={() => toggle(true)}>
          Hand
        </button>
        <button type="button" aria-pressed={!handOn} title="Pointer: map stays put" onClick={() => toggle(false)}>
          Pointer
        </button>
      </div>
    </div>
  );
}
