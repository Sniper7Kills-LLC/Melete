import { useEffect, useState } from "react";
import { Link, useParams } from "react-router-dom";
import { useAuthenticator } from "@aws-amplify/ui-react";

import {
  client,
  type BrushRow,
  type NotebookTemplateRow,
  type PageTemplateRow,
  type SavedKind,
} from "@/amplify-client";
import { isStubBackend } from "@/amplify-config";
import { TemplatePreview } from "@/components/TemplatePreview";
import { NotebookTemplatePreview } from "@/components/NotebookTemplatePreview";
import type { PageTemplate } from "@/types";
import type { NotebookTemplate } from "@/types/notebook-template";
import { shim } from "@/wasm";

/**
 * Read-only single-row viewer for shareable links.
 *
 * Route: `/t/:kind/:id` where kind ∈ {page, notebook, brush}. Anonymous
 * apiKey auth — works for PUBLIC and UNLISTED rows alike (apiKey read
 * is granted by the schema's `allow.publicApiKey().to(['read'])` rule).
 *
 * Renders: name, description, visibility badge, raw bodyToml in a
 * monospace block. The Download button copies the TOML to the
 * clipboard so the desktop client can paste it into its config dirs.
 */
type Kind = "page" | "notebook" | "brush";

type AnyRow = PageTemplateRow | NotebookTemplateRow | BrushRow;

type LoadState =
  | { status: "loading" }
  | { status: "err"; message: string }
  | { status: "ok"; row: AnyRow };

export function Share() {
  const params = useParams<{ kind: string; id: string }>();
  const kind = params.kind as Kind | undefined;
  const id = params.id;
  const [state, setState] = useState<LoadState>({ status: "loading" });
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    if (!kind || !id) return;
    if (isStubBackend) {
      setState({ status: "err", message: "Backend not configured." });
      return;
    }
    if (kind !== "page" && kind !== "notebook" && kind !== "brush") {
      setState({ status: "err", message: `Unknown kind: ${kind}` });
      return;
    }
    let cancelled = false;
    setState({ status: "loading" });
    fetchRow(kind, id)
      .then((r) => {
        if (cancelled) return;
        if (r.errors?.length) {
          setState({
            status: "err",
            message: r.errors.map((e) => e.message).join("; "),
          });
          return;
        }
        if (!r.data) {
          setState({ status: "err", message: "Not found." });
          return;
        }
        setState({ status: "ok", row: r.data });
      })
      .catch((e: unknown) => {
        if (cancelled) return;
        setState({
          status: "err",
          message: e instanceof Error ? e.message : String(e),
        });
      });
    return () => {
      cancelled = true;
    };
  }, [kind, id]);

  function download(toml: string) {
    void navigator.clipboard.writeText(toml).then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), 1600);
    });
  }

  return (
    <div className="h-full overflow-auto bg-slate-50 p-6">
      <div className="mx-auto max-w-3xl">
        <div className="mb-4 text-sm">
          <Link to="/gallery" className="text-indigo-600 hover:underline">
            ← Gallery
          </Link>
        </div>
        {state.status === "loading" && (
          <p className="text-sm text-slate-400">Loading…</p>
        )}
        {state.status === "err" && (
          <div className="rounded border border-rose-300 bg-rose-50 px-4 py-3 text-sm text-rose-800">
            <div className="font-semibold">Failed to load</div>
            <div className="text-xs">{state.message}</div>
          </div>
        )}
        {state.status === "ok" && (
          <article className="overflow-hidden rounded border border-slate-200 bg-white shadow-sm">
            <header className="border-b border-slate-200 px-5 py-4">
              <div className="flex items-start justify-between gap-3">
                <div>
                  <div className="text-xs uppercase tracking-wide text-slate-500">
                    {kindLabel(kind!)}
                  </div>
                  <h1 className="text-xl font-semibold text-slate-900">
                    {state.row.name}
                  </h1>
                  {state.row.description && (
                    <p className="mt-1 text-sm text-slate-600">
                      {state.row.description}
                    </p>
                  )}
                </div>
                <span
                  className={`rounded px-2 py-0.5 text-[10px] font-semibold uppercase tracking-wide ${
                    state.row.visibility === "PUBLIC"
                      ? "bg-emerald-100 text-emerald-700"
                      : state.row.visibility === "UNLISTED"
                        ? "bg-amber-100 text-amber-700"
                        : "bg-slate-100 text-slate-600"
                  }`}
                >
                  {state.row.visibility}
                </span>
              </div>
            </header>
            <div className="px-5 py-4">
              <ShareActions
                kind={kind!}
                rowId={state.row.id}
                rowName={state.row.name}
                bodyToml={state.row.bodyToml}
                onDownload={() => download(state.row.bodyToml)}
                downloadLabel={copied ? "Copied!" : "Download"}
              />
              {kind === "page" && (
                <SharePagePreview bodyToml={state.row.bodyToml} />
              )}
              {kind === "notebook" && (
                <ShareNotebookPreview bodyToml={state.row.bodyToml} />
              )}
              <div className="mt-4">
                <span className="text-xs uppercase tracking-wide text-slate-500">
                  TOML body
                </span>
                <pre className="mt-2 max-h-96 overflow-auto rounded border border-slate-200 bg-slate-50 p-3 font-mono text-[11px] leading-snug text-slate-800">
                  {state.row.bodyToml}
                </pre>
              </div>
            </div>
          </article>
        )}
      </div>
    </div>
  );
}

