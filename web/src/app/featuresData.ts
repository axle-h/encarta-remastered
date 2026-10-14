/** The Features menu as measured in the original (docs/vm-findings.md, "Features menu"). */
export type QuickKind = "article" | "map" | "media" | "quotation" | "dictionary";

export interface FeatureEntry {
  label: string;
  /** Content id of the entry's record; the route is `/item/{id}`. */
  id: number;
  /** Opens the Quick Search dialog for this category instead of a page. */
  quick?: QuickKind;
}

export interface FeatureGroup {
  title: string;
  entries: FeatureEntry[];
  /** Entries of the "More" flyout. */
  more: FeatureEntry[];
}

export const FEATURE_GROUPS: FeatureGroup[] = [
  {
    title: "Dictionary & Foreign Language Tools",
    entries: [
      { label: "Dictionary", id: 701712323 },
      { label: "Thesaurus", id: 701712324 },
      { label: "Translations", id: 701767016 },
      { label: "World Languages Interactivity", id: 701878967 },
    ],
    more: [],
  },
  {
    title: "Geography Tools",
    entries: [
      { label: "World Atlas", id: 701713739 },
      { label: "Quick Search: Maps", id: 701766739, quick: "map" },
      { label: "Country Facts and Figures", id: 701713705 },
    ],
    more: [
      { label: "World Statistical Map", id: 701713537 },
      { label: "Flags of the World", id: 701713732 },
      { label: "Map Treks", id: 701713686 },
      { label: "U.S. State Facts and Figures", id: 701713708 },
      { label: "Canadian Province & Territory Facts and Figures", id: 701713706 },
    ],
  },
  {
    title: "Explore",
    entries: [
      { label: "3D Ancient Sites", id: 701713683 },
      { label: "Encarta Kids", id: 701712340 },
      { label: "Panoramas", id: 701713680 },
    ],
    more: [
      { label: "Timeline", id: 701779281 },
      { label: "Quick Search: Photos & More", id: 701779282, quick: "media" },
      { label: "Virtual Tours", id: 701713684 },
      { label: "Videos", id: 701713681 },
      { label: "Encarta on the Web", id: 701835573 },
    ],
  },
];

export const BROWSE_ALL: FeatureEntry = { label: "Browse all", id: 701766702 };

export const featureHref = (e: FeatureEntry) => `/item/${e.id}`;
