/** The match player with its styles, as one lazily loaded chunk for the media viewer. */
import { useMemo } from "react";
import type { MediaInfo } from "../../api/types";
import "../games.css";
import { MatchBoard } from "./MatchGame";
import { parseMatch } from "./match";

export default function NativeMatch({ title, definition, media }: { title: string; definition: unknown; media: Record<string, MediaInfo> }) {
  const def = useMemo(() => parseMatch(definition), [definition]);
  return <MatchBoard title={title} def={def} media={media} />;
}
