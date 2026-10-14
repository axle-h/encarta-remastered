/**
 * Quizzes (SWQUIZ): the question bank as the board games group it, and a player for one group.
 * Both quiz types are played: multiple choice (pick one, "check answer") and typed answers.
 */
import { useEffect, useMemo, useRef, useState } from "react";
import { Link, useParams, useSearchParams } from "react-router-dom";
import { useApi } from "../../api/useApi";
import { Html } from "../../components/Html";
import { useDocumentTitle } from "../../components/useDocumentTitle";
import { getQuizGames, getQuizzes, imageUrl, playableAudio, type MediaRef, type Quiz, type QuizGame, type QuizPart } from "../api";
import { rng, shuffle } from "../mindmaze/castle";
import { QUIZ_POINTS, isRight, plain } from "./answer";

const WORDS: Record<string, string> = {
  NoAm: "North America",
  SoAm: "South America",
  Antar: "Antarctica",
  Austr: "Australia",
  "Austr&Pacific": "Australia & Pacific",
  PhysFeatures: "Physical Features",
  CitiesPeople: "Cities & People",
};

/** A group's display name: the boards' coded names ("2_NoAm_Countries") become "North America: Countries (round 2)". */
export function groupLabel(name: string): string {
  const m = /^(\d+)_(.+)$/.exec(name);
  if (!m) return name;
  const parts = m[2]!.split("_").map((w) => WORDS[w] ?? w.replace(/([a-z])([A-Z])/g, "$1 $2"));
  return `${parts.join(": ")} (round ${m[1]})`;
}

export function gameTitle(g: QuizGame): string {
  return g.items[0]?.title ?? g.title;
}

export function QuizzesPage() {
  useDocumentTitle("Quizzes");
  const games = useApi((s) => getQuizGames(s), []);
  return (
    <div className="games-page">
      <div className="games-crumbs">
        <Link to="/games">Games &amp; Fun Stuff</Link> › Quizzes
      </div>
      <h1 className="games-h1">Quizzes</h1>
      {games.loading && <div className="enc-loading"><div className="enc-spinner" /></div>}
      {games.error && <p className="games-error">{games.error.message}</p>}
      {games.data && (
        <>
          <p className="games-lead">
            {games.data.total.toLocaleString()} questions ({games.data.types.map((t) => `${t.quizzes.toLocaleString()} ${t.type === "textAnswer" ? "typed-answer" : "multiple-choice"}`).join(", ")}),
            grouped as the Encarta game boards use them. Pick a topic to play.
          </p>
          <div className="quiz-games">
            {games.data.games.map((g) => (
              <section key={g.file_name} className="quiz-game">
                <header>
                  {g.items[0]?.picon && <img src={g.items[0].picon} alt="" className="quiz-game-pic" />}
                  <div>
                    <h2>{gameTitle(g)}</h2>
                    {g.items[0]?.caption_html && <Html className="quiz-game-caption" html={g.items[0].caption_html} />}
                    <Link className="games-btn" to={`/games/quizzes/${g.file_name}/all`}>
                      Play all topics
                    </Link>
                  </div>
                </header>
                <ul className="quiz-groups">
                  {g.groups.map((gr) => (
                    <li key={gr.id}>
                      <Link to={`/games/quizzes/${g.file_name}/${gr.id}`}>
                        {groupLabel(gr.name)} <small>({gr.questions.length})</small>
                      </Link>
                    </li>
                  ))}
                </ul>
              </section>
            ))}
            {games.data.ungrouped.length > 0 && (
              <section className="quiz-game">
                <header>
                  <div>
                    <h2>More Questions</h2>
                    <p className="quiz-game-caption">Questions in the bank that no game board uses.</p>
                    <Link className="games-btn" to="/games/quizzes/more/all">
                      Play ({games.data.ungrouped.length})
                    </Link>
                  </div>
                </header>
              </section>
            )}
          </div>
        </>
      )}
    </div>
  );
}

