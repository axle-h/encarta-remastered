import type { TimelineDate, TimelineEvent } from "../api/types";

const MONTHS = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];

/** "c. 3000 BC", "1789", "July 14, 1789", "AD 79" (AD only for years below 1000). */
export function formatDate(d: TimelineDate): string {
  const y = d.year;
  let year: string;
  if (y < 0) {
    const a = -y;
    year = a >= 1_000_000 ? `${(a / 1_000_000).toLocaleString("en-US", { maximumFractionDigits: 1 })} million years ago` : `${a.toLocaleString("en-US")} BC`;
  } else if (y < 1000) year = `AD ${y}`;
  else year = String(y);
  const circa = d.circa ? "c. " : "";
  if (d.month && d.month >= 1 && d.month <= 12) {
    const m = MONTHS[d.month - 1]!;
    return `${circa}${d.day ? `${m} ${d.day}, ` : `${m} `}${year}`;
  }
  return `${circa}${year}`;
}

export function formatEventDate(e: Pick<TimelineEvent, "start" | "end">): string {
  const s = formatDate(e.start);
  if (!e.end || (e.end.year === e.start.year && e.end.month === e.start.month && e.end.day === e.start.day)) return s;
  return `${s} – ${formatDate(e.end)}`;
}

/** The pop-up date line: the converter's `date_text` ("4? BC - AD 29?", timeline.md §3.2), else our own wording. */
export function eventDateText(e: Pick<TimelineEvent, "start" | "end" | "date_text">): string {
  return e.date_text ?? formatEventDate(e);
}

/** Year label for an axis tick. */
export function yearLabel(y: number): string {
  if (y < 0) {
    const a = -y;
    if (a >= 1_000_000_000) return `${a / 1_000_000_000} bya`;
    if (a >= 1_000_000) return `${a / 1_000_000} mya`;
    return `${a.toLocaleString("en-US")} BC`;
  }
  if (y === 0) return "AD 1";
  return y < 1000 ? `AD ${y}` : String(y);
}
