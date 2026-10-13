/** Our own small SVG icon set (original artwork, drawn for this project). */
export type IconName =
  | "back"
  | "forward"
  | "home"
  | "search"
  | "go"
  | "close"
  | "plus"
  | "minus"
  | "outline"
  | "article"
  | "find"
  | "grid"
  | "speaker"
  | "play"
  | "arrow"
  | "chevron"
  | "image"
  | "video"
  | "table"
  | "cube"
  | "hand"
  | "map"
  | "text"
  | "book"
  | "clock";

const paths: Record<IconName, string> = {
  back: "M14 5l-7 7 7 7M8 12h11",
  forward: "M10 5l7 7-7 7M16 12H5",
  home: "M4 11l8-7 8 7M6.5 9.5V19h4v-5h3v5h4V9.5",
  search: "M10.5 4a6.5 6.5 0 1 0 0 13 6.5 6.5 0 0 0 0-13zM15.5 15.5L20 20",
  go: "M6 12h11M12 6.5l5.5 5.5-5.5 5.5",
  close: "M6 6l12 12M18 6L6 18",
  plus: "M12 7v10M7 12h10",
  minus: "M7 12h10",
  outline: "M5 7h2M10 7h9M5 12h2M10 12h9M5 17h2M10 17h9",
  article: "M6 4h12v16H6zM9 8h6M9 12h6M9 16h4",
  find: "M10.5 4a6.5 6.5 0 1 0 0 13 6.5 6.5 0 0 0 0-13zM15.5 15.5L20 20",
  grid: "M5 5h4v4H5zM10 5h4v4h-4zM15 5h4v4h-4zM5 10h4v4H5zM10 10h4v4h-4zM15 10h4v4h-4zM5 15h4v4H5zM10 15h4v4h-4zM15 15h4v4h-4z",
  speaker: "M4 9h4l5-4v14l-5-4H4zM16 9a4 4 0 0 1 0 6M18.5 6.5a7.5 7.5 0 0 1 0 11",
  play: "M8 5v14l11-7z",
  arrow: "M5 12h12M12 7l5 5-5 5",
  chevron: "M7 10l5 5 5-5",
  image: "M4 5h16v14H4zM4 16l5-5 4 4 3-3 4 4M15 9.5a1.5 1.5 0 1 0 0-.01",
  video: "M4 6h12v12H4zM16 10l4-2.5v9L16 14",
  table: "M4 5h16v14H4zM4 10h16M4 14.5h16M10 5v14",
  cube: "M12 3l8 4.5v9L12 21l-8-4.5v-9zM12 12l8-4.5M12 12v9M12 12L4 7.5",
  hand: "M8 13V6.5a1.5 1.5 0 0 1 3 0V12M11 11V5a1.5 1.5 0 0 1 3 0v6M14 11V6.5a1.5 1.5 0 0 1 3 0V14a6 6 0 0 1-6 6h-1a5 5 0 0 1-4-2l-2.5-3.5a1.5 1.5 0 0 1 2.4-1.8L8 14",
  map: "M4 6l5-2 6 2 5-2v14l-5 2-6-2-5 2zM9 4v14M15 6v14",
  text: "M5 6h14M5 10h14M5 14h14M5 18h9",
  book: "M5 4h6a2 2 0 0 1 2 2v14a2 2 0 0 0-2-2H5zM19 4h-6a2 2 0 0 0-2 2v14a2 2 0 0 1 2-2h6z",
  clock: "M12 4a8 8 0 1 0 0 16 8 8 0 0 0 0-16zM12 8v4l3 2",
};

const filled = new Set<IconName>(["play"]);

export function Icon({ name, size = 18, className }: { name: IconName; size?: number; className?: string }) {
  return (
    <svg
      className={`icon icon-${name}${className ? ` ${className}` : ""}`}
      width={size}
      height={size}
      viewBox="0 0 24 24"
      aria-hidden="true"
      fill={filled.has(name) ? "currentColor" : "none"}
      stroke={filled.has(name) ? "none" : "currentColor"}
      strokeWidth={2.2}
      strokeLinecap="round"
      strokeLinejoin="round"
    >
      <path d={paths[name]} />
    </svg>
  );
}

/** Small type badge for a media item (the original marks thumbnails with a type icon). */
export function mediaIcon(cls: string): IconName {
  switch (cls) {
    case "audio":
      return "speaker";
    case "video":
      return "video";
    case "table":
      return "table";
    case "tour":
      return "cube";
    case "interactive":
      return "hand";
    case "map":
      return "map";
    case "text":
      return "text";
    default:
      return "image";
  }
}
