// User feedback intake (#42 MVP).
//
// Single-form page that writes a `Feedback` row through the public
// API key (anon allowed) or the caller's userPool token (authed). The
// admin portal reads from `listFeedbackBySource` to triage. No SES
// emailing yet — that's a follow-up under #42 once volume justifies it.
//
// Mounted at /feedback. Desktop deep-links here via the hamburger
// "Send feedback…" action with `?from=desktop&version=<v>` query
// params so we can attribute the report.

import { useAuthenticator } from '@aws-amplify/ui-react';
import '@aws-amplify/ui-react/styles.css';
import { useMemo, useState } from 'react';
import { Link, useSearchParams } from 'react-router-dom';
import { client, type FeedbackCreateInput } from '../amplify-client';
import { isStubBackend } from '../amplify-config';

type Severity = FeedbackCreateInput['severity'];

const SEVERITIES: { value: Severity; label: string; help: string }[] = [
  { value: 'bug', label: 'Bug', help: 'Something is broken or behaves wrong.' },
  {
    value: 'feature',
    label: 'Feature request',
    help: 'Something the app should do but does not.',
  },
  {
    value: 'question',
    label: 'Question',
    help: 'You want help understanding how something works.',
  },
  { value: 'other', label: 'Other', help: 'Anything that does not fit above.' },
];

type SubmitState =
  | { status: 'idle' }
  | { status: 'sending' }
  | { status: 'sent' }
  | { status: 'err'; message: string };

export function Feedback() {
  const { authStatus, user } = useAuthenticator((c) => [c.authStatus, c.user]);
  const [params] = useSearchParams();
  const fromDesktop = params.get('from') === 'desktop';
  const desktopVersion = params.get('version') ?? undefined;

  const [severity, setSeverity] = useState<Severity>('bug');
  const [message, setMessage] = useState('');
  const [email, setEmail] = useState('');
  const [state, setState] = useState<SubmitState>({ status: 'idle' });

  const userEmail: string | undefined = useMemo(() => {
    const attrs = (user as unknown as { attributes?: Record<string, string> } | undefined)
      ?.attributes;
    return attrs?.email;
  }, [user]);

  const effectiveEmail = email.trim() || userEmail || undefined;
  const userAgent = typeof navigator !== 'undefined' ? navigator.userAgent : undefined;

  async function submit(e: React.FormEvent) {
    e.preventDefault();
    if (!message.trim()) {
      setState({ status: 'err', message: 'Message is required.' });
      return;
    }
    if (isStubBackend) {
      setState({
        status: 'err',
        message: 'Backend not configured — feedback can only be sent on the live site.',
      });
      return;
    }
    setState({ status: 'sending' });
    try {
      const id =
        typeof crypto !== 'undefined' && 'randomUUID' in crypto
          ? crypto.randomUUID()
          : `${Date.now()}-${Math.random().toString(36).slice(2, 10)}`;
      const input: FeedbackCreateInput = {
        id,
        sourceApp: fromDesktop ? 'desktop' : 'web',
        version: desktopVersion,
        severity,
        message: message.trim(),
        contactEmail: effectiveEmail ?? null,
        userAgent,
        createdAtIso: new Date().toISOString(),
      };
      const authMode = authStatus === 'authenticated' ? 'userPool' : 'apiKey';
      const result = await client.models.Feedback.create(input, { authMode });
      if (result.errors && result.errors.length > 0) {
        setState({ status: 'err', message: result.errors[0].message });
        return;
      }
      setState({ status: 'sent' });
      setMessage('');
    } catch (err) {
      setState({
        status: 'err',
        message: err instanceof Error ? err.message : String(err),
      });
    }
  }

  return (
    <div className="mx-auto max-w-2xl px-6 py-10 text-slate-800">
      <h1 className="mb-2 text-2xl font-semibold">Send feedback</h1>
      <p className="mb-6 text-sm text-slate-600">
        Tell us what is wrong, what is missing, or what you would like to see. Reports go straight
        to the maintainer.{' '}
        {fromDesktop && desktopVersion && (
          <span className="ml-1 rounded bg-slate-100 px-2 py-0.5 text-xs text-slate-700">
            from desktop {desktopVersion}
          </span>
        )}
      </p>

      {state.status === 'sent' ? (
        <div className="rounded border border-emerald-200 bg-emerald-50 p-4 text-sm text-emerald-800">
          Thanks — your feedback was received. We read every report.
          <div className="mt-3">
            <button
              type="button"
              onClick={() => setState({ status: 'idle' })}
              className="rounded border border-emerald-300 bg-white px-3 py-1.5 text-xs font-medium hover:bg-emerald-100"
            >
              Send another
            </button>{' '}
            <Link to="/" className="ml-2 text-xs text-emerald-700 underline">
              Back to home
            </Link>
          </div>
        </div>
      ) : (
        <form onSubmit={submit} className="space-y-5">
          <fieldset>
            <legend className="mb-2 text-sm font-medium">Type</legend>
            <div className="grid grid-cols-2 gap-2 sm:grid-cols-4">
              {SEVERITIES.map((s) => (
                <label
                  key={s.value}
                  className={`flex cursor-pointer flex-col gap-1 rounded border p-3 text-xs ${
                    severity === s.value
                      ? 'border-indigo-500 bg-indigo-50'
                      : 'border-slate-200 hover:border-slate-300'
                  }`}
                >
                  <input
                    type="radio"
                    name="severity"
                    value={s.value}
                    checked={severity === s.value}
                    onChange={() => setSeverity(s.value)}
                    className="sr-only"
                  />
                  <span className="font-medium text-slate-900">{s.label}</span>
                  <span className="text-slate-500">{s.help}</span>
                </label>
              ))}
            </div>
          </fieldset>

          <label className="block">
            <span className="mb-1 block text-sm font-medium">Message</span>
            <textarea
              value={message}
              onChange={(e) => setMessage(e.target.value)}
              rows={8}
              required
              placeholder={
                severity === 'bug'
                  ? 'What happened? What did you expect to happen? Steps to reproduce help.'
                  : 'Tell us what you are thinking.'
              }
              className="w-full rounded border border-slate-300 px-3 py-2 text-sm focus:border-indigo-500 focus:outline-none focus:ring-1 focus:ring-indigo-500"
            />
          </label>

          <label className="block">
            <span className="mb-1 block text-sm font-medium">
              Contact email <span className="text-slate-400">(optional)</span>
            </span>
            <input
              type="email"
              value={email}
              onChange={(e) => setEmail(e.target.value)}
              placeholder={userEmail ?? 'you@example.com'}
              className="w-full rounded border border-slate-300 px-3 py-2 text-sm focus:border-indigo-500 focus:outline-none focus:ring-1 focus:ring-indigo-500"
            />
            <span className="mt-1 block text-xs text-slate-500">
              Provide only if you want a reply. We never share your email.
            </span>
          </label>

          {state.status === 'err' && (
            <div className="rounded border border-rose-200 bg-rose-50 p-3 text-sm text-rose-800">
              {state.message}
            </div>
          )}

          <div className="flex items-center gap-3">
            <button
              type="submit"
              disabled={state.status === 'sending'}
              className="rounded bg-indigo-600 px-4 py-2 text-sm font-medium text-white hover:bg-indigo-700 disabled:cursor-not-allowed disabled:opacity-50"
            >
              {state.status === 'sending' ? 'Sending…' : 'Send feedback'}
            </button>
            <Link to="/" className="text-sm text-slate-600 hover:text-slate-900">
              Cancel
            </Link>
          </div>
        </form>
      )}
    </div>
  );
}
