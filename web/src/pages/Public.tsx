// Anonymous public-template browse.
//
// Uses API-key auth mode so a signed-out visitor can land on this page and
// see what's published. The schema's `allow.publicApiKey().to(['read'])`
// rule is what permits this; if you flip it off, this page goes 401.

import { useEffect, useState } from 'react';
import {
  client,
  type PageTemplateRow as PageTemplate,
  type NotebookTemplateRow as NotebookTemplate,
  type BrushRow as Brush,
} from '../amplify-client';
import { isStubBackend } from '../amplify-config';

type PublicState<T> =
  | { status: 'idle' }
  | { status: 'loading' }
  | { status: 'ok'; rows: T[] }
  | { status: 'err'; message: string };

function useListPublic<T extends { id: string }>(
  fetcher: () => Promise<{ data: T[] | null; errors?: { message: string }[] }>,
): PublicState<T> {
  const [state, setState] = useState<PublicState<T>>({ status: 'idle' });
  useEffect(() => {
    if (isStubBackend) {
      setState({
        status: 'err',
        message: 'Backend not configured (stub mode).',
      });
      return;
    }
    let cancelled = false;
    setState({ status: 'loading' });
    fetcher()
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
  }, []);
  return state;
}

function ListSection<T extends { id: string; name: string; description?: string | null }>({
  title,
  state,
}: {
  title: string;
  state: PublicState<T>;
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
        <p className="text-sm text-slate-400">No public {title.toLowerCase()} yet.</p>
      )}
      {state.status === 'ok' && state.rows.length > 0 && (
        <ul className="grid grid-cols-1 gap-2 sm:grid-cols-2 lg:grid-cols-3">
          {state.rows.map((row) => (
            <li
              key={row.id}
              className="rounded border border-slate-200 bg-white px-3 py-2 shadow-sm"
            >
              <div className="text-sm font-medium text-slate-900">
                {row.name}
              </div>
              {row.description && (
                <div className="text-xs text-slate-500">{row.description}</div>
              )}
              <div className="mt-1 text-[10px] text-slate-400">{row.id}</div>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}

export function Public() {
  const pages = useListPublic<PageTemplate>(() =>
    client.models.PageTemplate.list({
      filter: { visibility: { eq: 'PUBLIC' } },
      authMode: 'apiKey',
    }),
  );
  const notebooks = useListPublic<NotebookTemplate>(() =>
    client.models.NotebookTemplate.list({
      filter: { visibility: { eq: 'PUBLIC' } },
      authMode: 'apiKey',
    }),
  );
  const brushes = useListPublic<Brush>(() =>
    client.models.Brush.list({
      filter: { visibility: { eq: 'PUBLIC' } },
      authMode: 'apiKey',
    }),
  );

  return (
    <div className="h-full overflow-auto p-6">
      <div className="mx-auto max-w-5xl space-y-6">
        <header>
          <h1 className="text-2xl font-semibold text-slate-900">
            Public templates
          </h1>
          <p className="text-sm text-slate-600">
            Browse templates and brushes shared by the community. No sign-in
            required.
          </p>
          {isStubBackend && (
            <div className="mt-3 rounded border border-amber-300 bg-amber-50 px-3 py-2 text-sm text-amber-800">
              <strong>Backend not configured.</strong> No real{' '}
              <code>amplify_outputs.json</code> at the repo root. Run{' '}
              <code>npx ampx sandbox</code> from the repo root, or set up
              Amplify Hosting to populate it.
            </div>
          )}
        </header>
        <ListSection title="Page templates" state={pages} />
        <ListSection title="Notebook templates" state={notebooks} />
        <ListSection title="Brushes" state={brushes} />
      </div>
    </div>
  );
}
