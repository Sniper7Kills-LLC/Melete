// Admin / superadmin dashboard.
//
// Visible to users with `admin` or `superadmin` in their Cognito
// groups (`cognito:groups` claim on the ID token). Anyone else hits
// the FORBIDDEN guard — server-side `@aws_auth` also enforces, so a
// client tamper just gets blocked at AppSync.
//
// Scope:
//   - Platform stats dashboard (AdminStats singleton).
//   - User search by email (Cognito proxy via `adminSearchUsers`).
//   - Superadmin actions per row: grant Pro, reset daily quota.
//     Requires `reason` field — passed straight to AdminAuditLog.
//
// Detailed user-detail + audit-log views are scoped for a follow-up.

import { useEffect, useState } from "react";
import { useAuthenticator } from "@aws-amplify/ui-react";
import "@aws-amplify/ui-react/styles.css";
import {
  client,
  type AdminStatsRow,
  type AdminUserSummary,
} from "../amplify-client";

function jwtPayload(idToken: string | undefined): Record<string, unknown> | null {
  if (!idToken) return null;
  const parts = idToken.split(".");
  if (parts.length < 2) return null;
  try {
    const json = atob(parts[1].replace(/-/g, "+").replace(/_/g, "/"));
    return JSON.parse(json) as Record<string, unknown>;
  } catch {
    return null;
  }
}

function useUserGroups(): string[] {
  const { user } = useAuthenticator((c) => [c.user]);
  // Amplify v6 puts the ID token on the auth session; for the simple
  // claim read we crack open the cached token directly. Falls through
  // to empty array if no session yet.
  const [groups, setGroups] = useState<string[]>([]);
  useEffect(() => {
    async function load() {
      try {
        const { fetchAuthSession } = await import("aws-amplify/auth");
        const session = await fetchAuthSession();
        const idTokenStr = session.tokens?.idToken?.toString();
        const claims = jwtPayload(idTokenStr);
        const g = claims?.["cognito:groups"];
        if (Array.isArray(g)) {
          setGroups(g.filter((x): x is string => typeof x === "string"));
        } else {
          setGroups([]);
        }
      } catch {
        setGroups([]);
      }
    }
    if (user) void load();
  }, [user]);
  return groups;
}

export function Admin() {
  const { authStatus } = useAuthenticator((c) => [c.authStatus]);
  const groups = useUserGroups();
  if (authStatus !== "authenticated") {
    return (
      <div className="p-8 text-slate-600">Sign in to access the admin panel.</div>
    );
  }
  if (!groups.includes("admin") && !groups.includes("superadmin")) {
    return (
      <div className="p-8 text-rose-700">
        Forbidden — you are not in the admin group.
      </div>
    );
  }
  return <AdminAuthorized isSuperadmin={groups.includes("superadmin")} />;
}

function AdminAuthorized({ isSuperadmin }: { isSuperadmin: boolean }) {
  return (
    <div className="h-full overflow-y-auto">
      <div className="mx-auto max-w-6xl space-y-8 p-8">
      <header>
        <h1 className="text-3xl font-semibold">Admin</h1>
        <p className="mt-1 text-sm text-slate-600">
          {isSuperadmin
            ? "Superadmin — read + write access."
            : "Admin — read-only access."}
        </p>
      </header>
      <StatsSection />
      <UserSearchSection isSuperadmin={isSuperadmin} />
      </div>
    </div>
  );
}

function StatsSection() {
  const [stats, setStats] = useState<AdminStatsRow | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let alive = true;
    async function load() {
      try {
        const r = await client.models.AdminStats.get({ id: "global" });
        if (!alive) return;
        setStats(r.data);
      } catch (e) {
        if (!alive) return;
        setError((e as Error).message);
      } finally {
        if (alive) setLoading(false);
      }
    }
    void load();
    return () => {
      alive = false;
    };
  }, []);

  if (loading) return <div>Loading stats…</div>;
  if (error) return <div className="text-rose-700">Stats error: {error}</div>;
  if (!stats) {
    return (
      <div className="rounded border border-slate-200 bg-white p-4 text-sm text-slate-600">
        No stats row yet — the DDB stream maintainer hasn't fired (no
        UserEntitlement / Notebook writes since last deploy).
      </div>
    );
  }

  return (
    <section>
      <h2 className="mb-3 text-xl font-semibold">Platform stats</h2>
      <div className="grid grid-cols-2 gap-3 md:grid-cols-4">
        <Stat label="Total users" value={stats.totalUsers} />
        <Stat label="MRR" value={`$${(stats.mrrCents / 100).toFixed(2)}`} />
        <Stat label="Free" value={stats.freeUsers} />
        <Stat label="Pro" value={stats.proUsers} />
        <Stat label="Studio" value={stats.studioUsers} />
        <Stat label="Trialing" value={stats.trialingUsers} />
        <Stat label="Past due" value={stats.pastDueUsers} />
        <Stat label="Canceled" value={stats.canceledUsers} />
        <Stat label="Notebooks" value={stats.totalNotebooks} />
      </div>
      <p className="mt-2 text-xs text-slate-400">
        Last updated:{" "}
        {stats.lastUpdatedIso
          ? new Date(stats.lastUpdatedIso).toLocaleString()
          : "never"}
      </p>
    </section>
  );
}

