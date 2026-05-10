// Signed-in "my templates" view. AccountChip in the header drives
// auth — this page reads `useAuthenticator` to decide whether to show
// content, a sign-in prompt, or the stub-mode banner. The data-mode
// queries use Cognito userPool auth (the schema's `allow.owner()`
// rule scopes results to the caller's Cognito sub).

import { useAuthenticator } from '@aws-amplify/ui-react';
import '@aws-amplify/ui-react/styles.css';
import { useEffect, useState } from 'react';
import {
  client,
  type BrushRow as Brush,
  type NotebookTemplateRow as NotebookTemplate,
  type PageTemplateRow as PageTemplate,
  type Visibility,
} from '../amplify-client';
import { isStubBackend } from '../amplify-config';

type LoadState<T> =
  | { status: 'loading' }
  | { status: 'ok'; rows: T[] }
  | { status: 'err'; message: string };

type ModelKind = 'PageTemplate' | 'NotebookTemplate' | 'Brush';

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
  }
}

const VISIBILITIES: Visibility[] = ['PRIVATE', 'UNLISTED', 'PUBLIC'];

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

  return (
    <div className="h-full overflow-auto p-6">
      <div className="mx-auto max-w-5xl space-y-6">
        <header>
          <h1 className="text-2xl font-semibold text-slate-900">
            My templates
          </h1>
          <p className="text-sm text-slate-600">
            Templates and brushes you have authored. Use the visibility
            menu on each row to publish — <strong>UNLISTED</strong>{' '}
            shows only via direct link, <strong>PUBLIC</strong> appears
            in the Gallery.
          </p>
        </header>
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
      </div>
    </div>
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