/** Vello WASM preview block under the action row on the page-template
 *  share page (#92). Parses bodyToml lazily so a TOML error renders a
 *  small banner instead of crashing the route. */
function SharePagePreview({ bodyToml }: { bodyToml: string }) {
  const [parsed, setParsed] = useState<
    | { status: "loading" }
    | { status: "err"; message: string }
    | { status: "ok"; template: PageTemplate }
  >({ status: "loading" });
  const [zoom, setZoom] = useState(2.2);

  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        await shim.ready();
        const t = shim.parseTemplateToml(bodyToml);
        if (!cancelled) setParsed({ status: "ok", template: t });
      } catch (e) {
        if (!cancelled)
          setParsed({
            status: "err",
            message: e instanceof Error ? e.message : String(e),
          });
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [bodyToml]);

  return (
    <div className="mt-4">
      <div className="mb-2 flex items-center gap-3">
        <span className="text-xs uppercase tracking-wide text-slate-500">
          Preview
        </span>
        {parsed.status === "ok" && (
          <label className="ml-auto flex items-center gap-1 text-xs text-slate-600">
            zoom
            <input
              type="range"
              min={1}
              max={5}
              step={0.1}
              value={zoom}
              onChange={(e) => setZoom(Number(e.target.value))}
            />
            <span className="w-10 text-right tabular-nums">
              {zoom.toFixed(1)}×
            </span>
          </label>
        )}
      </div>
      <div className="flex items-center justify-center overflow-auto rounded border border-slate-200 bg-slate-100 p-4">
        {parsed.status === "loading" && (
          <div className="px-3 py-6 text-xs text-slate-500">Parsing TOML…</div>
        )}
        {parsed.status === "err" && (
          <div className="rounded border border-rose-300 bg-rose-50 px-3 py-2 text-xs text-rose-800">
            Couldn&rsquo;t parse template: {parsed.message}
          </div>
        )}
        {parsed.status === "ok" && (
          <TemplatePreview template={parsed.template} zoom={zoom} />
        )}
      </div>
    </div>
  );
}

/** Notebook-template preview block on `/t/notebook/:id`. Parses the
 *  TOML lazily via the WASM shim and feeds the resulting
 *  `NotebookTemplate` to `NotebookTemplatePreview`, which renders each
 *  referenced page template via the same TemplatePreview component
 *  the page-template Share page uses. */
function ShareNotebookPreview({ bodyToml }: { bodyToml: string }) {
  const [parsed, setParsed] = useState<
    | { status: "loading" }
    | { status: "err"; message: string }
    | { status: "ok"; nt: NotebookTemplate }
  >({ status: "loading" });

  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        await shim.ready();
        const nt = shim.parseNotebookTemplateToml(bodyToml) as NotebookTemplate;
        if (!cancelled) setParsed({ status: "ok", nt });
      } catch (e) {
        if (!cancelled)
          setParsed({
            status: "err",
            message: e instanceof Error ? e.message : String(e),
          });
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [bodyToml]);

  if (parsed.status === "loading")
    return (
      <div className="mt-4 px-1 text-xs text-slate-500">
        Parsing notebook template…
      </div>
    );
  if (parsed.status === "err")
    return (
      <div className="mt-4 rounded border border-rose-300 bg-rose-50 px-3 py-2 text-xs text-rose-800">
        Couldn&rsquo;t parse notebook template: {parsed.message}
      </div>
    );
  return <NotebookTemplatePreview notebookTemplate={parsed.nt} />;
}

