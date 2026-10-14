/**
 * `<Atlas />`: Encarta's World Atlas on MapLibre GL JS. Globe and flat views over the converter's
 * styles (docs/atlas-output.md), the style carousel, the Map Customizer, the Map Legend, place search,
 * hover highlights and the click menu with each place's articles, and the statistics overlay.
 * Design notes: docs/atlas-client.md.
 */
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import maplibregl, { type Map as MlMap, type MapMouseEvent, type StyleSpecification } from "maplibre-gl";
import { Protocol } from "pmtiles";
import "maplibre-gl/dist/maplibre-gl.css";
import "./atlas.css";

import { MENU_STYLE, findStyle, styleBody, styleLabel, type StyleIndexEntry } from "./catalog";
import {
  dataFile,
  defaultEndpoints,
  fetchJson,
  getPlace,
  getPlaceLinks,
  loadIndicator,
  loadPalettes,
  loadStats,
  loadStyle,
  loadStyleIndex,
  makeTransformRequest,
  primaryArticle,
  type Endpoints,
  type Palettes,
  type PlaceHit,
  type PlaceLink,
  type StatIndicator,
  type StatsIndex,
  type StatValue,
} from "./data";
import { hoverClassesAt, hoverTarget, rankHits, viewTitle, type HitPlace, type QueriedFeature } from "./hit";
import { HIT_PREFIX, hitClassesAt, hitRanges, vectorSources, type ClassOverrides } from "./layers";
import {
  MILLER_MVT_PROTOCOL,
  MILLER_RASTER_PROTOCOL,
  createMillerRasterProtocol,
  createMillerVectorProtocol,
  fromViewLat,
  toViewLat,
  zoomToFitHeight,
  type MillerImage,
} from "./miller";
import { LOOK_PROTOCOL, bakedLooks, createLookProtocol } from "./recolour";
import { useSettings } from "../settings/settings";
import { formatLatLong } from "./format";
import { colourValues, DEFAULT_RAMP, statRamp } from "./stats";
import { buildStyle, defaultVisibleClasses, styleMeta, type Projection, type StyleMeta } from "./style";
import { altitudeForView, altitudeForZoom, formatAltitude, maxZoomAt, pOverMForPane, parseMsEncartaUrl, quantiseShift, zoomForView, zoomShift, DEFAULT_P_OVER_M } from "./zoom";
import { LocatorGlobe } from "./ui/LocatorGlobe";
import { CustomizerPanel, LegendPanel, Panel } from "./ui/Panels";
import { PlaceMenu } from "./ui/PlaceMenu";
import { SearchBox } from "./ui/SearchBox";
import { SpriteContext, useSpriteSheet } from "./ui/Sprite";
import { StatsPanel } from "./ui/StatsPanel";
import { StylePicker } from "./ui/StylePicker";

// ---------------------------------------------------------------------------------------------
// Public API

/** Where to open the atlas: a gazetteer entity (the `gid` of `/atlas/place/{gid}` links), a point and altitude, or a `msencarta://` map URL. */
export type AtlasPlace = { entity?: number; lat?: number; lon?: number; altitudeKm?: number } | string;

export interface AtlasLink {
  /** Content id to open (`/item/{refId}`); an article for `kind_label === "article"`, else a media item. */
  refId: number;
  kind: number;
  kindLabel: string;
  title?: string | null;
  itemKind?: string | null;
  /** The place the link belongs to. */
  entity: number;
  placeName: string;
}

export interface AtlasViewState {
  lat: number;
  lon: number;
  altitudeKm: number;
  zoom: number;
  /** Slug of the style (`atlas-political`). */
  style: string;
  styleId: number;
  projection: Projection;
  title: string;
  /** Entity of the place named in the title, when there is one. */
  entity?: number;
}

export interface AtlasProps {
  initialPlace?: AtlasPlace;
  /** MapStyle id, slug (`atlas-political`), MapStyle name or carousel caption ("Political"). Default: Atlas Default. */
  initialStyle?: string | number;
  initialProjection?: Projection;
  /** Opens an article or media item linked to a place (P2MTBL). Default: navigate to `/item/{refId}`. */
  onOpenArticle?: (refId: number, link: AtlasLink) => void;
  /** Called after each move or style change (for keeping a URL in sync). */
  onViewChange?: (view: AtlasViewState) => void;
  /** Fill and outline this entity (gazetteer id = MVT feature id), e.g. a quiz question's country. */
  highlightEntity?: number | null;
  /** Colour of `highlightEntity` (default Encarta's highlight yellow). */
  highlightColor?: string;
  /**
   * Quiz mode: no place names (labels, title, tooltips) and no click menu; clicks go to
   * `onPlaceClick` only. The symbols and the map styling stay.
   */
  quizMode?: boolean;
  /** Every click on the map, with the places under the point (in the hit list's order). */
  onPlaceClick?: (hits: HitPlace[], lngLat: { lat: number; lon: number }) => void;
  /** Move the view to this box (`[[west, south], [east, north]]`) or point (with `zoom`); a new object each time re-centres. */
  focus?: { lat: number; lon: number; bounds?: [[number, number], [number, number]]; zoom?: number; maxZoom?: number; instant?: boolean } | null;
  /** A target ring at this point (e.g. a quiz city). */
  markerAt?: { lat: number; lon: number } | null;
  /** Dragging pans the map (default true); false is the quiz's pointer tool. */
  dragPan?: boolean;
  /** Base URL of `out/atlas/` (default `/out/atlas`). */
  dataUrl?: string;
  /** Base URL of the JSON API (default `/api`). */
  apiUrl?: string;
  className?: string;
}

