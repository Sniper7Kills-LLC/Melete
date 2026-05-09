import React from "react";
import ReactDOM from "react-dom/client";
import { BrowserRouter, Link, NavLink, Route, Routes } from "react-router-dom";

import "./index.css";
import { Viewer } from "./pages/Viewer";
import { Designer } from "./pages/Designer";
import { Templeter } from "./pages/Templeter";

function NavBar() {
  const linkBase =
    "px-3 py-1.5 text-sm rounded transition-colors hover:bg-slate-200";
  const linkActive = "bg-slate-900 text-white hover:bg-slate-900";
  return (
    <header className="flex items-center gap-3 border-b border-slate-200 bg-white px-4 py-2">
      <Link to="/" className="text-lg font-semibold text-slate-900">
        Journal · Web POC
      </Link>
      <nav className="flex gap-1">
        <NavLink
          end
          to="/"
          className={({ isActive }) =>
            `${linkBase} ${isActive ? linkActive : "text-slate-700"}`
          }
        >
          Viewer
        </NavLink>
        <NavLink
          to="/designer"
          className={({ isActive }) =>
            `${linkBase} ${isActive ? linkActive : "text-slate-700"}`
          }
        >
          Designer
        </NavLink>
        <NavLink
          to="/templeter"
          className={({ isActive }) =>
            `${linkBase} ${isActive ? linkActive : "text-slate-700"}`
          }
        >
          Templeter
        </NavLink>
      </nav>
      <span className="ml-auto text-xs text-slate-400">
        WASM mocked · UI scaffolding
      </span>
    </header>
  );
}

function App() {
  return (
    <BrowserRouter
      future={{ v7_startTransition: true, v7_relativeSplatPath: true }}
    >
      <div className="flex h-full flex-col">
        <NavBar />
        <main className="flex-1 overflow-hidden">
          <Routes>
            <Route path="/" element={<Viewer />} />
            <Route path="/designer" element={<Designer />} />
            <Route path="/templeter" element={<Templeter />} />
            <Route
              path="*"
              element={
                <div className="p-8 text-slate-600">
                  Not found.{" "}
                  <Link to="/" className="text-indigo-600 underline">
                    Go home
                  </Link>
                  .
                </div>
              }
            />
          </Routes>
        </main>
      </div>
    </BrowserRouter>
  );
}

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