function kindLabel(k: Kind): string {
  switch (k) {
    case "page":
      return "Page template";
    case "notebook":
      return "Notebook template";
    case "brush":
      return "Brush";
  }
}

function savedKindFor(k: Kind): SavedKind {
  switch (k) {
    case "page":
      return "PageTemplate";
    case "notebook":
      return "NotebookTemplate";
    case "brush":
      return "Brush";
  }
}

function forkMutationFor(k: Kind, id: string) {
  switch (k) {
    case "page":
      return client.mutations.forkPageTemplate({ id });
    case "notebook":
      return client.mutations.forkNotebookTemplate({ id });
    case "brush":
      return client.mutations.forkBrush({ id });
  }
}

function ShareActions({
  kind,
  rowId,
  rowName,
  bodyToml: _bodyToml,
  onDownload,
  downloadLabel,
}: {
  kind: Kind;
  rowId: string;
  rowName: string;
  bodyToml: string;
  onDownload: () => void;
  downloadLabel: string;
}) {
  const { authStatus } = useAuthenticator((c) => [c.authStatus]);
  const signedIn = authStatus === "authenticated";
  const [forkBusy, setForkBusy] = useState(false);
  const [saveBusy, setSaveBusy] = useState(false);
  const [actionMsg, setActionMsg] = useState<string | null>(null);

  async function fork() {
    if (!signedIn || forkBusy) return;
    setForkBusy(true);
    setActionMsg(null);
    try {
      const r = await forkMutationFor(kind, rowId);
      if (r.errors?.length) {
        setActionMsg(`Fork failed: ${r.errors.map((e) => e.message).join("; ")}`);
        return;
      }
      setActionMsg("Forked into your library — see /my for an editable copy.");
    } catch (e) {
      setActionMsg(
        `Fork failed: ${e instanceof Error ? e.message : String(e)}`,
      );
    } finally {
      setForkBusy(false);
    }
  }

  async function save() {
    if (!signedIn || saveBusy) return;
    setSaveBusy(true);
    setActionMsg(null);
    try {
      const r = await client.models.SavedTemplate.create(
        {
          kind: savedKindFor(kind),
          sourceId: rowId,
          sourceName: rowName,
          savedAt: new Date().toISOString(),
        },
        { authMode: "userPool" },
      );
      if (r.errors?.length) {
        setActionMsg(
          `Save failed: ${r.errors.map((e) => e.message).join("; ")}`,
        );
        return;
      }
      setActionMsg(
        "Saved to your library — updates from the original flow through automatically.",
      );
    } catch (e) {
      setActionMsg(
        `Save failed: ${e instanceof Error ? e.message : String(e)}`,
      );
    } finally {
      setSaveBusy(false);
    }
  }

  return (
    <div>
      <div className="flex flex-wrap items-center gap-2">
        <button
          type="button"
          onClick={save}
          disabled={!signedIn || saveBusy}
          title={
            signedIn
              ? "Subscribe — keeps a reference, you receive author updates"
              : "Sign in to save"
          }
          className="rounded border border-emerald-300 bg-emerald-50 px-3 py-1 text-xs font-medium text-emerald-800 hover:bg-emerald-100 disabled:cursor-not-allowed disabled:opacity-50"
        >
          {saveBusy ? "Saving…" : "Save to library"}
        </button>
        <button
          type="button"
          onClick={fork}
          disabled={!signedIn || forkBusy}
          title={
            signedIn
              ? "Fork — clones a private editable copy, no further updates from source"
              : "Sign in to fork"
          }
          className="rounded border border-slate-300 bg-white px-3 py-1 text-xs font-medium text-slate-700 hover:bg-slate-100 disabled:cursor-not-allowed disabled:opacity-50"
        >
          {forkBusy ? "Forking…" : "Fork"}
        </button>
        <button
          type="button"
          onClick={onDownload}
          className="rounded bg-indigo-600 px-3 py-1 text-xs font-medium text-white hover:bg-indigo-700"
        >
          {downloadLabel}
        </button>
      </div>
      {actionMsg && (
        <div className="mt-2 text-xs text-slate-700">{actionMsg}</div>
      )}
    </div>
  );
}

function fetchRow(kind: Kind, id: string) {
  switch (kind) {
    case "page":
      return client.models.PageTemplate.get(
        { id },
        { authMode: "apiKey" },
      );
    case "notebook":
      return client.models.NotebookTemplate.get(
        { id },
        { authMode: "apiKey" },
      );
    case "brush":
      return client.models.Brush.get({ id }, { authMode: "apiKey" });
  }
}