export function QuizRoute() {
  const { file = "", group = "all" } = useParams();
  const [search] = useSearchParams();
  const games = useApi((s) => getQuizGames(s), []);
  const pick = useMemo(() => {
    if (!games.data) return null;
    if (file === "more") return { title: "More Questions", group: "All questions", ids: games.data.ungrouped };
    const g = games.data.games.find((x) => x.file_name.toLowerCase() === file.toLowerCase());
    if (!g) return { title: "Quiz", group: "", ids: [] as number[] };
    if (group === "all") return { title: gameTitle(g), group: "All topics", ids: [...new Set(g.groups.flatMap((x) => x.questions))] };
    const gr = g.groups.find((x) => x.id === group);
    return { title: gameTitle(g), group: gr ? groupLabel(gr.name) : "", ids: gr?.questions ?? [] };
  }, [games.data, file, group]);
  const limit = Number(search.get("n")) || (group === "all" ? 15 : 0);
  if (games.error) return <div className="games-page"><p className="games-error">{games.error.message}</p></div>;
  if (!pick) return <div className="enc-loading"><div className="enc-spinner" /></div>;
  return (
    <div className="games-page">
      <div className="games-crumbs">
        <Link to="/games">Games &amp; Fun Stuff</Link> › <Link to="/games/quizzes">Quizzes</Link> › {pick.title}
      </div>
      <QuizPlayer key={`${file}/${group}`} title={pick.title} subtitle={pick.group} ids={pick.ids} limit={limit} />
    </div>
  );
}

function Picture({ m, className }: { m: MediaRef | null; className?: string }) {
  if (!m) return null;
  const src = imageUrl(m.media, m.file_name) ?? m.url;
  return <img className={className} src={src} alt="" width={m.width ?? undefined} height={m.height ?? undefined} />;
}

function Sound({ m, label }: { m: MediaRef | null; label: string }) {
  const ref = useRef<HTMLAudioElement>(null);
  const src = m ? playableAudio(m.media) : undefined;
  if (!src) return null;
  return (
    <span className="quiz-sound">
      <audio ref={ref} src={src} preload="auto" />
      <button type="button" className="games-btn" onClick={() => void ref.current?.play()}>
        ▶ {label}
      </button>
    </span>
  );
}

function Part({ part, label }: { part: QuizPart; label: string }) {
  return (
    <div className="quiz-part">
      <Picture m={part.image} className="quiz-pic" />
      <div className="quiz-part-text">
        {part.html && <Html html={part.html} />}
        <Sound m={part.audio} label={label} />
      </div>
    </div>
  );
}

type Phase = "ask" | "result" | "done";

