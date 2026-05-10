// Signed-in "my templates" view.
//
// Wrapped in `<Authenticator>` so the UI shows the Cognito sign-in flow
// when the visitor is anonymous. The auth-mode for these queries is
// `userPool` (Amplify Gen 2's default), which is what the schema's
// `allow.owner()` rule honors — visitors only see rows where their
// Cognito sub matches `owner`.

import { Authenticator } from '@aws-amplify/ui-react';
import '@aws-amplify/ui-react/styles.css';
import { useEffect, useState } from 'react';
import {
  client,
  type PageTemplateRow as PageTemplate,
  type NotebookTemplateRow as NotebookTemplate,
  type BrushRow as Brush,
} from '../amplify-client';
import { isStubBackend } from '../amplify-config';

type LoadState<T> =
  | { status: 'loading' }
  | { status: 'ok'; rows: T[] }
  | { status: 'err'; message: string };

function useMyList<T extends { id: string }>(
  ownerSub: string | undefined,
  fetcher: (
    sub: string,
  ) => Promise<{ data: T[] | null; errors?: { message: string }[] }>,
): LoadState<T> {
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
  return state;
}

function MySection<
  T extends { id: string; name: string; visibility: string; description?: string | null },
>({
  title,
  state,
}: {
  title: string;
  state: LoadState<T>;
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
              className="flex items-center justify-between rounded border border-slate-200 bg-white px-3 py-2"
            >
              <div>
                <div className="text-sm font-medium text-slate-900">
                  {row.name}
                </div>
                {row.description && (
                  <div className="text-xs text-slate-500">
                    {row.description}
                  </div>
                )}
              </div>
              <span
                className={`rounded px-2 py-0.5 text-[10px] font-semibold uppercase tracking-wide ${
                  row.visibility === 'PUBLIC'
                    ? 'bg-emerald-100 text-emerald-700'
                    : row.visibility === 'UNLISTED'
                      ? 'bg-amber-100 text-amber-700'
                      : 'bg-slate-100 text-slate-600'
                }`}
              >
                {row.visibility}
              </span>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}

function MyContent({
  signOut,
  email,
  ownerSub,
}: {
  signOut: (() => void) | undefined;
  email: string | undefined;
  ownerSub: string | undefined;
}) {
  const pages = useMyList<PageTemplate>(ownerSub, (sub) =>
    client.models.PageTemplate.listPageTemplatesByOwner({ owner: sub }),
  );
  const notebooks = useMyList<NotebookTemplate>(ownerSub, (sub) =>
    client.models.NotebookTemplate.listNotebookTemplatesByOwner({ owner: sub }),
  );
  const brushes = useMyList<Brush>(ownerSub, (sub) =>
    client.models.Brush.listBrushesByOwner({ owner: sub }),
  );

  return (
    <div className="h-full overflow-auto p-6">
      <div className="mx-auto max-w-5xl space-y-6">
        <header className="flex items-start justify-between gap-4">
          <div>
            <h1 className="text-2xl font-semibold text-slate-900">
              My templates
            </h1>
            <p className="text-sm text-slate-600">
              Templates and brushes you have authored. Only you can see
              private rows.
            </p>
            {isStubBackend && (
              <div className="mt-3 rounded border border-amber-300 bg-amber-50 px-3 py-2 text-sm text-amber-800">
                <strong>Backend not configured.</strong> Run{' '}
                <code>npx ampx sandbox</code> at the repo root.
              </div>
            )}
          </div>
          <div className="flex items-center gap-3 text-sm">
            {email && <span className="text-slate-600">{email}</span>}
            <button
              type="button"
              onClick={signOut}
              className="rounded border border-slate-300 bg-white px-3 py-1.5 text-slate-700 hover:bg-slate-100"
            >
              Sign out
            </button>
          </div>
        </header>
        <MySection title="Page templates" state={pages} />
        <MySection title="Notebook templates" state={notebooks} />
        <MySection title="Brushes" state={brushes} />
      </div>
    </div>
  );
}

export function My() {
  return (
    <Authenticator loginMechanisms={['email']} signUpAttributes={['email']}>
      {({ signOut, user }) => (
        <MyContent
          signOut={signOut}
          email={user?.signInDetails?.loginId}
          ownerSub={user?.userId}
        />
      )}
    </Authenticator>
  );
}
