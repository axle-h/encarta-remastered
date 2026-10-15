import { lazy, Suspense, useEffect, useRef, useState } from "react";
import { Navigate, Outlet, Route, Routes, useNavigate, useParams, useSearchParams } from "react-router-dom";
import { ArticleMediaPage, MediaBrowserPage } from "../pages/MediaBrowserPage";
import { AtlasPlaceholder } from "../pages/AtlasPlaceholder";
import { BrowsePage } from "../pages/BrowsePage";
import { DictionaryPage } from "../pages/DictionaryPage";
import { FavoritesPage } from "../pages/FavoritesPage";
import { HelpPage } from "../pages/HelpPage";
import { HomePage } from "../pages/HomePage";
import { ItemPage } from "../pages/ItemPage";
import { NotFound } from "../pages/NotFound";
import { SearchPage } from "../pages/SearchPage";
import { TimelinePage } from "../pages/TimelinePage";
import { ToursPage } from "../pages/ToursPage";
import { KidsShell } from "../kids/KidsShell";
import { KidsHome } from "../kids/KidsHome";
import { KidsBrowsePage } from "../kids/KidsBrowse";
import { KidsSearchPage } from "../kids/KidsSearch";
import { KidsPicturesPage } from "../kids/KidsPictures";
import { MediaViewerProvider } from "../media/MediaViewer";
import { Shell } from "./Shell";
import { SettingsProvider } from "../settings/settings";
import "./chrome.css";

import { AtlasLazy, type AtlasModuleProps, type AtlasView } from "./atlasModule";

/** The atlas's starting view from the route: `/atlas/place/{gid}`, `?url=msencarta://…`, `?lat&lon&alt`. */
export function atlasInitialPlace(splat: string, search: URLSearchParams): AtlasModuleProps["initialPlace"] {
  const m = /^place\/(\d+)/.exec(splat);
  if (m) return { entity: Number(m[1]) };
  const url = search.get("url");
  if (url) return url;
  const num = (k: string) => (search.has(k) ? Number(search.get(k)) : undefined);
  if (search.has("lat") || search.has("lon")) return { lat: num("lat"), lon: num("lon"), altitudeKm: num("alt") };
  return undefined;
}

const TourViewLazy = lazy(async () => ({ default: (await import("../pages/TourView")).TourView }));
const GamesLazy = lazy(async () => ({ default: (await import("../games")).GamesRoutes }));

/** The URL search for an atlas view (`lat`, `lon`, `alt`, `style`, `view`), as the route reads it back. */
export function atlasViewSearch(v: AtlasView): string {
  const p = new URLSearchParams();
  p.set("lat", v.lat.toFixed(4));
  p.set("lon", v.lon.toFixed(4));
  p.set("alt", String(Math.round(v.altitudeKm)));
  if (v.style) p.set("style", v.style);
  if (v.projection === "flat") p.set("view", "flat");
  return p.toString();
}

function AtlasRoute() {
  const splat = useParams()["*"] ?? "";
  const [search] = useSearchParams();
  const navigate = useNavigate();
  // The map keeps the URL in step with its view (replace, debounced). Only a URL change that did
  // not come from the map itself (a link to /atlas/place/…, Back) remounts it at the new place.
  const here = `${splat}?${search.toString()}`;
  const written = useRef<string | null>(null);
  const [mount, setMount] = useState(0);
  const seen = useRef(here);
  useEffect(() => {
    if (here === seen.current) return;
    seen.current = here;
    if (here !== written.current) setMount((n) => n + 1);
  }, [here]);
  const timer = useRef<number | undefined>(undefined);
  useEffect(() => () => window.clearTimeout(timer.current), []);
  const onViewChange = (v: AtlasView) => {
    window.clearTimeout(timer.current);
    timer.current = window.setTimeout(() => {
      const q = atlasViewSearch(v);
      written.current = `?${q}`;
      navigate({ pathname: "/atlas", search: `?${q}` }, { replace: true });
    }, 400);
  };
  if (!AtlasLazy) return <AtlasPlaceholder />;
  const view = search.get("view");
  return (
    <Suspense fallback={<div className="enc-loading"><div className="enc-spinner" /></div>}>
      <div className="atlas-route">
        <AtlasLazy
          key={mount}
          className="atlas-route-map"
          initialPlace={atlasInitialPlace(splat, search)}
          initialStyle={search.get("style") ?? undefined}
          initialProjection={view === "flat" || view === "globe" ? view : undefined}
          onOpenArticle={(refId) => navigate(`/item/${refId}`)}
          onViewChange={onViewChange}
        />
      </div>
    </Suspense>
  );
}

function TourRoute() {
  const { tourId } = useParams();
  return (
    <Suspense fallback={<div className="enc-loading"><div className="enc-spinner" /></div>}>
      <TourViewLazy tourId={tourId!} />
    </Suspense>
  );
}

function Adult() {
  return (
    <MediaViewerProvider>
      <Shell>
        <Outlet />
      </Shell>
    </MediaViewerProvider>
  );
}

function Kids() {
  return (
    <MediaViewerProvider kids>
      <KidsShell>
        <Outlet />
      </KidsShell>
    </MediaViewerProvider>
  );
}

export function App() {
  return (
    <SettingsProvider>
    <Routes>
      <Route element={<Adult />}>
        <Route index element={<HomePage />} />
        <Route path="browse/:id" element={<BrowsePage />} />
        <Route path="item/:id" element={<ItemPage />} />
        <Route path="item/:id/media" element={<ArticleMediaPage />} />
        <Route path="search" element={<SearchPage />} />
        <Route path="media" element={<MediaBrowserPage />} />
        <Route path="dictionary" element={<DictionaryPage tab="dictionary" />} />
        <Route path="dictionary/:dict/:entry" element={<DictionaryPage tab="dictionary" />} />
        <Route path="thesaurus" element={<DictionaryPage tab="thesaurus" />} />
        <Route path="translate" element={<DictionaryPage tab="bilingual" />} />
        <Route path="timeline" element={<TimelinePage />} />
        <Route path="tours" element={<ToursPage />} />
        <Route path="tours/:tourId" element={<TourRoute />} />
        <Route path="atlas/*" element={<AtlasRoute />} />
        <Route path="favorites" element={<FavoritesPage />} />
        <Route path="help" element={<HelpPage />} />
        <Route path="games/*" element={<Suspense fallback={<div className="enc-loading"><div className="enc-spinner" /></div>}><GamesLazy /></Suspense>} />
        <Route path="*" element={<NotFound />} />
      </Route>
      <Route path="kids" element={<Kids />}>
        <Route index element={<KidsHome />} />
        <Route path="browse/:id" element={<KidsBrowsePage />} />
        <Route path="item/:id" element={<ItemPage kids />} />
        <Route path="item/:id/media" element={<KidsPicturesPage />} />
        <Route path="search" element={<KidsSearchPage />} />
        <Route path="dictionary" element={<DictionaryPage tab="dictionary" kids />} />
        <Route path="dictionary/:dict/:entry" element={<DictionaryPage tab="dictionary" kids />} />
        <Route path="*" element={<Navigate to="/kids" replace />} />
      </Route>
    </Routes>
    </SettingsProvider>
  );
}
