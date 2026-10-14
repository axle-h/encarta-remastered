/** Readout formats for the Options dialog: lat/long notation and metric/US distances and areas. */
import type { LatLongFormat, Units } from "../settings/settings";

export const KM_PER_MILE = 1.609344;
const SQKM_PER_SQMI = KM_PER_MILE * KM_PER_MILE;

/** One coordinate: "dm" is 12°30′ N, "dms" 12°30′15″ N, "decimal" 12.5042° N. */
export function formatCoord(value: number, axis: "lat" | "lon", fmt: LatLongFormat): string {
  const hemi = axis === "lat" ? (value < 0 ? "S" : "N") : value < 0 ? "W" : "E";
  const abs = Math.abs(value);
  if (fmt === "decimal") return `${abs.toFixed(4)}° ${hemi}`;
  if (fmt === "dm") {
    let d = Math.floor(abs);
    let m = Math.round((abs - d) * 60);
    if (m === 60) {
      d += 1;
      m = 0;
    }
    return `${d}°${String(m).padStart(2, "0")}′ ${hemi}`;
  }
  let d = Math.floor(abs);
  let m = Math.floor((abs - d) * 60);
  let s = Math.round(((abs - d) * 60 - m) * 60);
  if (s === 60) {
    s = 0;
    m += 1;
  }
  if (m === 60) {
    m = 0;
    d += 1;
  }
  return `${d}°${String(m).padStart(2, "0")}′${String(s).padStart(2, "0")}″ ${hemi}`;
}

export function formatLatLong(lat: number, lon: number, fmt: LatLongFormat): string {
  return `${formatCoord(lat, "lat", fmt)}, ${formatCoord(lon, "lon", fmt)}`;
}

const group = (n: number) => Math.round(n).toLocaleString("en-US");

/** A distance given in km, in km or miles. */
export function formatDistance(km: number, units: Units): string {
  const v = units === "us" ? km / KM_PER_MILE : km;
  const u = units === "us" ? "mi" : "km";
  if (v >= 100) return `${group(v)} ${u}`;
  return `${v.toFixed(v >= 10 ? 0 : 1)} ${u}`;
}

/** An area given in square km, in sq km or sq mi. */
export function formatArea(sqKm: number, units: Units): string {
  return units === "us" ? `${group(sqKm / SQKM_PER_SQMI)} sq mi` : `${group(sqKm)} sq km`;
}