// ---------------------------------------------------------------------------------------------
// Protocols (global in MapLibre: registered once per page)

let protocolsReady = false;
let protocolEndpoints: Endpoints = defaultEndpoints();
let protocolPalettes: Palettes | null = null;
let protocolMiller: MillerImage[] = [];

function registerProtocols(): void {
  if (protocolsReady) return;
  const pmtiles = new Protocol({ metadata: true });
  maplibregl.addProtocol("pmtiles", pmtiles.tile);
  maplibregl.addProtocol(LOOK_PROTOCOL, createLookProtocol(() => ({ palettes: protocolPalettes, ep: protocolEndpoints })));
  maplibregl.addProtocol(MILLER_MVT_PROTOCOL, createMillerVectorProtocol());
  maplibregl.addProtocol(
    MILLER_RASTER_PROTOCOL,
    createMillerRasterProtocol(() => ({
      images: protocolMiller,
      imageUrl: (raster, look, level) => dataFile(`miller/${raster.toLowerCase()}/${look.toLowerCase()}-${level}.webp`, protocolEndpoints),
    })),
  );
  protocolsReady = true;
}

const EMPTY: ClassOverrides = { on: new Set(), off: new Set() };
const ENTITY_LAYERS = ["areas", "lines", "points", "labels", "outlines"];

/**
 * The style's layers and sources exist (after `style.load`). Unlike `map.isStyleLoaded()`, this does
 * not wait for tiles, so feature-state can be set while they load.
 */
function styleReady(map: MlMap): boolean {
  return (map as unknown as { style?: { _loaded?: boolean } }).style?._loaded === true;
}

function setEntityState(map: MlMap, sources: string[], entity: number, state: Record<string, unknown>): void {
  for (const source of sources) {
    if (!map.getSource(source)) continue;
    for (const sourceLayer of ENTITY_LAYERS) map.setFeatureState({ source, sourceLayer, id: entity }, state);
  }
}

function resolveInitial(place: AtlasPlace | undefined): { lat?: number; lon?: number; altitudeKm?: number; entity?: number; menuStyle?: number } {
  if (!place) return {};
  if (typeof place === "string") return parseMsEncartaUrl(place) ?? {};
  return place;
}

function linkOf(l: PlaceLink, entity: number, placeName: string): AtlasLink {
  return { refId: l.ref_id, kind: l.kind, kindLabel: l.kind_label, title: l.title, itemKind: l.item_kind, entity, placeName };
}

// ---------------------------------------------------------------------------------------------

