// Signed-in "my templates" view. AccountChip in the header drives
// auth — this page reads `useAuthenticator` to decide whether to show
// content, a sign-in prompt, or the stub-mode banner. The data-mode
// queries use Cognito userPool auth (the schema's `allow.owner()`
// rule scopes results to the caller's Cognito sub).

import { useAuthenticator } from '@aws-amplify/ui-react';
import '@aws-amplify/ui-react/styles.css';
import { useEffect, useState } from 'react';
import { Link } from 'react-router-dom';
import {
  client,
  type BrushRow as Brush,
  type NotebookRow as Notebook,
  type NotebookTemplateRow as NotebookTemplate,
  type PageTemplateRow as PageTemplate,
  type SavedTemplateRow,
  type Visibility,
} from '../amplify-client';
import { isStubBackend } from '../amplify-config';

type LoadState<T> =
  | { status: 'loading' }
  | { status: 'ok'; rows: T[] }
  | { status: 'err'; message: string };

type ModelKind = 'PageTemplate' | 'NotebookTemplate' | 'Brush' | 'Notebook';

interface RowLike {
  id: string;
  name: string;
  visibility: string;
  description?: string | null;
}

function useMyList<T extends { id: string }>(
  ownerSub: string | undefined,
  fetcher: (
    sub: string,
  ) => Promise<{ data: T[] | null; errors?: { message: string }[] }>,
): [LoadState<T>, (mut: (rows: T[]) => T[]) => void] {
  const [state, setState] = useState<LoadState<T>>({ status: 'loading' });
  useEffect(() => {
    if (!ownerSub) return;
    if (isStubBackend) {
      setState({
        status: 'err',
        message: 'Backend not configured (stub mode).',
      });
      return;
    }
    let cancelled = false;
    setState({ status: 'loading' });
    fetcher(ownerSub)
      .then((r) => {
        if (cancelled) return;
        if (r.errors?.length) {
          setState({
            status: 'err',
            message: r.errors.map((e) => e.message).join('; '),
          });
        } else {
          setState({ status: 'ok', rows: r.data ?? [] });
        }
      })
      .catch((e: unknown) => {
        if (cancelled) return;
        setState({
          status: 'err',
          message: e instanceof Error ? e.message : String(e),
        });
      });
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps -- fetcher is stable per page
  }, [ownerSub]);

  function patch(mut: (rows: T[]) => T[]) {
    setState((s) =>
      s.status === 'ok' ? { status: 'ok', rows: mut(s.rows) } : s,
    );
  }
  return [state, patch];
}

async function publishRow(
  kind: ModelKind,
  id: string,
  visibility: Visibility,
) {
  switch (kind) {
    case 'PageTemplate':
      return client.mutations.publishPageTemplate({ id, visibility });
    case 'NotebookTemplate':
      return client.mutations.publishNotebookTemplate({ id, visibility });
    case 'Brush':
      return client.mutations.publishBrush({ id, visibility });
    case 'Notebook':
      // No publish* pipeline mutation for notebooks (no fork/clone
      // semantic — visibility just gates who can read the snapshot).
      // Plain update against the model row is enough.
      return client.models.Notebook.update({
        id,
        visibility,
        updatedAtSort: new Date().toISOString(),
      });
  }
}

async function deleteRow(kind: ModelKind, id: string) {
  switch (kind) {
    case 'PageTemplate':
      return client.models.PageTemplate.delete({ id });
    case 'NotebookTemplate':
      return client.models.NotebookTemplate.delete({ id });
    case 'Brush':
      return client.models.Brush.delete({ id });
    case 'Notebook':
      return client.models.Notebook.delete({ id });
  }
}

const VISIBILITIES: Visibility[] = ['PRIVATE', 'UNLISTED', 'PUBLIC'];

function editHrefFor(kind: ModelKind, id: string): string | null {
  switch (kind) {
    case 'PageTemplate':
      return `/designer?edit=${id}`;
    case 'NotebookTemplate':
      return `/templeter?edit=${id}`;
    case 'Brush':
      return `/tooler?edit=${id}`;
    case 'Notebook':
      // Notebooks are not editable in the browser — strokes only flow
      // from the desktop. Surface a "View" link to the live viewer.
      return `/n/${id}`;
  }
}

function editLabelFor(kind: ModelKind): string {
  return kind === 'Notebook' ? 'View' : 'Edit';
}

