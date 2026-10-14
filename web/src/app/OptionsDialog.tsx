import { useEffect, useState } from "react";
import { DEFAULT_SETTINGS, useSettings, type LatLongFormat, type Settings, type TextSize, type Units } from "../settings/settings";

export const OPTION_PAGES = ["General", "Encarta Updates", "Closed Captioning", "World Atlas", "Quality"] as const;
type Page = (typeof OPTION_PAGES)[number];

function Radio<T extends string>({ name, value, current, label, onPick }: { name: string; value: T; current: T; label: string; onPick: (v: T) => void }) {
  return (
    <label>
      <input type="radio" name={name} checked={current === value} onChange={() => onPick(value)} /> {label}
    </label>
  );
}

/**
 * Tools > Options: five pages listed at the left, the page's controls at the right. Changes
 * apply on OK; Cancel drops them. Encarta Updates and Quality are inert (no update service, no
 * customer-experience program).
 */
export function OptionsDialog({ onClose }: { onClose: () => void }) {
  const { settings, update } = useSettings();
  const [draft, setDraft] = useState<Settings>({ ...DEFAULT_SETTINGS, ...settings });
  const [page, setPage] = useState<Page>("General");
  const [ceip, setCeip] = useState(false);
  const [checked, setChecked] = useState(false);
  const set = <K extends keyof Settings>(k: K, v: Settings[K]) => setDraft((d) => ({ ...d, [k]: v }));
  useEffect(() => {
    const key = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    document.addEventListener("keydown", key);
    return () => document.removeEventListener("keydown", key);
  }, [onClose]);

  const check = (k: "startupSound" | "menuSounds" | "captions" | "atlasLinearStats" | "atlasCrosshairs", label: string) => (
    <label>
      <input type="checkbox" checked={draft[k]} onChange={(e) => set(k, e.target.checked)} /> {label}
    </label>
  );

  return (
    <div className="dlg-backdrop">
      <div className="dlg opt" role="dialog" aria-modal="true" aria-label="Options">
        <ul className="opt-pages" role="tablist" aria-orientation="vertical">
          {OPTION_PAGES.map((p) => (
            <li key={p} role="presentation">
              <button type="button" role="tab" aria-selected={page === p} className={page === p ? "on" : ""} onClick={() => setPage(p)}>
                {p}
              </button>
            </li>
          ))}
        </ul>
        <div className="opt-body" role="tabpanel" aria-label={page}>
          <h2 className="dlg-title">
            <span>{page}</span> <b>Options</b>
          </h2>
          {page === "General" && (
            <>
              <fieldset className="dlg-group">
                <legend>Sounds</legend>
                {check("startupSound", "Play startup sound")}
                {check("menuSounds", "Play menu and button sounds")}
              </fieldset>
              <fieldset className="dlg-group">
                <legend>Article text size</legend>
                {(["small", "medium", "large"] as TextSize[]).map((v) => (
                  <Radio key={v} name="textSize" value={v} current={draft.textSize} label={v.charAt(0).toUpperCase() + v.slice(1)} onPick={(x) => set("textSize", x)} />
                ))}
              </fieldset>
            </>
          )}
          {page === "Encarta Updates" && (
            <fieldset className="dlg-group">
              <legend>Updates</legend>
              <p>Encarta's update service closed in 2009.</p>
              <button type="button" onClick={() => setChecked(true)}>
                Check for Updates
              </button>
              {checked && <p role="status">No updates are available.</p>}
            </fieldset>
          )}
          {page === "Closed Captioning" && (
            <fieldset className="dlg-group">
              <legend>Captions</legend>
              {check("captions", "Show captions for audio and video")}
            </fieldset>
          )}
          {page === "World Atlas" && (
            <>
              <fieldset className="dlg-group">
                <legend>Display</legend>
                {check("atlasLinearStats", "Always show statistical data in linear form")}
                {check("atlasCrosshairs", "Show crosshairs at the map centre")}
              </fieldset>
              <fieldset className="dlg-group">
                <legend>Latitude and longitude</legend>
                {(
                  [
                    ["dm", "Degrees, minutes"],
                    ["dms", "Degrees, minutes, seconds"],
                    ["decimal", "Decimal"],
                  ] as [LatLongFormat, string][]
                ).map(([v, l]) => (
                  <Radio key={v} name="latlong" value={v} current={draft.latLongFormat} label={l} onPick={(x) => set("latLongFormat", x)} />
                ))}
              </fieldset>
              <fieldset className="dlg-group">
                <legend>Units</legend>
                {(
                  [
                    ["metric", "Metric"],
                    ["us", "U.S."],
                  ] as [Units, string][]
                ).map(([v, l]) => (
                  <Radio key={v} name="units" value={v} current={draft.units} label={l} onPick={(x) => set("units", x)} />
                ))}
              </fieldset>
            </>
          )}
          {page === "Quality" && (
            <fieldset className="dlg-group">
              <legend>Customer experience</legend>
              <label>
                <input type="checkbox" checked={ceip} onChange={(e) => setCeip(e.target.checked)} /> Join the Customer Experience Improvement Program
              </label>
              <p>Nothing is sent from this copy of Encarta.</p>
            </fieldset>
          )}
          <div className="dlg-buttons">
            <button
              type="button"
              onClick={() => {
                update(draft);
                onClose();
              }}
            >
              OK
            </button>
            <button type="button" onClick={onClose}>
              Cancel
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
