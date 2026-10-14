import { lazy, type ComponentType } from "react";
import { AtlasPlaceholder } from "../pages/AtlasPlaceholder";

// The atlas (plan Step 5) lives in src/atlas/ and is owned separately; load it when present.
export interface AtlasView {
  lat: number;
  lon: number;
  altitudeKm: number;
  style?: string;
  projection?: string;
}
export interface AtlasModuleProps {
  initialPlace?: { entity?: number; lat?: number; lon?: number; altitudeKm?: number } | string;
  initialStyle?: string;
  initialProjection?: "globe" | "flat";
  onOpenArticle?: (refId: number) => void;
  onViewChange?: (v: AtlasView) => void;
  className?: string;
}
const atlasModules = import.meta.glob("../atlas/index.tsx");
const atlasLoader = atlasModules["../atlas/index.tsx"];
export const AtlasLazy = atlasLoader
  ? lazy(async () => {
      const m = (await atlasLoader()) as { Atlas?: ComponentType<AtlasModuleProps> };
      return { default: m.Atlas ?? (AtlasPlaceholder as ComponentType<AtlasModuleProps>) };
    })
  : null;