export function Atlas({
  initialPlace,
  initialStyle,
  initialProjection = "globe",
  onOpenArticle,
  onViewChange,
  highlightEntity,
  highlightColor = "#ffd400",
  quizMode = false,
  onPlaceClick,
  focus,
  markerAt,
  dragPan = true,
  dataUrl,
  apiUrl,
  className,
}: AtlasProps) {
  const ep = useMemo(() => defaultEndpoints({ dataUrl, apiUrl }), [dataUrl, apiUrl]);
  const initial = useMemo(() => resolveInitial(initialPlace), [initialPlace]);

  const container = useRef<HTMLDivElement>(null);
  const mapRef = useRef<MlMap | null>(null);
  const [mapReady, setMapReady] = useState<MlMap | null>(null);
  const rawLoaded = useRef<StyleSpecification | null>(null);

  const [index, setIndex] = useState<StyleIndexEntry[] | null>(null);
  const [palettes, setPalettes] = useState<Palettes | null>(null);
  const [entry, setEntry] = useState<StyleIndexEntry | undefined>();
  const [raw, setRaw] = useState<StyleSpecification | null>(null);
  const [error, setError] = useState<string | null>(null);

  const [projection, setProjection] = useState<Projection>(initialProjection);
  const [overrides, setOverrides] = useState<ClassOverrides>(EMPTY);
  const [labelScale, setLabelScale] = useState(1);
  const [lookOverride, setLookOverride] = useState<string | undefined>();
  const { settings } = useSettings();
  const [units, setUnits] = useState<"imperial" | "metric">(settings.units === "us" ? "imperial" : "metric");
  // The Options dialog's choice wins whenever it changes; the Customizer can still override it.
  useEffect(() => setUnits(settings.units === "us" ? "imperial" : "metric"), [settings.units]);
  const unitSys = units === "imperial" ? "us" : "metric";
  const [hoverHighlight, setHoverHighlight] = useState(false);
  const [paneHeight, setPaneHeight] = useState(0);
  /** Real latitude of the view centre at the last move end (for the zoom shift). */
  const [shiftLat, setShiftLat] = useState(initial.lat ?? 30);

  const [zoom, setZoom] = useState(2);
  const [center, setCenter] = useState<{ lat: number; lon: number }>({ lat: initial.lat ?? 20, lon: initial.lon ?? 0 });
  const [outline, setOutline] = useState<[number, number][] | null>(null);
  const [title, setTitle] = useState<{ title: string; subtitle?: string; entity?: number }>({ title: "World" });
  const [titleArticle, setTitleArticle] = useState<PlaceLink | undefined>();
  const [tooltip, setTooltip] = useState<{ x: number; y: number; text: string } | null>(null);
  const [menu, setMenu] = useState<{ x: number; y: number; hits: HitPlace[]; lngLat: [number, number] } | null>(null);
  const [panel, setPanel] = useState<"legend" | "customizer" | "about" | null>(null);
  const [selected, setSelected] = useState<number | null>(initial.entity ?? null);

  const [statsIndex, setStatsIndex] = useState<StatsIndex | null>(null);
  const [statsError, setStatsError] = useState<string | null>(null);
  const [indicator, setIndicator] = useState<StatIndicator | null>(null);
  const [statValues, setStatValues] = useState<StatValue[]>([]);
  const [hoverCountry, setHoverCountry] = useState<number | null>(null);
  const [inView, setInView] = useState<Set<number> | null>(null);

  const sprite = useSpriteSheet(ep);
  const meta: StyleMeta | null = useMemo(() => (raw ? styleMeta(raw) : null), [raw]);
  const drawn = useMemo(() => (raw ? defaultVisibleClasses(raw) : new Set<number>()), [raw]);
  const stylePOverM = meta?.pOverM ?? DEFAULT_P_OVER_M;
  // Encarta's P/m grows with the map pane height (VM V1); the styles are compiled for its 518-px pane.
  const pOverM = paneHeight > 0 ? pOverMForPane(paneHeight, meta?.pOverMPerHeightPx) : stylePOverM;
  const flat = projection === "flat";
  const shift = quantiseShift(zoomShift(flat, pOverM, stylePOverM, flat ? 0 : shiftLat));
  const ranges = useMemo(() => (raw ? hitRanges(raw, overrides) : new Map()), [raw, overrides]);
  const body = entry ? styleBody(entry) : "earth";
  const isStatistical = entry?.id === 12;

  // Latest values for map event handlers.
  const live = useRef({
    drawn,
    raw,
    built: null as StyleSpecification | null,
    meta,
    zoom,
    body,
    selected,
    hover: null as number | null,
    colours: new Map<number, string>(),
    onViewChange,
    entry,
    projection,
    title,
    pOverM,
    shift,
    ranges,
    hoverHighlight,
    paneHeight,
    quizMode,
    onPlaceClick,
  });
  live.current = { ...live.current, drawn, raw, meta, zoom, body, selected, onViewChange, entry, projection, title, pOverM, shift, ranges, hoverHighlight, paneHeight, quizMode, onPlaceClick };
  /** Real latitude ↔ the map's latitude (the flat view draws Miller in view latitudes, miller.ts). */
  const toMapLat = useCallback((lat: number) => (live.current.projection === "flat" ? toViewLat(lat) : lat), []);
  const fromMapLat = useCallback((lat: number) => (live.current.projection === "flat" ? fromViewLat(lat) : lat), []);

  // --- data -------------------------------------------------------------------------------
  useEffect(() => {
    protocolEndpoints = ep;
    const ac = new AbortController();
    Promise.all([
      loadStyleIndex(ep, ac.signal),
      loadPalettes(ep, ac.signal).catch(() => null),
      fetchJson<MillerImage[]>(dataFile("miller/index.json", ep), ac.signal).catch(() => []),
    ])
      .then(([idx, pal, miller]) => {
        protocolPalettes = pal;
        protocolMiller = miller;
        setPalettes(pal);
        setIndex(idx);
        const fromMenu = initial.menuStyle !== undefined ? MENU_STYLE[initial.menuStyle] : undefined;
        setEntry(findStyle(idx, initialStyle ?? fromMenu));
      })
      .catch((e: unknown) => !ac.signal.aborted && setError(`Atlas data not available (${String(e instanceof Error ? e.message : e)}).`));
    return () => ac.abort();
    // initialStyle is read once; later changes go through the style picker.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [ep]);

  useEffect(() => {
    if (!entry) return;
    const ac = new AbortController();
    loadStyle(entry, ep, ac.signal)
      .then((s) => {
        setRaw(s);
        setOverrides(EMPTY);
        setLookOverride(undefined);
      })
      .catch((e: unknown) => !ac.signal.aborted && setError(`Style ${entry.name} not available (${String(e)}).`));
    return () => ac.abort();
  }, [entry, ep]);

  useEffect(() => {
    if (!isStatistical || statsIndex) return;
    const ac = new AbortController();
    loadStats(ep, ac.signal)
      .then(setStatsIndex)
      .catch(() => !ac.signal.aborted && setStatsError("Statistics are not available on this server."));
    return () => ac.abort();
  }, [isStatistical, statsIndex, ep]);

  // --- the style the map shows ---------------------------------------------------------------
  const baked = useMemo(() => (index ? bakedLooks(index) : new Set<string>()), [index]);
  const look = useMemo(() => {
    if (!lookOverride || !meta?.raster) return null;
    return { raster: meta.raster, look: lookOverride, baked: baked.has(`${meta.raster.toUpperCase()}/${lookOverride.toUpperCase()}`) };
  }, [lookOverride, meta, baked]);
  const statsOn = isStatistical && !!indicator;
  const built = useMemo(
    () =>
      raw
        ? buildStyle(raw, {
            ep,
            projection,
            overrides,
            labelScale: quizMode ? 0 : labelScale,
            look,
            palettes,
            stats: statsOn,
            shift,
            highlight: highlightEntity != null ? { entity: highlightEntity, color: highlightColor } : null,
          })
        : null,
    [raw, ep, projection, overrides, labelScale, look, palettes, statsOn, shift, quizMode, highlightEntity, highlightColor],
  );
  live.current.built = built;
  const ramp = useMemo(() => {
    const r = raw ? statRamp(raw) : DEFAULT_RAMP;
    return r.positive.length ? r : DEFAULT_RAMP;
  }, [raw]);
  const statColours = useMemo(() => colourValues(statValues, ramp, settings.atlasLinearStats), [statValues, ramp, settings.atlasLinearStats]);
  live.current.colours = statsOn ? statColours : new Map();

  const sourcesOf = useCallback((s: StyleSpecification | null) => (s ? vectorSources(s) : []), []);

  const applyStates = useCallback(() => {
    const map = mapRef.current;
    const s = live.current.raw;
    if (!map || !s || !styleReady(map)) return;
    const sources = sourcesOf(s);
    if (map.getSource("earth")) map.removeFeatureState({ source: "earth", sourceLayer: "areas" });
    for (const [id, colour] of live.current.colours) map.setFeatureState({ source: "earth", sourceLayer: "areas", id }, { stat: colour });
    for (const id of [live.current.selected, live.current.hover]) if (id !== null) setEntityState(map, sources, id, { hover: true });
  }, [sourcesOf]);

  const updateZoomLimits = useCallback(() => {
    const map = mapRef.current;
    const m = live.current.meta;
    if (!map || !m) return;
    const c = map.getCenter();
    const { shift: s, projection: p, paneHeight: h } = live.current;
    // The style's altitude limits, moved like its layers; the flat view opens on the whole world
    // fitted to the pane height (V12) and goes no further out.
    const min = m.minzoom + s;
    map.setMinZoom(Math.max(0, p === "flat" && h > 0 ? Math.max(min, zoomToFitHeight(h)) : min));
    map.setMaxZoom(maxZoomAt(c.lng, fromMapLat(c.lat), m.maxzoom, m.minRects) + s);
  }, [fromMapLat]);

  const hitsAt = useCallback((point: { x: number; y: number }, pad = 3): QueriedFeature[] => {
    const map = mapRef.current;
    const s = map?.getStyle();
    if (!map || !s) return [];
    const ids = s.layers.filter((l) => l.id.startsWith(HIT_PREFIX) || l.type === "symbol" || l.type === "line").map((l) => l.id);
    return map.queryRenderedFeatures(
      [
        [point.x - pad, point.y - pad],
        [point.x + pad, point.y + pad],
      ],
      { layers: ids },
    ) as unknown as QueriedFeature[];
  }, []);

  const iconOf = useCallback((f: QueriedFeature) => {
    const map = mapRef.current;
    if (!map || !f.layer || f.layer.type !== "symbol") return undefined;
    const v = map.getLayoutProperty(f.layer.id, "icon-image");
    return typeof v === "string" ? v : undefined;
  }, []);

  const refreshTitle = useCallback(() => {
    const map = mapRef.current;
    if (!map) return;
    const c = map.project(map.getCenter());
    const hits = rankHits(hitsAt(c, 1));
    const ll = map.getCenter();
    const lat = fromMapLat(ll.lat);
    const alt = altitudeForView(map.getZoom(), live.current.projection === "flat", live.current.pOverM, lat);
    const t = viewTitle(hits, alt, live.current.body);
    setTitle((prev) => (prev.title === t.title && prev.subtitle === t.subtitle && prev.entity === t.entity ? prev : t));
    const e = live.current.entry;
    live.current.onViewChange?.({
      lat,
      lon: ll.lng,
      altitudeKm: alt,
      zoom: map.getZoom(),
      style: e ? e.file.replace(/^.*\//, "").replace(/\.json$/, "") : "",
      styleId: e?.id ?? 0,
      projection: live.current.projection,
      title: t.title,
      entity: t.entity,
    });
  }, [hitsAt, fromMapLat]);

  // --- create the map once the first style is there; then diff or replace ---------------------
  useEffect(() => {
    if (!built || !container.current || !meta) return;
    registerProtocols();
    let map = mapRef.current;
    if (!map) {
      const lat0 = initial.lat ?? 30;
      const z = initial.altitudeKm ? zoomForView(initial.altitudeKm, flat, pOverM, lat0) : 2;
      map = new maplibregl.Map({
        container: container.current,
        style: built,
        center: [initial.lon ?? 10, flat ? toViewLat(lat0) : lat0],
        zoom: Math.min(Math.max(z, meta.minzoom + shift), meta.maxzoom + shift),
        minZoom: 0,
        maxZoom: 22,
        transformRequest: makeTransformRequest(ep),
        attributionControl: false,
        fadeDuration: 0,
        maxPitch: 0,
        dragRotate: false,
        pitchWithRotate: false,
        renderWorldCopies: true,
      });
      map.touchZoomRotate.disableRotation();
      map.keyboard.disableRotation();
      mapRef.current = map;
      setMapReady(map);
      rawLoaded.current = raw;
      if (typeof window !== "undefined") (window as unknown as { __encartaAtlas?: MlMap }).__encartaAtlas = map;
      map.on("style.load", () => {
        updateZoomLimits();
        applyStates();
      });
      map.on("load", () => {
        updateZoomLimits();
        refreshTitle();
      });
      map.on("idle", () => {
        container.current?.setAttribute("data-idle", "1");
        refreshTitle(); // after a style change, once its tiles are in
      });
      map.on("dataloading", () => container.current?.removeAttribute("data-idle"));
      map.on("error", (e) => console.warn("atlas:", e.error?.message ?? e));
    } else if (rawLoaded.current !== raw) {
      rawLoaded.current = raw;
      map.setStyle(built, { diff: false });
    } else {
      map.setStyle(built, { diff: true });
      map.once("styledata", applyStates);
    }
    container.current.style.background = meta.spaceColour;
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [built]);

  useEffect(
    () => () => {
      mapRef.current?.remove();
      mapRef.current = null;
      setMapReady(null);
    },
    [],
  );

  useEffect(() => {
    if (meta && mapRef.current && styleReady(mapRef.current)) updateZoomLimits();
  }, [meta, updateZoomLimits]);

  // Move, hover and click handlers.
  useEffect(() => {
    const map = mapReady;
    if (!map) return;
    let frame = 0;
    const onMove = () => {
      if (frame) return;
      frame = requestAnimationFrame(() => {
        frame = 0;
        const c = map.getCenter();
        const lat = fromMapLat(c.lat);
        setZoom(map.getZoom());
        setCenter({ lat, lon: c.lng });
        const alt = altitudeForView(map.getZoom(), live.current.projection === "flat", live.current.pOverM, lat);
        if (alt >= 14000) setOutline(null);
        else {
          const { clientWidth: w, clientHeight: h } = map.getContainer();
          const pts: [number, number][] = [];
          for (const [x, y] of [
            [0, 0],
            [w / 2, 0],
            [w, 0],
            [w, h / 2],
            [w, h],
            [w / 2, h],
            [0, h],
            [0, h / 2],
          ] as const) {
            const ll = map.unproject([x, y]);
            pts.push([ll.lng, fromMapLat(ll.lat)]);
          }
          setOutline(pts);
        }
      });
    };
    const onMoveEnd = () => {
      updateZoomLimits();
      refreshTitle();
      setShiftLat(fromMapLat(map.getCenter().lat));
    };
    /** Classes in the hit list at the current zoom (the ranges are in the style's own zooms). */
    const allowed = () => hitClassesAt(live.current.ranges, map.getZoom() - live.current.shift);
    let hoverFrame = 0;
    let last: MapMouseEvent | null = null;
    const setHover = (entity: number | null) => {
      const prev = live.current.hover;
      if (prev === entity) return;
      const sources = sourcesOf(live.current.raw);
      if (prev !== null && prev !== live.current.selected) setEntityState(map, sources, prev, { hover: false });
      if (entity !== null) setEntityState(map, sources, entity, { hover: true });
      live.current.hover = entity;
    };
    const onMouseMove = (e: MapMouseEvent) => {
      last = e;
      if (hoverFrame) return;
      hoverFrame = requestAnimationFrame(() => {
        hoverFrame = 0;
        if (!last || !live.current.raw) return;
        const hits = rankHits(hitsAt(last.point), iconOf, allowed());
        // Encarta highlights nothing on hover (V11); the Customizer can turn it on.
        const target = live.current.hoverHighlight && live.current.built ? hoverTarget(hits, hoverClassesAt(live.current.built, map.getZoom())) : undefined;
        setHover(target?.entity ?? null);
        const country = hits.find((h) => /^(Sovereign|CountryRegion)/.test(h.className) && live.current.colours.has(h.entity));
        setHoverCountry(country ? country.entity : null);
        // A statistics readout over a coloured country (V14); place names only with the hover option.
        const top = live.current.quizMode ? undefined : (country ?? (live.current.hoverHighlight ? hits[0] : undefined));
        setTooltip(top ? { x: last.point.x, y: last.point.y, text: top.name } : null);
        map.getCanvas().style.cursor = hits.length ? "pointer" : "";
      });
    };
    const onMouseOut = () => {
      last = null;
      setHover(null);
      setTooltip(null);
      setHoverCountry(null);
    };
    const onClick = (e: MapMouseEvent) => {
      const hits = rankHits(hitsAt(e.point), iconOf, allowed());
      setTooltip(null);
      live.current.onPlaceClick?.(hits, { lat: fromMapLat(e.lngLat.lat), lon: e.lngLat.lng });
      if (!hits.length || live.current.quizMode) {
        setMenu(null);
        return;
      }
      setMenu({ x: e.point.x, y: e.point.y, hits, lngLat: [e.lngLat.lng, fromMapLat(e.lngLat.lat)] });
    };
    map.on("move", onMove);
    map.on("moveend", onMoveEnd);
    map.on("mousemove", onMouseMove);
    map.on("mouseout", onMouseOut);
    map.on("click", onClick);
    map.on("movestart", () => setMenu(null));
    onMove();
    return () => {
      map.off("move", onMove);
      map.off("moveend", onMoveEnd);
      map.off("mousemove", onMouseMove);
      map.off("mouseout", onMouseOut);
      map.off("click", onClick);
      cancelAnimationFrame(frame);
      cancelAnimationFrame(hoverFrame);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [mapReady]);

  // Scale bar with the chosen units.
  useEffect(() => {
    const map = mapReady;
    if (!map) return;
    const scale = new maplibregl.ScaleControl({ unit: units, maxWidth: 140 });
    map.addControl(scale, "bottom-right");
    return () => {
      if (mapRef.current === map) map.removeControl(scale); // not after map.remove() on unmount
    };
  }, [units, mapReady]);

  // Globe ↔ flat without reloading the style, keeping the altitude (so the flat map zooms out by one).
  const lastProjection = useRef(projection);
  useEffect(() => {
    const map = mapReady;
    if (!map || lastProjection.current === projection) return;
    const c = map.getCenter();
    // Keep the place and the altitude: the centre moves between real and view latitudes, and the
    // zoom by the flat view's offset and the globe's latitude term.
    const wasFlat = lastProjection.current === "flat";
    const realLat = wasFlat ? fromViewLat(c.lat) : c.lat;
    const alt = altitudeForView(map.getZoom(), wasFlat, live.current.pOverM, realLat);
    lastProjection.current = projection;
    live.current.projection = projection;
    if (styleReady(map)) map.setProjection({ type: projection === "globe" ? "globe" : "mercator" });
    const toFlat = projection === "flat";
    map.setMinZoom(0);
    map.jumpTo({ center: [c.lng, toFlat ? toViewLat(realLat) : realLat], zoom: zoomForView(alt, toFlat, live.current.pOverM, realLat) });
    updateZoomLimits();
  }, [projection, mapReady, updateZoomLimits]);

  // The map pane's height sets P/m (VM V1: P/m ≈ 6.05 × pane height).
  useEffect(() => {
    const el = container.current;
    if (!el) return;
    const ro = new ResizeObserver(() => setPaneHeight(el.clientHeight));
    ro.observe(el);
    setPaneHeight(el.clientHeight);
    return () => ro.disconnect();
  }, []);

  // A new zoom shift (pane size, latitude) moves the zoom limits too.
  useEffect(() => {
    if (mapRef.current && styleReady(mapRef.current)) updateZoomLimits();
  }, [shift, updateZoomLimits]);

  // Statistics → feature-state.
  useEffect(() => {
    applyStates();
  }, [statColours, statsOn, applyStates]);

  // Selection highlight.
  useEffect(() => {
    const map = mapRef.current;
    const s = raw;
    if (!map || !s || !styleReady(map)) return;
    const sources = sourcesOf(s);
    if (selected === null) return;
    setEntityState(map, sources, selected, { hover: true });
    return () => {
      if (live.current.hover !== selected) setEntityState(map, sources, selected, { hover: false });
    };
  }, [selected, raw, sourcesOf]);

  // Classes drawn in the view, for the legend's "only what is on the map".
  useEffect(() => {
    const map = mapReady;
    if (!map || panel !== "legend") return;
    // querySourceFeatures (features of the loaded tiles) instead of a whole-view queryRenderedFeatures,
    // which takes seconds over large fills on the globe.
    const collect = () => {
      const out = new Set<number>();
      for (const source of sourcesOf(live.current.raw)) {
        if (!map.getSource(source)) continue;
        for (const sourceLayer of ENTITY_LAYERS) {
          for (const f of map.querySourceFeatures(source, { sourceLayer })) {
            const c = f.properties?.class;
            if (typeof c === "number") out.add(c);
          }
        }
      }
      setInView((prev) => (prev && prev.size === out.size && [...out].every((c) => prev.has(c)) ? prev : out));
    };
    collect();
    map.on("idle", collect);
    return () => {
      map.off("idle", collect);
    };
  }, [mapReady, panel, sourcesOf]);

  // The title place's article ("Read Article").
  useEffect(() => {
    setTitleArticle(undefined);
    if (title.entity === undefined) return;
    const ac = new AbortController();
    getPlaceLinks(title.entity, ep, ac.signal)
      .then((l) => setTitleArticle(primaryArticle(l)))
      .catch(() => undefined);
    return () => ac.abort();
  }, [title.entity, ep]);

  // --- actions ----------------------------------------------------------------------------
  const open = useCallback(
    (l: PlaceLink, entity: number, name: string) => {
      setMenu(null);
      const link = linkOf(l, entity, name);
      if (onOpenArticle) onOpenArticle(l.ref_id, link);
      else if (typeof window !== "undefined") window.location.assign(`/item/${l.ref_id}`);
    },
    [onOpenArticle],
  );

  const flyTo = useCallback(
    (lat: number, lon: number, altitudeKm: number | null | undefined, bbox?: [number, number, number, number] | null) => {
      const map = mapRef.current;
      const m = live.current.meta;
      if (!map || !m) return;
      const s = live.current.shift;
      const maxz = maxZoomAt(lon, lat, m.maxzoom, m.minRects) + s;
      if (!altitudeKm && bbox && bbox[0] !== bbox[2]) {
        map.fitBounds(
          [
            [bbox[0], toMapLat(bbox[1])],
            [bbox[2], toMapLat(bbox[3])],
          ],
          { padding: 40, maxZoom: maxz, duration: 1200 },
        );
        return;
      }
      const z = zoomForView(altitudeKm || 200, live.current.projection === "flat", live.current.pOverM, lat);
      map.setMaxZoom(maxz);
      map.flyTo({ center: [lon, toMapLat(lat)], zoom: Math.min(Math.max(z, m.minzoom + s), maxz), duration: 1400, essential: true });
    },
    [toMapLat],
  );

  useEffect(() => {
    const map = mapReady;
    if (!map || !focus) return;
    const duration = focus.instant ? 0 : 900;
    const lat = (v: number) => toMapLat(v);
    if (focus.bounds) {
      const [[w, so], [e, n]] = focus.bounds;
      map.fitBounds([[w, lat(so)], [e, lat(n)]], { padding: 48, maxZoom: focus.maxZoom ?? 5.5, duration });
    } else map.flyTo({ center: [focus.lon, lat(focus.lat)], zoom: focus.zoom ?? 3.6, duration });
  }, [focus, mapReady, toMapLat]);

  useEffect(() => {
    const map = mapReady;
    if (!map) return;
    if (dragPan) map.dragPan.enable();
    else map.dragPan.disable();
  }, [dragPan, mapReady]);

  useEffect(() => {
    const map = mapReady;
    if (!map || !markerAt) return;
    const el = document.createElement("div");
    el.className = "enc-atlas-target";
    const m = new maplibregl.Marker({ element: el }).setLngLat([markerAt.lon, toMapLat(markerAt.lat)]).addTo(map);
    return () => {
      m.remove();
    };
  }, [markerAt, mapReady, toMapLat, projection]);

  const goToPlace = useCallback(
    (p: PlaceHit) => {
      setSelected(p.entity);
      const fly = (q: PlaceHit) => {
        const bbox = q.west != null && q.south != null && q.east != null && q.north != null ? ([q.west, q.south, q.east, q.north] as [number, number, number, number]) : null;
        flyTo(q.lat, q.lon, q.goto_km, bbox);
      };
      // Search hits carry only the position; the gazetteer row has the "go to" altitude and extent.
      if (p.goto_km == null && p.west == null) {
        getPlace(p.entity, ep)
          .then((d) => fly({ ...p, ...d.place }))
          .catch(() => fly(p));
      } else fly(p);
    },
    [flyTo, ep],
  );

  // Initial entity (from `/atlas/place/{gid}`).
  useEffect(() => {
    if (initial.entity === undefined || !mapReady) return;
    const ac = new AbortController();
    getPlace(initial.entity, ep, ac.signal)
      .then((d) => {
        const p = d.place;
        if (p.body && p.body !== live.current.body && index) {
          const moon = index.find((e) => styleBody(e) === p.body);
          if (moon) setEntry(moon);
        }
        goToPlace(p);
      })
      .catch(() => undefined);
    return () => ac.abort();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [initial.entity, ep, mapReady, index]);

  const goToHit = useCallback(
    (h: HitPlace) => {
      setMenu(null);
      setSelected(h.entity);
      getPlace(h.entity, ep)
        .then((d) => goToPlace(d.place))
        .catch(() => menu && flyTo(menu.lngLat[1], menu.lngLat[0], altitudeForView(zoom + 1, projection === "flat", pOverM, menu.lngLat[1])));
    },
    [ep, goToPlace, flyTo, menu, zoom, pOverM],
  );

  const pickStyle = useCallback(
    (e: StyleIndexEntry) => {
      if (e.id === entry?.id) return;
      setMenu(null);
      if (styleBody(e) !== body) setSelected(null);
      setEntry(e);
    },
    [entry, body],
  );

  const chooseStat = useCallback(
    (id: number | null) => {
      if (id === null) {
        setIndicator(null);
        setStatValues([]);
        return;
      }
      loadIndicator(id, ep)
        .then((r) => {
          setIndicator(r.indicator);
          setStatValues(r.values);
          setStatsError(null);
        })
        .catch(() => setStatsError("This statistic could not be loaded."));
    },
    [ep],
  );

  const zoomTo = (z: number) => {
    const map = mapRef.current;
    if (!map) return;
    map.easeTo({ zoom: Math.min(Math.max(z, map.getMinZoom()), map.getMaxZoom()), duration: 300 });
  };

  // --- render -----------------------------------------------------------------------------
  const maxz = meta ? maxZoomAt(center.lon, center.lat, meta.maxzoom, meta.minRects) + shift : 9;
  const minz = Math.max(0, (meta?.minzoom ?? 0) + shift, flat && paneHeight > 0 ? zoomToFitHeight(paneHeight) : 0);
  const altitude = altitudeForView(zoom, flat, pOverM, center.lat);
  const lookOf = meta?.raster && palettes ? palettes[meta.raster.toUpperCase()] : undefined;
  const lookName = lookOverride ?? meta?.look;

  return (
    <SpriteContext.Provider value={sprite}>
      <div className={`enc-atlas ${className ?? ""}`} data-style={entry ? entry.file : undefined}>
        <header className="enc-atlas-head">
          <LocatorGlobe ep={ep} lat={center.lat} lon={center.lon} outline={outline} body={body} />
          <div className="enc-atlas-head__main">
            <div className="enc-atlas-title">
              <h1>{quizMode ? "World" : title.title}</h1>
              {title.subtitle && !quizMode && <span className="enc-atlas-title__sub">{title.subtitle}</span>}
            </div>
            <div className="enc-atlas-toolbar">
              {!quizMode && <button
                type="button"
                className="enc-atlas-btn enc-atlas-btn--primary"
                disabled={!titleArticle}
                onClick={() => titleArticle && title.entity !== undefined && open(titleArticle, title.entity, title.title)}
              >
                Read Article
              </button>}
              {!quizMode && <button type="button" className="enc-atlas-btn" aria-pressed={panel === "legend"} onClick={() => setPanel(panel === "legend" ? null : "legend")}>
                Map Legend
              </button>}
              {!quizMode && <button type="button" className="enc-atlas-btn" aria-pressed={panel === "customizer"} onClick={() => setPanel(panel === "customizer" ? null : "customizer")}>
                Map Customizer
              </button>}
              <div className="enc-atlas-zoom" role="group" aria-label="Zoom">
                <button type="button" aria-label="Zoom out" onClick={() => zoomTo(zoom - 1)}>
                  −
                </button>
                <input
                  type="range"
                  min={minz}
                  max={maxz}
                  step={0.01}
                  value={Math.min(Math.max(zoom, minz), maxz)}
                  aria-label="Altitude"
                  list={quizMode ? "enc-atlas-zoom-ticks" : undefined}
                  aria-valuetext={formatAltitude(altitude, unitSys)}
                  onChange={(e) => mapRef.current?.jumpTo({ zoom: Number(e.target.value) })}
                />
                {quizMode && (
                  <datalist id="enc-atlas-zoom-ticks">
                    {Array.from({ length: 9 }, (_, i) => <option key={i} value={minz + ((maxz - minz) * i) / 8} />)}
                  </datalist>
                )}
                <button type="button" aria-label="Zoom in" onClick={() => zoomTo(zoom + 1)}>
                  +
                </button>
                <span className="enc-atlas-alt" title="Camera altitude (Encarta's zoom unit)">
                  {formatAltitude(altitude, unitSys)}
                </span>
              </div>
              {!quizMode && <div className="enc-atlas-seg" role="group" aria-label="View">
                <button type="button" aria-pressed={projection === "globe"} onClick={() => setProjection("globe")}>
                  Globe
                </button>
                <button type="button" aria-pressed={projection === "flat"} onClick={() => setProjection("flat")}>
                  Flat
                </button>
              </div>}
              {!quizMode && <SearchBox ep={ep} body={body} onPick={goToPlace} />}
            </div>
          </div>
        </header>
        <div className="enc-atlas-body">
          <div className="enc-atlas-mapwrap">
            <div className="enc-atlas-map" ref={container} />
            {settings.atlasCrosshairs && <div className="enc-atlas-crosshair" aria-hidden="true" />}
            {error && <div className="enc-atlas-error">{error}</div>}
            {tooltip && !menu && (
              <div className="enc-atlas-tip" style={{ left: tooltip.x + 14, top: tooltip.y + 16 }}>
                {tooltip.text}
              </div>
            )}
            {menu && (
              <PlaceMenu
                ep={ep}
                x={menu.x}
                y={menu.y}
                hits={menu.hits}
                onHover={(h) => {
                  const map = mapRef.current;
                  // Encarta does not highlight the hovered menu entry either (V11).
                  if (!map || !hoverHighlight) return;
                  const sources = sourcesOf(raw);
                  const prev = live.current.hover;
                  if (prev !== null && prev !== selected) setEntityState(map, sources, prev, { hover: false });
                  if (h) setEntityState(map, sources, h.entity, { hover: true });
                  live.current.hover = h?.entity ?? null;
                }}
                onGoTo={goToHit}
                onOpen={(l, h) => open(l, h.entity, h.name)}
                onClose={() => setMenu(null)}
              />
            )}
            {panel === "legend" && built && (
              <LegendPanel style={built} zoom={zoom} overrides={overrides} look={lookName && lookOf ? lookOf.looks[lookName.toUpperCase()] : undefined} lookName={lookName} stats={statsOn} inView={inView} onClose={() => setPanel(null)} />
            )}
            {panel === "customizer" && raw && (
              <CustomizerPanel
                raw={raw}
                projection={projection}
                onProjection={setProjection}
                overrides={overrides}
                onOverrides={setOverrides}
                labelScale={labelScale}
                onLabelScale={setLabelScale}
                looks={lookOf ? Object.keys(lookOf.looks) : []}
                look={lookOverride}
                defaultLook={meta?.look}
                onLook={setLookOverride}
                units={units}
                onUnits={setUnits}
                hoverHighlight={hoverHighlight}
                onHoverHighlight={(on) => {
                  setHoverHighlight(on);
                  const map = mapRef.current;
                  const prev = live.current.hover;
                  if (!on && map && prev !== null && prev !== selected) setEntityState(map, sourcesOf(raw), prev, { hover: false });
                  if (!on) live.current.hover = null;
                }}
                onReset={() => {
                  setOverrides(EMPTY);
                  setLabelScale(1);
                  setLookOverride(undefined);
                  setHoverHighlight(false);
                }}
                onClose={() => setPanel(null)}
              />
            )}
            {panel === "about" && entry && meta && (
              <Panel title="About this Map" onClose={() => setPanel(null)} className="enc-atlas-about">
                <dl>
                  <dt>Map style</dt>
                  <dd>
                    {styleLabel(entry)} ({entry.name}, MapStyle {entry.id})
                  </dd>
                  <dt>Raster</dt>
                  <dd>
                    {entry.raster} / {lookName}
                    {look && !look.baked ? " (recoloured in the browser)" : ""}
                  </dd>
                  <dt>Altitude range</dt>
                  <dd>
                    {formatAltitude(meta.maxAltitudeKm ?? altitudeForZoom(meta.minzoom, stylePOverM), unitSys)} to {formatAltitude(meta.minAltitudeKm ?? altitudeForZoom(meta.maxzoom, stylePOverM), unitSys)}
                    {meta.minRects.length ? `; down to 5 km in ${meta.minRects.length} city and island boxes` : ""}
                  </dd>
                  <dt>Now</dt>
                  <dd>
                    {formatAltitude(altitude, unitSys)} (zoom {zoom.toFixed(2)}), {formatLatLong(center.lat, center.lon, settings.latLongFormat)}
                  </dd>
                  <dt>Scale</dt>
                  <dd>
                    P/m {Math.round(pOverM)} for this {paneHeight}-px pane (Encarta: 3,135 for 518 px); style zooms shifted by {shift.toFixed(2)}
                  </dd>
                </dl>
                {meta.notes && <p className="enc-atlas-note">{meta.notes}</p>}
                <p className="enc-atlas-note">Map data: Microsoft Encarta 2009, converted from your own disc. Rendering: MapLibre GL JS.</p>
              </Panel>
            )}
          </div>
          {!quizMode && <aside className="enc-atlas-side">
            {index && <StylePicker ep={ep} index={index} current={entry} onPick={pickStyle} />}
            <button type="button" className="enc-atlas-btn enc-atlas-about-btn" onClick={() => setPanel(panel === "about" ? null : "about")}>
              About this Map
            </button>
          </aside>}
        </div>
        {isStatistical && (
          <StatsPanel index={statsIndex} error={statsError} indicator={indicator} values={statValues} colours={statColours} hoverCountry={hoverCountry} onChoose={chooseStat} />
        )}
      </div>
    </SpriteContext.Provider>
  );
}