function shareUrlFor(kind: ModelKind, id: string): string {
  // Notebooks share via the dedicated /n/:id viewer route, not the
  // generic /t/:kind/:id template share page.
  if (kind === 'Notebook') {
    return `${window.location.origin}/n/${id}`;
  }
  const slug =
    kind === 'PageTemplate'
      ? 'page'
      : kind === 'NotebookTemplate'
        ? 'notebook'
        : 'brush';
  return `${window.location.origin}/t/${slug}/${id}`;
}

function ShareLinkButton({
  kind,
  row,
}: {
  kind: ModelKind;
  row: RowLike;
}) {
  const [copied, setCopied] = useState(false);
  function copy() {
    const url = shareUrlFor(kind, row.id);
    void navigator.clipboard.writeText(url).then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), 1600);
    });
  }
  return (
    <button
      type="button"
      onClick={copy}
      title="Copy a shareable link to this UNLISTED row"
      className="rounded border border-amber-300 bg-amber-50 px-2 py-1 text-xs font-medium text-amber-800 hover:bg-amber-100"
    >
      {copied ? 'Link copied!' : 'Copy link'}
    </button>
  );
}

function VisibilityBadge({ v }: { v: string }) {
  return (
    <span
      className={`rounded px-2 py-0.5 text-[10px] font-semibold uppercase tracking-wide ${
        v === 'PUBLIC'
          ? 'bg-emerald-100 text-emerald-700'
          : v === 'UNLISTED'
            ? 'bg-amber-100 text-amber-700'
            : 'bg-slate-100 text-slate-600'
      }`}
    >
      {v}
    </span>
  );
}

