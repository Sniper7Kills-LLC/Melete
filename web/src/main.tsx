import React from "react";
import ReactDOM from "react-dom/client";
import {
  BrowserRouter,
  Link,
  Navigate,
  NavLink,
  Route,
  Routes,
} from "react-router-dom";
import { Amplify } from "aws-amplify";
import { Authenticator } from "@aws-amplify/ui-react";

import "./index.css";
import { amplifyOutputs, isStubBackend } from "./amplify-config";
import { AccountChip } from "./components/AccountChip";
import { Viewer } from "./pages/Viewer";
import { Designer } from "./pages/Designer";
import { Templeter } from "./pages/Templeter";
import { Tooler } from "./pages/Tooler";
import { Gallery } from "./pages/Gallery";
import { My } from "./pages/My";
import { Share } from "./pages/Share";
import { NotebookViewer } from "./pages/NotebookViewer";
import { useUnits } from "./store/unitsStore";

// Configure Amplify before any GraphQL or auth call. `amplify_outputs.json`
// (Gen 2 shape with top-level `auth` / `data` / `storage` blocks) is
// passed directly per the official quickstart — Amplify v6 detects the
// outputs format from the `version` field. The stub falls in when no
// real outputs file is present; in that mode Gallery / My render a
// "Backend not configured" banner and skip live network calls.
// eslint-disable-next-line @typescript-eslint/no-explicit-any -- outputs is a wide JSON shape
Amplify.configure(amplifyOutputs as any);

function NavBar() {
  const linkBase =
    "px-3 py-1.5 text-sm rounded transition-colors hover:bg-slate-200";
  const linkActive = "bg-slate-900 text-white hover:bg-slate-900";
  const linkClass = ({ isActive }: { isActive: boolean }) =>
    `${linkBase} ${isActive ? linkActive : "text-slate-700"}`;
  // Two visual clusters: "Design" (the three editors) and "Library"
  // (browse + personal). Keeps build-flow links together and separates
  // them from the discovery/management flow.
  return (
    <header className="flex items-center gap-3 border-b border-slate-200 bg-white px-4 py-2">
      <Link to="/" className="text-lg font-semibold text-slate-900">
        Journal
      </Link>
      <nav className="flex items-center gap-1">
        <span className="ml-1 text-[11px] uppercase tracking-wide text-slate-400">
          Design
        </span>
        <NavLink to="/designer" className={linkClass}>
          Page designer
        </NavLink>
        <NavLink to="/templeter" className={linkClass}>
          Planner designer
        </NavLink>
        <NavLink to="/tooler" className={linkClass}>
          Brush designer
        </NavLink>
        <span aria-hidden className="mx-2 h-5 w-px bg-slate-200" />
        <span className="text-[11px] uppercase tracking-wide text-slate-400">
          Library
        </span>
        <NavLink to="/gallery" className={linkClass}>
          Gallery
        </NavLink>
        <NavLink to="/my" className={linkClass}>
          My library
        </NavLink>
      </nav>
      <UnitsSelector />
      <span className="text-xs text-slate-400">
        {isStubBackend ? "Backend: stub" : "Backend: live"}
      </span>
      <AccountChip />
    </header>
  );
}

function UnitsSelector() {
  const units = useUnits((s) => s.units);
  const setUnits = useUnits((s) => s.setUnits);
  return (
    <label className="ml-auto flex items-center gap-1 text-xs text-slate-600">
      Units
      <select
        value={units}
        onChange={(e) => setUnits(e.target.value as "mm" | "in")}
        className="rounded border border-slate-300 bg-white px-2 py-1 text-xs"
      >
        <option value="mm">mm</option>
        <option value="in">inches</option>
      </select>
    </label>
  );
}

function App() {
  return (
    <Authenticator.Provider>
      <BrowserRouter
        future={{ v7_startTransition: true, v7_relativeSplatPath: true }}
      >
        <div className="flex h-full flex-col">
          <NavBar />
          <main className="flex-1 overflow-hidden">
            <Routes>
              {/* Home lands on Gallery — there's no real /-only content
                  in the POC, and Gallery is the discovery surface. The
                  sample-notebook Viewer moves to /demo so it stays
                  reachable for screenshots / smoke tests. */}
              <Route path="/" element={<Navigate to="/gallery" replace />} />
              <Route path="/demo" element={<Viewer />} />
              <Route path="/designer" element={<Designer />} />
              <Route path="/templeter" element={<Templeter />} />
              <Route path="/tooler" element={<Tooler />} />
              <Route path="/gallery" element={<Gallery />} />
              {/* Legacy /public path collapsed into /gallery — keep a
                  redirect so any external links / bookmarks survive. */}
              <Route
                path="/public"
                element={<Navigate to="/gallery" replace />}
              />
              <Route path="/my" element={<My />} />
              <Route path="/t/:kind/:id" element={<Share />} />
              <Route path="/n/:id" element={<NotebookViewer />} />
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
    </Authenticator.Provider>
  );
}

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