function Stat({ label, value }: { label: string; value: React.ReactNode }) {
  return (
    <div className="rounded border border-slate-200 bg-white p-3">
      <div className="text-xs text-slate-500">{label}</div>
      <div className="text-xl font-semibold text-slate-900">{value}</div>
    </div>
  );
}

function UserSearchSection({ isSuperadmin }: { isSuperadmin: boolean }) {
  const [email, setEmail] = useState("");
  const [results, setResults] = useState<AdminUserSummary[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function search() {
    setBusy(true);
    setError(null);
    try {
      const r = await client.mutations.adminSearchUsers({ email });
      if (r.errors && r.errors.length > 0) {
        setError(r.errors.map((e) => e.message).join("\n"));
        return;
      }
      setResults(r.data?.items ?? []);
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setBusy(false);
    }
  }

  async function runMutate(action: string, targetUserId: string) {
    const reason = window.prompt(`Reason for ${action}:`);
    if (!reason || reason.trim().length === 0) return;
    const payload =
      action === "grantTier" ? JSON.stringify({ tier: "pro" }) : "{}";
    setBusy(true);
    setError(null);
    try {
      const r = await client.mutations.adminMutate({
        action,
        targetUserId,
        payload,
        reason,
      });
      if (r.errors && r.errors.length > 0) {
        setError(r.errors.map((e) => e.message).join("\n"));
      } else {
        window.alert(`${action} applied`);
      }
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setBusy(false);
    }
  }

  return (
    <section>
      <h2 className="mb-3 text-xl font-semibold">Find user by email</h2>
      <div className="flex gap-2">
        <input
          type="email"
          value={email}
          onChange={(e) => setEmail(e.target.value)}
          placeholder="email or prefix"
          className="flex-1 rounded border border-slate-300 px-3 py-2 text-sm"
        />
        <button
          type="button"
          onClick={() => void search()}
          disabled={busy || email.trim().length === 0}
          className="rounded bg-slate-900 px-4 py-2 text-sm font-medium text-white hover:bg-slate-800 disabled:opacity-50"
        >
          {busy ? "Searching…" : "Search"}
        </button>
      </div>
      {error && (
        <pre className="mt-3 whitespace-pre-wrap rounded border border-rose-200 bg-rose-50 p-3 text-xs text-rose-800">
          {error}
        </pre>
      )}
      {results.length > 0 && (
        <table className="mt-4 w-full table-auto border-collapse text-sm">
          <thead>
            <tr className="border-b border-slate-200 text-left text-slate-500">
              <th className="py-2 pr-4">Email</th>
              <th className="py-2 pr-4">Status</th>
              <th className="py-2 pr-4">Enabled</th>
              <th className="py-2 pr-4">Created</th>
              {isSuperadmin && <th className="py-2 pr-4">Actions</th>}
            </tr>
          </thead>
          <tbody>
            {results.map((u) => (
              <tr key={u.userId} className="border-b border-slate-100">
                <td className="py-2 pr-4 font-medium">{u.email}</td>
                <td className="py-2 pr-4 text-slate-600">{u.status}</td>
                <td className="py-2 pr-4 text-slate-600">
                  {u.enabled ? "yes" : "no"}
                </td>
                <td className="py-2 pr-4 text-slate-500">
                  {u.createdAtIso
                    ? new Date(u.createdAtIso).toLocaleDateString()
                    : "—"}
                </td>
                {isSuperadmin && (
                  <td className="py-2 pr-4">
                    <div className="flex gap-2">
                      <button
                        type="button"
                        onClick={() => void runMutate("grantTier", u.userId)}
                        className="rounded border border-slate-300 px-2 py-1 text-xs hover:bg-slate-100"
                      >
                        Grant Pro
                      </button>
                      <button
                        type="button"
                        onClick={() =>
                          void runMutate("resetDailyUsage", u.userId)
                        }
                        className="rounded border border-slate-300 px-2 py-1 text-xs hover:bg-slate-100"
                      >
                        Reset daily
                      </button>
                    </div>
                  </td>
                )}
              </tr>
            ))}
          </tbody>
        </table>
      )}
      {results.length === 0 && email.length > 0 && !busy && !error && (
        <div className="mt-4 text-sm text-slate-500">
          No matches. (Search hasn't run, or pool empty.)
        </div>
      )}
    </section>
  );
}
