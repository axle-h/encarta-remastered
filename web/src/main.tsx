import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { BrowserRouter } from "react-router-dom";
import { App } from "./app/App";
import { loadDiscFonts } from "./app/fonts";
import "./styles/base.css";
import "./styles/shell.css";
import "./styles/article.css";
import "./styles/pages.css";
import "./styles/kids.css";
import "./styles/print.css";

void loadDiscFonts();

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <BrowserRouter future={{ v7_startTransition: true, v7_relativeSplatPath: true }}>
      <App />
    </BrowserRouter>
  </StrictMode>,
);