function DeleteButton({
  kind,
  row,
  onDeleted,
}: {
  kind: ModelKind;
  row: RowLike;
  onDeleted: () => void;
}) {
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);

  async function handle() {
    if (busy) return;
    const ok = window.confirm(
      `Delete "${row.name}"? This cannot be undone.`,
    );
    if (!ok) return;
    setBusy(true);
    setErr(null);
    try {
      const r = await deleteRow(kind, row.id);
      if (r.errors?.length) {
        setErr(r.errors.map((e) => e.message).join('; '));
        return;
      }
      onDeleted();
    } catch (e) {
      setErr(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="flex items-center gap-2">
      {err && (
        <span
          title={err}
          className="max-w-[180px] truncate text-[10px] text-rose-600"
        >
          {err}
        </span>
      )}
      <button
        type="button"
        onClick={handle}
        disabled={busy}
        title="Delete this row permanently"
        className="rounded border border-rose-300 bg-white px-2 py-1 text-xs font-medium text-rose-700 hover:bg-rose-50 disabled:opacity-50"
      >
        {busy ? 'Deleting…' : 'Delete'}
      </button>
    </div>
  );
}

function PublishMenu({
  kind,
  row,
  onChange,
}: {
  kind: ModelKind;
  row: RowLike;
  onChange: (next: Visibility) => void;
}) {
  const [busy, setBusy] = useState<Visibility | null>(null);
  const [err, setErr] = useState<string | null>(null);

  async function pick(target: Visibility) {
    if (target === row.visibility || busy) return;
    setBusy(target);
    setErr(null);
    try {
      const r = await publishRow(kind, row.id, target);
      if (r.errors?.length) {
        setErr(r.errors.map((e) => e.message).join('; '));
      } else {
        onChange(target);
      }
    } catch (e) {
      setErr(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(null);
    }
  }

  return (
    <div className="flex items-center gap-2">
      {err && (
        <span
          title={err}
          className="max-w-[180px] truncate text-[10px] text-rose-600"
        >
          {err}
        </span>
      )}
      <select
        value={row.visibility}
        disabled={busy !== null}
        onChange={(e) => pick(e.target.value as Visibility)}
        className="rounded border border-slate-300 bg-white px-2 py-0.5 text-xs text-slate-700 disabled:opacity-50"
      >
        {VISIBILITIES.map((v) => (
          <option key={v} value={v}>
            {busy === v ? `${v}…` : v}
          </option>
        ))}
      </select>
    </div>
  );
}

function MySection<T extends RowLike>({
  title,
  kind,
  state,
  patch,
}: {
  title: string;
  kind: ModelKind;
  state: LoadState<T>;
  patch: (mut: (rows: T[]) => T[]) => void;
}) {
  return (
    <section className="space-y-2">
      <h2 className="text-sm font-semibold uppercase tracking-wide text-slate-500">
        {title}
      </h2>
      {state.status === 'loading' && (
        <p className="text-sm text-slate-400">Loading…</p>
      )}
      {state.status === 'err' && (
        <p className="text-sm text-rose-600">Error: {state.message}</p>
      )}
      {state.status === 'ok' && state.rows.length === 0 && (
        <p className="text-sm text-slate-400">
          You have not created any {title.toLowerCase()} yet.
        </p>
      )}
      {state.status === 'ok' && state.rows.length > 0 && (
        <ul className="space-y-1">
          {state.rows.map((row) => (
            <li
              key={row.id}
              className="flex items-center justify-between gap-3 rounded border border-slate-200 bg-white px-3 py-2"
            >
              <div className="min-w-0 flex-1">
                <div className="text-sm font-medium text-slate-900">
                  {row.name}
                </div>
                {row.description && (
                  <div className="text-xs text-slate-500">
                    {row.description}
                  </div>
                )}
              </div>
              <VisibilityBadge v={row.visibility} />
              {row.visibility === 'UNLISTED' && (
                <ShareLinkButton kind={kind} row={row} />
              )}
              {editHrefFor(kind, row.id) && (
                <Link
                  to={editHrefFor(kind, row.id)!}
                  className="rounded border border-slate-300 bg-white px-2 py-1 text-xs font-medium text-slate-700 hover:bg-slate-100"
                >
                  {editLabelFor(kind)}
                </Link>
              )}
              <PublishMenu
                kind={kind}
                row={row}
                onChange={(next) =>
                  patch((rows) =>
                    rows.map((r) =>
                      r.id === row.id ? { ...r, visibility: next } : r,
                    ),
                  )
                }
              />
              <DeleteButton
                kind={kind}
                row={row}
                onDeleted={() =>
                  patch((rows) => rows.filter((r) => r.id !== row.id))
                }
              />
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}

function MyContent({ ownerSub }: { ownerSub: string }) {
  const [pages, patchPages] = useMyList<PageTemplate>(ownerSub, (sub) =>
    client.models.PageTemplate.listPageTemplatesByOwner({ owner: sub }),
  );
  const [notebooks, patchNotebooks] = useMyList<NotebookTemplate>(
    ownerSub,
    (sub) =>
      client.models.NotebookTemplate.listNotebookTemplatesByOwner({
        owner: sub,
      }),
  );
  const [brushes, patchBrushes] = useMyList<Brush>(ownerSub, (sub) =>
    client.models.Brush.listBrushesByOwner({ owner: sub }),
  );
  const [myNotebooks, patchMyNotebooks] = useMyList<Notebook>(ownerSub, (sub) =>
    client.models.Notebook.listNotebooksByOwner({ owner: sub }),
  );
  const [saved, patchSaved] = useMyList<SavedTemplateRow>(ownerSub, (sub) =>
    client.models.SavedTemplate.listSavedTemplatesByOwner({ owner: sub }),
  );

  // Two-tab split: "Mine" = content the signed-in user authored;
  // "Saved" = subscriptions to other authors' shared templates. The
  // mental models differ (authoring + visibility actions vs. browse +
  // unsave), so keeping them on one scroll was confusing.
  const [tab, setTab] = useState<'mine' | 'saved'>('mine');
  const savedCount = saved.status === 'ok' ? saved.rows.length : null;

  return (
    <div className="h-full overflow-auto p-6">
      <div className="mx-auto max-w-5xl space-y-6">
        <header>
          <h1 className="text-2xl font-semibold text-slate-900">
            My library
          </h1>
          <p className="text-sm text-slate-600">
            Templates and brushes you have authored, plus subscriptions
            to other authors. Use the visibility menu on each row to
            publish — <strong>UNLISTED</strong> shows only via direct
            link, <strong>PUBLIC</strong> appears in the Gallery.
          </p>
        </header>
        <div className="flex gap-1 border-b border-slate-200">
          <TabButton
            active={tab === 'mine'}
            onClick={() => setTab('mine')}
            label="Mine"
          />
          <TabButton
            active={tab === 'saved'}
            onClick={() => setTab('saved')}
            label={
              savedCount === null
                ? 'Saved'
                : `Saved (${savedCount})`
            }
          />
        </div>
        {tab === 'mine' && (
          <>
            <MySection
              title="Page templates"
              kind="PageTemplate"
              state={pages}
              patch={patchPages}
            />
            <MySection
              title="Notebook templates"
              kind="NotebookTemplate"
              state={notebooks}
              patch={patchNotebooks}
            />
            <MySection
              title="Brushes"
              kind="Brush"
              state={brushes}
              patch={patchBrushes}
            />
            <MySection
              title="Notebooks"
              kind="Notebook"
              state={myNotebooks}
              patch={patchMyNotebooks}
            />
          </>
        )}
        {tab === 'saved' && <SavedSection state={saved} patch={patchSaved} />}
      </div>
    </div>
  );
}

function TabButton({
  active,
  onClick,
  label,
}: {
  active: boolean;
  onClick: () => void;
  label: string;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={
        'px-4 py-2 text-sm font-medium transition-colors -mb-px border-b-2 ' +
        (active
          ? 'border-indigo-600 text-indigo-700'
          : 'border-transparent text-slate-500 hover:text-slate-800')
      }
    >
      {label}
    </button>
  );
}

function SavedSection({
  state,
  patch,
}: {
  state: LoadState<SavedTemplateRow>;
  patch: (mut: (rows: SavedTemplateRow[]) => SavedTemplateRow[]) => void;
}) {
  return (
    <section className="space-y-2">
      <h2 className="text-sm font-semibold uppercase tracking-wide text-slate-500">
        Saved (subscribed)
      </h2>
      <p className="text-xs text-slate-500">
        References to public/unlisted templates by other authors.
        Updates from the source flow through automatically — the
        desktop fetches the current bodyToml on demand.
      </p>
      {state.status === 'loading' && (
        <p className="text-sm text-slate-400">Loading…</p>
      )}
      {state.status === 'err' && (
        <p className="text-sm text-rose-600">Error: {state.message}</p>
      )}
      {state.status === 'ok' && state.rows.length === 0 && (
        <p className="text-sm text-slate-400">
          No saved templates yet. Open a Share link and use{' '}
          <strong>Save to library</strong>.
        </p>
      )}
      {state.status === 'ok' && state.rows.length > 0 && (
        <ul className="space-y-1">
          {state.rows.map((row) => (
            <SavedRow
              key={row.id}
              row={row}
              onRemoved={() =>
                patch((rows) => rows.filter((r) => r.id !== row.id))
              }
            />
          ))}
        </ul>
      )}
    </section>
  );
}

function SavedRow({
  row,
  onRemoved,
}: {
  row: SavedTemplateRow;
  onRemoved: () => void;
}) {
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);

  async function unsave() {
    if (busy) return;
    setBusy(true);
    setErr(null);
    try {
      const r = await client.models.SavedTemplate.delete({ id: row.id });
      if (r.errors?.length) {
        setErr(r.errors.map((e) => e.message).join('; '));
        return;
      }
      onRemoved();
    } catch (e) {
      setErr(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }

  const shareKind =
    row.kind === 'PageTemplate'
      ? 'page'
      : row.kind === 'NotebookTemplate'
        ? 'notebook'
        : 'brush';

  return (
    <li className="flex items-center justify-between gap-3 rounded border border-slate-200 bg-white px-3 py-2">
      <div className="min-w-0 flex-1">
        <div className="text-sm font-medium text-slate-900">
          {row.sourceName ?? '(untitled)'}
        </div>
        <div className="text-xs text-slate-500">
          {row.kind} · saved {row.savedAt.slice(0, 10)}
        </div>
      </div>
      {err && (
        <span
          title={err}
          className="max-w-[180px] truncate text-[10px] text-rose-600"
        >
          {err}
        </span>
      )}
      <Link
        to={`/t/${shareKind}/${row.sourceId}`}
        className="rounded border border-slate-300 bg-white px-2 py-1 text-xs font-medium text-slate-700 hover:bg-slate-100"
      >
        View
      </Link>
      <button
        type="button"
        onClick={unsave}
        disabled={busy}
        className="rounded border border-rose-300 bg-white px-2 py-1 text-xs font-medium text-rose-700 hover:bg-rose-50 disabled:opacity-50"
      >
        {busy ? 'Removing…' : 'Unsave'}
      </button>
    </li>
  );
}

export function My() {
  const { authStatus, user } = useAuthenticator((c) => [
    c.authStatus,
    c.user,
  ]);

  if (isStubBackend) {
    return (
      <div className="h-full overflow-auto p-6">
        <div className="mx-auto max-w-3xl rounded border border-amber-300 bg-amber-50 px-4 py-4 text-sm text-amber-800">
          <div className="text-base font-semibold">Backend not configured</div>
          <div className="mt-1">
            Run <code>npx ampx sandbox</code> at the repo root to see
            your templates here.
          </div>
        </div>
      </div>
    );
  }

  if (authStatus !== 'authenticated' || !user?.userId) {
    return (
      <div className="h-full overflow-auto p-6">
        <div className="mx-auto max-w-3xl rounded border border-slate-200 bg-white px-4 py-6 text-center text-slate-600">
          <div className="text-lg font-semibold text-slate-900">
            Sign in to see your templates
          </div>
          <p className="mt-2 text-sm">
            Use the <strong>Sign in</strong> button in the top-right.
          </p>
        </div>
      </div>
    );
  }

  return <MyContent ownerSub={user.userId} />;
}
