/**
 * Geography Quiz, laid out as in the VM (fixtures/geoquiz): the start dialog (name and one of four
 * difficulties), then a question column on the left ("Which country is this?", the points on
 * offer, five answer buttons, "Question n of 20", Exit Quiz) beside the quiz map showing the place.
 */
import { useCallback, useEffect, useState } from "react";
import { Link, useNavigate } from "react-router-dom";
import { useApi } from "../../api/useApi";
import { useDocumentTitle } from "../../components/useDocumentTitle";
import { getGeoGame, getGeoLevels, type GeoGame, type GeoLevel } from "../api";
import { playFanfare } from "./fanfare";
import { addScore, LEVEL_NAMES, lastName, pick, pointsFor, questionText, rememberName, type HighScore, type QuestionState } from "./geo";
import { QuizMap } from "./QuizMap";

export function GeoQuiz() {
  useDocumentTitle("Geography Quiz");
  const navigate = useNavigate();
  const levels = useApi((s) => getGeoLevels(s), []);
  const [name, setName] = useState(lastName());
  const [level, setLevel] = useState(1);
  const [started, setStarted] = useState(false);
  const [game, setGame] = useState<GeoGame | null>(null);
  const [error, setError] = useState<string>();
  const [retrying, setRetrying] = useState(false);

  const start = useCallback(
    (lv: number) => {
      rememberName(name.trim());
      setError(undefined);
      setGame(null);
      setRetrying(false);
      setStarted(true);
      getGeoGame(lv).then(setGame, (e: Error) => setError(e.message));
    },
    [name],
  );

  if (!started)
    return (
      <div className="games-page gq-page">
        <StartDialog
          levels={levels.data?.levels ?? []}
          error={levels.error?.message}
          name={name}
          level={level}
          onName={setName}
          onLevel={setLevel}
          onOk={() => start(level)}
          onCancel={() => navigate("/games")}
        />
      </div>
    );
  return (
    <div className="gq-page gq-playing">
      {error && <p className="games-error">{error}</p>}
      {!game && !error && <div className="enc-loading"><div className="enc-spinner" /></div>}
      {game && (
        <QuizRun
          key={game.seed}
          game={game}
          name={name.trim() || "Player"}
          level={levels.data?.levels.find((l) => l.level === game.level) ?? game.settings}
          onAgain={() => {
            setLevel(game.level);
            setRetrying(true);
          }}
          onExit={() => setStarted(false)}
          scoresHidden={retrying}
        />
      )}
      {game && retrying && (
        <div className="gq-modal">
          <StartDialog
            levels={levels.data?.levels ?? []}
            error={levels.error?.message}
            name={name}
            level={level}
            onName={setName}
            onLevel={setLevel}
            onOk={() => start(level)}
            onCancel={() => navigate(-1)}
          />
        </div>
      )}
    </div>
  );
}

function StartDialog({
  levels,
  error,
  name,
  level,
  onName,
  onLevel,
  onOk,
  onCancel,
}: {
  levels: GeoLevel[];
  error?: string;
  name: string;
  level: number;
  onName: (s: string) => void;
  onLevel: (n: number) => void;
  onOk: () => void;
  onCancel: () => void;
}) {
  return (
    <form
      className="gq-dialog"
      role="dialog"
      aria-label="Geography Quiz"
      onSubmit={(e) => {
        e.preventDefault();
        onOk();
      }}
    >
      <div className="gq-dialog-title">
        <span>Geography Quiz</span>
        <button type="button" className="gq-dialog-x" onClick={onCancel} aria-label="Close">
          ×
        </button>
      </div>
      <div className="gq-dialog-body">
        <div className="gq-dialog-banner">quiz</div>
        <p>To begin the quiz, enter your name and choose your level of difficulty.</p>
        <label className="gq-name">
          <b>Your Name:</b>
          <input value={name} maxLength={30} autoFocus onChange={(e) => onName(e.target.value)} />
        </label>
        {error && <p className="games-error">{error}</p>}
        <fieldset className="gq-levels">
          <legend className="sr-only">Level of difficulty</legend>
          {levels.map((l) => (
            <label key={l.level}>
              <input type="radio" name="gq-level" checked={level === l.level} onChange={() => onLevel(l.level)} />
              <span>
                {l.name} ({l.difficulty})
              </span>
            </label>
          ))}
        </fieldset>
        <div className="gq-dialog-buttons">
          <button type="submit" disabled={!levels.length}>
            OK
          </button>
          <button type="button" onClick={onCancel}>
            Cancel
          </button>
        </div>
      </div>
    </form>
  );
}

