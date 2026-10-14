/**
 * Games (plan Step 7): routes under `/games`. Lazy-loaded by the app (src/app/App.tsx).
 * docs/games.md describes each game, its data and the choices made where the original is unknown.
 */
import { lazy, Suspense, type ReactNode } from "react";
import { Navigate, Route, Routes } from "react-router-dom";
import { GamesHome } from "./GamesHome";
import "./games.css";

const MindMaze = lazy(async () => ({ default: (await import("./mindmaze/MindMaze")).MindMaze }));
const GeoQuiz = lazy(async () => ({ default: (await import("./geoquiz/GeoQuiz")).GeoQuiz }));
const QuizzesPage = lazy(async () => ({ default: (await import("./quiz/Quizzes")).QuizzesPage }));
const QuizRoute = lazy(async () => ({ default: (await import("./quiz/Quizzes")).QuizRoute }));
const CityClimate = lazy(async () => ({ default: (await import("./charts/CityClimate")).CityClimate }));
const PopulationChart = lazy(async () => ({ default: (await import("./charts/PopulationChart")).PopulationChart }));
const MatchList = lazy(async () => ({ default: (await import("./match/MatchGame")).MatchList }));
const MatchRoute = lazy(async () => ({ default: (await import("./match/MatchGame")).MatchRoute }));

function Wait({ children }: { children: ReactNode }) {
  return <Suspense fallback={<div className="enc-loading"><div className="enc-spinner" /></div>}>{children}</Suspense>;
}

export function GamesRoutes() {
  return (
    <Routes>
      <Route index element={<GamesHome />} />
      <Route path="geoquiz" element={<Wait><GeoQuiz /></Wait>} />
      <Route path="quizzes" element={<Wait><QuizzesPage /></Wait>} />
      <Route path="quizzes/:file/:group" element={<Wait><QuizRoute /></Wait>} />
      <Route path="mindmaze" element={<Wait><MindMaze /></Wait>} />
      <Route path="city-climate" element={<Wait><CityClimate /></Wait>} />
      <Route path="population" element={<Wait><PopulationChart /></Wait>} />
      <Route path="match" element={<Wait><MatchList /></Wait>} />
      <Route path="match/:file" element={<Wait><MatchRoute /></Wait>} />
      <Route path="*" element={<Navigate to="/games" replace />} />
    </Routes>
  );
}

export default GamesRoutes;
