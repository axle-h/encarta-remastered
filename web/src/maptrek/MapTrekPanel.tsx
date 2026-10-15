/**
 * The modeless "Map Trek" dialog (docs/vm-findings.md, Map Trek): a caption bar with a close
 * button, brown body, "map trek" banner, a white text box (trek title, thumbnail and caption,
 * place name, text), a stop list with a "Stop n of N" footer, and New Trek / Back / Next buttons,
 * at the VM's sizes (450 px wide). Presentational: the page owns the stop.
 */
import type { ReactNode } from "react";
import "./MapTrek.css";

export interface TrekStopRow {
  id: number;
  title: string;
}

export interface MapTrekPanelProps {
  trekTitle: string;
  stops: TrekStopRow[];
  /** Index of the current stop in `stops`, or -1. */
  index: number;
  thumbSrc?: string;
  /** Caption HTML, already rendered by the caller. */
  caption?: ReactNode;
  /** The stop's name and text. */
  children?: ReactNode;
  onSelect: (index: number) => void;
  onNewTrek: () => void;
  /** The caption bar's close button (VM: the map stays at the stop). */
  onClose?: () => void;
}

export function MapTrekPanel({ trekTitle, stops, index, thumbSrc, caption, children, onSelect, onNewTrek, onClose }: MapTrekPanelProps) {
  const n = stops.length;
  return (
    <div className="mt-dialog" role="dialog" aria-label="Map Trek" aria-modal="false">
      {onClose && (
        <div className="mt-caption">
          <span>Map Trek</span>
          <button type="button" className="mt-close" aria-label="Close Map Trek" onClick={onClose}>
            ×
          </button>
        </div>
      )}
      <div className="mt-banner">map trek</div>
      <div className="mt-body">
        <div className="mt-text">
          <h2 className="mt-trek-title">{trekTitle}</h2>
          <div className={`mt-stop${thumbSrc ? " has-thumb" : ""}`}>
            {thumbSrc && (
              <figure className="mt-thumb">
                <img src={thumbSrc} alt="" width={64} height={64} />
                {caption && <figcaption>{caption}</figcaption>}
              </figure>
            )}
            <div className="mt-stop-text">{children}</div>
          </div>
        </div>
        <div className="mt-list">
          <ol>
            {stops.map((s, i) => (
              <li key={s.id}>
                <button type="button" className={i === index ? "current" : ""} aria-current={i === index ? "step" : undefined} onClick={() => onSelect(i)}>
                  {s.title}
                </button>
              </li>
            ))}
          </ol>
          <div className="mt-count">{index >= 0 ? `Stop ${index + 1} of ${n}` : `${n} stops`}</div>
        </div>
      </div>
      <div className="mt-buttons">
        <button type="button" className="mt-button" onClick={onNewTrek}>
          New Trek
        </button>
        <span className="mt-spacer" />
        <button type="button" className="mt-button" disabled={index <= 0} onClick={() => onSelect(index - 1)}>
          Back
        </button>
        <button type="button" className="mt-button" disabled={index < 0 || index >= n - 1} onClick={() => onSelect(index + 1)}>
          Next
        </button>
      </div>
    </div>
  );
}