const FRESH: QuestionState = { wrong: [], done: false, points: 0 };

function QuizRun({ game, name, level, onAgain, onExit, scoresHidden }: { game: GeoGame; name: string; level: GeoLevel | null; onAgain: () => void; onExit: () => void; scoresHidden: boolean }) {
  const points = level?.points ?? [5, 4, 3, 2, 0];
  const [index, setIndex] = useState(0);
  const [q, setQ] = useState<QuestionState>(FRESH);
  const [score, setScore] = useState(0);
  const [right, setRight] = useState(0);
  const [hand, setHand] = useState(true);
  const [table, setTable] = useState<HighScore[] | null>(null);
  const place = game.places[index] ?? null;
  const done = index >= game.places.length;

  useEffect(() => {
    if (done && table === null) setTable(addScore({ name, score, level: game.level, date: Date.now() }));
  }, [done, table, name, score, game.level]);

  // A right answer (or the last wrong one) goes straight to the next question, as in the VM.
  const choose = (entity: number) => {
    if (!place || done) return;
    const next = pick(q, entity, place.entity, points);
    if (next.done && !q.done) playFanfare();
    if (!next.done) {
      setQ(next);
      return;
    }
    setScore((s) => s + next.points);
    if (next.wrong.length === 0 && next.points > 0) setRight((n) => n + 1);
    setQ(FRESH);
    setIndex((i) => i + 1);
  };

  return (
    <div className="gq-run">
      <aside className="gq-panel" aria-live="polite">
        <div className="gq-panel-title">Geography Quiz</div>
        {place && (
          <>
            <div className="gq-question">{questionText(place)}</div>
            <div className="gq-points">({pointsFor(points, q.wrong.length)} points)</div>
            <div className="gq-choices">
              {place.choices.map((c) => {
                const wrong = q.wrong.includes(c.entity);
                return (
                  <button key={c.entity} type="button" className={`gq-choice${wrong ? " is-wrong" : ""}`} disabled={wrong} onClick={() => choose(c.entity)}>
                    {c.name}
                  </button>
                );
              })}
            </div>
            <div className="gq-count">
              Question {index + 1} of {game.places.length}
            </div>
          </>
        )}
        <div className="gq-exit">
          {!done && <span className="gq-score">Score: {score}</span>}
          <button type="button" onClick={onExit}>
            Exit Quiz
          </button>
        </div>
        <Link className="gq-back" to="/games">
          Games &amp; Fun Stuff
        </Link>
      </aside>
      <QuizMap place={done ? null : place} hand={hand} onHand={setHand} />
      {done && !scoresHidden && (
        <ScoresDialog
          name={name}
          level={level}
          score={score}
          max={game.places.length * (points[0] ?? 0)}
          right={right}
          total={game.places.length}
          table={table}
          onAgain={onAgain}
          onExit={onExit}
        />
      )}
    </div>
  );
}

/** The modal "Geography Quiz - Scores" dialog that opens after question 20. */
export function ScoresDialog({
  name,
  level,
  score,
  max,
  right,
  total,
  table,
  onAgain,
  onExit,
}: {
  name: string;
  level: GeoLevel | null;
  score: number;
  max: number;
  right: number;
  total: number;
  table: HighScore[] | null;
  onAgain: () => void;
  onExit: () => void;
}) {
  return (
    <div className="gq-modal">
      <div className="gq-dialog" role="dialog" aria-modal="true" aria-label="Geography Quiz - Scores">
        <div className="gq-dialog-title">
          <span>Geography Quiz - Scores</span>
        </div>
        <div className="gq-dialog-body">
          <div className="gq-dialog-banner">scores</div>
          <p>
            {name}, your score is <b>{score}</b> of {max}. You answered {right} of {total} on the first try.
          </p>
          {table && (
            <>
              <div className="gq-best-title">High Scores for Geography Quiz:</div>
              <ol className="gq-table">
                {table.map((s, i) => (
                  <li key={i}>
                    <span>{s.name}</span> <b>{s.score}</b> <em>{LEVEL_NAMES[s.level] ?? ""}</em>
                  </li>
                ))}
              </ol>
            </>
          )}
          <div className="gq-dialog-buttons">
            <button type="button" onClick={onExit}>
              Exit
            </button>
            <button type="button" autoFocus onClick={onAgain}>
              Try Again
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
