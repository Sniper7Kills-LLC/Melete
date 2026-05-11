import { useEffect, useState } from "react";
import { Link } from "react-router-dom";
import { Authenticator, useAuthenticator } from "@aws-amplify/ui-react";
import "@aws-amplify/ui-react/styles.css";

import { isStubBackend } from "@/amplify-config";
import { useUsage } from "@/useUsage";
import { client, type UserEntitlementRow } from "@/amplify-client";

/**
 * Persistent header account chip. Anonymous → "Sign in" button that
 * opens an Authenticator modal. Authenticated → email + dropdown
 * menu (Sign out). Stub backend → renders nothing (auth would 401
 * against the stub config).
 *
 * Wraps everything in `Authenticator.Provider` upstream (see main.tsx)
 * so this component and any other route can read `useAuthenticator`
 * without remounting.
 */
export function AccountChip() {
  if (isStubBackend) return null;
  return <AccountChipImpl />;
}

function AccountChipImpl() {
  const { authStatus, user, signOut } = useAuthenticator((c) => [
    c.authStatus,
    c.user,
  ]);
  const [signInOpen, setSignInOpen] = useState(false);
  const [menuOpen, setMenuOpen] = useState(false);

  // Auto-close the sign-in modal once auth flips to authenticated.
  useEffect(() => {
    if (authStatus === "authenticated" && signInOpen) {
      setSignInOpen(false);
    }
  }, [authStatus, signInOpen]);

  if (authStatus === "configuring") {
    return (
      <span className="text-xs text-slate-400">Auth…</span>
    );
  }

  if (authStatus !== "authenticated") {
    return (
      <>
        <button
          type="button"
          onClick={() => setSignInOpen(true)}
          className="rounded border border-slate-300 bg-white px-3 py-1 text-xs font-medium text-slate-700 hover:bg-slate-100"
        >
          Sign in
        </button>
        {signInOpen && (
          <SignInModal onClose={() => setSignInOpen(false)} />
        )}
      </>
    );
  }

  const email = user?.signInDetails?.loginId ?? "Account";
  return (
    <div className="relative">
      <button
        type="button"
        onClick={() => setMenuOpen((v) => !v)}
        className="flex items-center gap-2 rounded border border-slate-300 bg-white px-2 py-1 text-xs text-slate-700 hover:bg-slate-100"
      >
        <span
          className="grid h-5 w-5 place-items-center rounded-full bg-indigo-600 text-[10px] font-semibold uppercase text-white"
          aria-hidden
        >
          {email.charAt(0)}
        </span>
        <span className="max-w-[160px] truncate font-medium">{email}</span>
        <span className="text-slate-400" aria-hidden>
          ▾
        </span>
      </button>
      {menuOpen && (
        <>
          <div
            className="fixed inset-0 z-10"
            onClick={() => setMenuOpen(false)}
          />
          <div className="absolute right-0 top-full z-20 mt-1 w-52 rounded border border-slate-200 bg-white py-1 text-sm shadow-lg">
            <div className="border-b border-slate-100 px-3 py-2 text-[11px] uppercase tracking-wide text-slate-400">
              Signed in as
            </div>
            <div className="px-3 py-1 pb-2 text-xs text-slate-700">{email}</div>
            <div className="border-t border-slate-100" />
            <UsageBlock
              userId={user?.userId}
              onNavigate={() => setMenuOpen(false)}
            />
            <div className="border-t border-slate-100" />
            <Link
              to="/my"
              onClick={() => setMenuOpen(false)}
              className="block px-3 py-2 text-slate-700 hover:bg-slate-100"
            >
              My library
            </Link>
            <Link
              to="/billing"
              onClick={() => setMenuOpen(false)}
              className="block px-3 py-2 text-slate-700 hover:bg-slate-100"
            >
              Billing
            </Link>
            <div className="border-t border-slate-100" />
            <button
              type="button"
              onClick={() => {
                setMenuOpen(false);
                signOut?.();
              }}
              className="block w-full px-3 py-2 text-left text-sm text-rose-700 hover:bg-rose-50"
            >
              Sign out
            </button>
          </div>
        </>
      )}
    </div>
  );
}

function UsageBlock({
  userId,
  onNavigate,
}: {
  userId: string | undefined;
  onNavigate: () => void;
}) {
  const { usage } = useUsage(userId);
  const [entitlement, setEntitlement] = useState<UserEntitlementRow | null>(
    null,
  );
  useEffect(() => {
    let alive = true;
    async function load() {
      if (!userId) return;
      try {
        const r = await client.models.UserEntitlement.get({ id: userId });
        if (!alive) return;
        setEntitlement(r.data);
      } catch {
        // Soft-fail — UsageBlock degrades to "showing usage / —".
      }
    }
    void load();
    return () => {
      alive = false;
    };
  }, [userId]);

  const tier = entitlement?.tier ?? "free";
  // Free defaults mirror TierConfig seed values; kept here to avoid a
  // second round-trip just to render a dropdown.
  const notebookCap = entitlement?.notebookCap ?? 1;
  const dailyCap = entitlement?.dailyWriteCap ?? 1000;

  return (
    <Link
      to="/billing"
      onClick={onNavigate}
      className="block px-3 py-2 hover:bg-slate-50"
    >
      <div className="flex items-baseline justify-between">
        <span className="text-[11px] font-medium uppercase tracking-wide text-slate-500">
          Usage
        </span>
        <span className="rounded bg-slate-900 px-1.5 py-0.5 text-[10px] font-semibold uppercase tracking-wide text-white">
          {tier}
        </span>
      </div>
      <div className="mt-1.5 space-y-1.5">
        <UsageMini
          label="Notebooks"
          used={usage?.notebooksUsed ?? 0}
          cap={notebookCap}
        />
        <UsageMini
          label="Strokes today"
          used={usage?.strokesToday ?? 0}
          cap={dailyCap}
        />
      </div>
    </Link>
  );
}

function UsageMini({
  label,
  used,
  cap,
}: {
  label: string;
  used: number;
  cap: number;
}) {
  const pct = Math.min(Math.max(used / Math.max(cap, 1), 0), 1);
  const barColor =
    pct >= 0.9
      ? "bg-rose-500"
      : pct >= 0.7
        ? "bg-amber-500"
        : "bg-emerald-500";
  return (
    <div>
      <div className="flex items-baseline justify-between text-[11px] text-slate-600">
        <span>{label}</span>
        <span className="font-medium tabular-nums text-slate-700">
          {used.toLocaleString()} / {cap.toLocaleString()}
        </span>
      </div>
      <div className="mt-0.5 h-1 w-full overflow-hidden rounded-full bg-slate-200">
        <div
          className={`h-full ${barColor} transition-all`}
          style={{ width: `${pct * 100}%` }}
        />
      </div>
    </div>
  );
}

function SignInModal({ onClose }: { onClose: () => void }) {
  return (
    <div
      className="fixed inset-0 z-30 flex items-center justify-center bg-slate-900/50 p-4"
      onClick={onClose}
    >
      <div
        className="relative w-full max-w-md rounded-lg bg-white p-2 shadow-2xl"
        onClick={(e) => e.stopPropagation()}
      >
        <button
          type="button"
          onClick={onClose}
          className="absolute right-2 top-2 z-10 rounded p-1 text-slate-400 hover:bg-slate-100"
          aria-label="Close"
        >
          ✕
        </button>
        <Authenticator
          loginMechanisms={["email"]}
          signUpAttributes={["email"]}
        />
      </div>
    </div>
  );
}
