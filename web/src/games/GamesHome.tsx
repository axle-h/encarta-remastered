/**
 * The games hub: the Geography Quiz and the quizzes (both in Encarta 2009), then the extras that
 * are on the disc but that Encarta 2009 no longer opens (MindMaze, City Climate, Population Chart).
 */
import { Link } from "react-router-dom";
import { getJson } from "../api/client";
import { useApi } from "../api/useApi";
import { useDocumentTitle } from "../components/useDocumentTitle";

interface Catalogue {
  mindmaze: { total: number } | null;
  quiz: { total: number } | null;
  geoquiz: { levels: unknown[] } | null;
  charts: { id: string; title: string }[];
  match_games: number;
}

function Tile({ to, title, text, off }: { to: string; title: string; text: string; off?: boolean }) {
  return (
    <li className={`games-tile${off ? " is-off" : ""}`}>
      <Link to={to}>
        <span className="games-tile-title">{title}</span>
        <span className="games-tile-text">{text}</span>
      </Link>
    </li>
  );
}

export function GamesHome() {
  useDocumentTitle("Games & Fun Stuff");
  const cat = useApi((s) => getJson<Catalogue>("/api/games", s), []);
  const c = cat.data;
  const chart = (id: string) => !c || c.charts.some((x) => x.id === id);
  return (
    <div className="games-page">
      <h1 className="games-h1">Games &amp; Fun Stuff</h1>
      {cat.error && <p className="games-error">{cat.error.message}</p>}
      <ul className="games-tiles">
        <Tile to="/games/geoquiz" title="Geography Quiz" text="Name the country, city or continent shown on the map. Four levels, from Navigator to World Class." off={!!c && !c.geoquiz} />
        <Tile
          to="/games/quizzes"
          title="Quizzes"
          text={`${c?.quiz ? c.quiz.total.toLocaleString() + " " : ""}picture questions on animals, history, geography, sports and more, grouped as Encarta's game boards use them.`}
          off={!!c && !c.quiz?.total}
        />
        {(!c || c.match_games > 0) && (
          <Tile to="/games/match" title="Match Games" text={`${c ? c.match_games + " " : ""}matching games: brood or gaggle, cats, dogs, presidents, musical instruments …`} />
        )}
      </ul>
      <h2 className="games-h2">Extras</h2>
      <p className="games-lead">
        These are on the Encarta 2009 disc, but Encarta 2009 itself no longer opens them. They are rebuilt here from their data.
      </p>
      <ul className="games-tiles">
        <Tile
          to="/games/mindmaze"
          title="MindMaze"
          text={`Find your way through the castle by answering ${c?.mindmaze ? c.mindmaze.total.toLocaleString() + " " : ""}questions from its denizens.`}
          off={!!c && !c.mindmaze}
        />
        <Tile to="/games/city-climate" title="City Climate" text="Compare temperature and precipitation for up to eight of 165 cities." off={!chart("city_climate")} />
        <Tile to="/games/population" title="Population Chart" text="Census population of US states and Canadian provinces, 1800 to 2001." off={!chart("population")} />
      </ul>
    </div>
  );
}