export function QuizPlayer({ title, subtitle, ids, limit = 0, seed }: { title: string; subtitle?: string; ids: number[]; limit?: number; seed?: number }) {
  useDocumentTitle(title);
  const order = useMemo(() => {
    const s = shuffle(ids, rng(seed ?? Math.floor(Math.random() * 2 ** 31)));
    return limit > 0 ? s.slice(0, limit) : s;
  }, [ids, limit, seed]);
  const quizzes = useApi((s) => (order.length ? getQuizzes(order, s) : Promise.resolve([] as Quiz[])), [order]);
  const [index, setIndex] = useState(0);
  const [phase, setPhase] = useState<Phase>("ask");
  const [choice, setChoice] = useState("");
  const [right, setRight] = useState(false);
  const [score, setScore] = useState(0);
  const [rightCount, setRightCount] = useState(0);
  const [credits, setCredits] = useState(false);
  const list = quizzes.data ?? [];
  const quiz = list[index];

  useEffect(() => {
    setChoice("");
    setCredits(false);
  }, [index]);

  if (quizzes.loading) return <div className="enc-loading"><div className="enc-spinner" /></div>;
  if (quizzes.error) return <p className="games-error">{quizzes.error.message}</p>;
  if (!list.length) return <p className="games-lead">There are no questions here.</p>;

  const check = () => {
    if (!quiz || !choice.trim()) return;
    const ok = isRight(quiz, choice);
    setRight(ok);
    if (ok) {
      setScore((s) => s + QUIZ_POINTS);
      setRightCount((n) => n + 1);
    }
    setPhase("result");
  };
  const next = () => {
    if (index + 1 >= list.length) setPhase("done");
    else {
      setIndex(index + 1);
      setPhase("ask");
    }
  };
  const restart = () => {
    setIndex(0);
    setScore(0);
    setRightCount(0);
    setPhase("ask");
  };

  return (
    <div className="quiz-window" role="region" aria-label={title}>
      <div className="quiz-titlebar">
        <span>
          {title}
          {subtitle ? ` — ${subtitle}` : ""}
        </span>
        <span className="quiz-score">SCORE {String(score).padStart(3, "0")}</span>
      </div>
      {phase === "done" ? (
        <div className="quiz-body quiz-done">
          <h2>Well done!</h2>
          <p>
            You answered <b>{rightCount}</b> of <b>{list.length}</b> questions correctly, for a score of <b>{score}</b>.
          </p>
          <div className="quiz-actions">
            <button type="button" className="games-btn" onClick={restart}>
              Play again
            </button>
            <Link className="games-btn" to="/games/quizzes">
              More quizzes
            </Link>
          </div>
        </div>
      ) : (
        quiz && (
          <div className="quiz-body">
            <div className="quiz-progress">
              Question {index + 1} of {list.length}
            </div>
            {phase === "ask" ? (
              <form
                onSubmit={(e) => {
                  e.preventDefault();
                  check();
                }}
              >
                <Part part={quiz.question} label="Listen" />
                {quiz.type === "textAnswer" ? (
                  <label className="quiz-typed">
                    Your answer:{" "}
                    <input autoFocus value={choice} onChange={(e) => setChoice(e.target.value)} autoComplete="off" spellCheck={false} />
                  </label>
                ) : (
                  <ul className="quiz-choices">
                    {quiz.answers.map((a) => (
                      <li key={a.id}>
                        <label className={choice === a.id ? "is-chosen" : ""}>
                          <input type="radio" name={`q${quiz.id}`} value={a.id} checked={choice === a.id} onChange={() => setChoice(a.id)} />
                          <Picture m={a.image} className="quiz-answer-pic" />
                          {a.html && <Html as="span" html={a.html} />}
                          <Sound m={a.audio} label="Listen" />
                        </label>
                      </li>
                    ))}
                  </ul>
                )}
                <div className="quiz-actions">
                  <button type="submit" className="games-btn games-btn-primary" disabled={!choice.trim()}>
                    check answer
                  </button>
                </div>
              </form>
            ) : (
              <div className={`quiz-result ${right ? "is-right" : "is-wrong"}`}>
                <h3>{right ? "Correct!" : "Not quite."}</h3>
                {!right && quiz.type === "textAnswer" && quiz.accept[0] && (
                  <p>
                    The answer is <b>{quiz.accept[0]}</b>.
                  </p>
                )}
                {!right && quiz.type !== "textAnswer" && (
                  <p>
                    The answer is <b>{plain(quiz.answers.find((a) => a.id === String(quiz.correct))?.html ?? "")}</b>.
                  </p>
                )}
                <Part part={quiz.reward} label="Listen" />
                <div className="quiz-actions">
                  {(quiz.reward_credits || quiz.credits) && (
                    <button type="button" className="games-btn" onClick={() => setCredits(!credits)}>
                      Media Credits
                    </button>
                  )}
                  <button type="button" className="games-btn games-btn-primary" onClick={next} autoFocus>
                    continue
                  </button>
                </div>
                {credits && <p className="quiz-credits">{quiz.reward_credits || quiz.credits}</p>}
              </div>
            )}
          </div>
        )
      )}
    </div>
  );
}
