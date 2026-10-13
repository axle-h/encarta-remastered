import { lazy, Suspense, type ComponentType } from "react";
import { Navigate, Outlet, Route, Routes, useParams } from "react-router-dom";
import { ArticleMediaPage, MediaBrowserPage } from "../pages/MediaBrowserPage";
import { AtlasPlaceholder } from "../pages/AtlasPlaceholder";
import { BrowsePage } from "../pages/BrowsePage";
import { DictionaryPage } from "../pages/DictionaryPage";
import { HomePage } from "../pages/HomePage";
import { ItemPage } from "../pages/ItemPage";
import { NotFound } from "../pages/NotFound";
import { SearchPage } from "../pages/SearchPage";
import { TimelinePage } from "../pages/TimelinePage";
import { ToursPage } from "../pages/ToursPage";
import { KidsShell } from "../kids/KidsShell";
import { KidsHome } from "../kids/KidsHome";
import { MediaViewerProvider } from "../media/MediaViewer";
import { Shell } from "./Shell";

// The atlas (plan Step 5) lives in src/atlas/ and is owned separately; load it when present.
const atlasModules = import.meta.glob("../atlas/index.tsx");
const atlasLoader = atlasModules["../atlas/index.tsx"];
const AtlasLazy = atlasLoader
  ? lazy(async () => {
      const m = (await atlasLoader()) as { Atlas?: ComponentType; default?: ComponentType };
      return { default: m.Atlas ?? m.default ?? AtlasPlaceholder };
    })
  : null;

const TourViewLazy = lazy(async () => ({ default: (await import("../pages/TourView")).TourView }));

function AtlasRoute() {
  if (!AtlasLazy) return <AtlasPlaceholder />;
  return (
    <Suspense fallback={<div className="enc-loading"><div className="enc-spinner" /></div>}>
      <AtlasLazy />
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
        <Route path="*" element={<NotFound />} />
      </Route>
      <Route path="kids" element={<Kids />}>
        <Route index element={<KidsHome />} />
        <Route path="browse/:id" element={<BrowsePage kids />} />
        <Route path="item/:id" element={<ItemPage kids />} />
        <Route path="item/:id/media" element={<ArticleMediaPage kids />} />
        <Route path="search" element={<SearchPage kids />} />
        <Route path="dictionary" element={<DictionaryPage tab="dictionary" kids />} />
        <Route path="dictionary/:dict/:entry" element={<DictionaryPage tab="dictionary" kids />} />
        <Route path="*" element={<Navigate to="/kids" replace />} />
      </Route>
    </Routes>
  );
}
