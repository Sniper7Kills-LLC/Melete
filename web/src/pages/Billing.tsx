// Billing / subscription management page.
//
// Three states:
//   1. Not signed in → render Authenticator prompt; Stripe ops require auth.
//   2. Signed in, no UserEntitlement row → treat as free tier defaults.
//   3. Signed in with row → render current tier, usage caps, upgrade /
//      manage / add-on actions.
//
// Stripe Checkout + Customer Portal URLs are minted server-side via
// `createCheckoutSession` / `createPortalSession` mutations and we
// `window.location.assign` to them. Add-on purchases (extra notebook,
// extra storage) use the same Checkout mutation with different tier /
// interval combinations — backend resolves which Stripe price ID to
// hand to Checkout.

import { useEffect, useState } from "react";
import { useAuthenticator, Authenticator } from "@aws-amplify/ui-react";
import "@aws-amplify/ui-react/styles.css";
import {
  client,
  type TierConfigRow,
  type UserEntitlementRow,
} from "../amplify-client";
import { useUsage } from "../useUsage";

interface Plan {
  id: "free" | "pro" | "studio";
  name: string;
  priceMonthly: string;
  priceYearly: string;
  features: string[];
  highlight?: boolean;
}

// Hardcoded fallback caps so the page renders even when the
// TierConfig query returns nothing (auth, missing rows, etc).
// The numbers here mirror the seeded values in
// `amplify/scripts/seed-tier-config.ts` — keep in sync.
const FALLBACK: Record<
  "free" | "pro" | "studio",
  Omit<TierConfigRow, "stripePriceIdMonthly" | "stripePriceIdYearly">
> = {
  free: {
    id: "free",
    notebookCap: 1,
    strokesPerPageCap: 10000,
    strokesPerNotebookCap: 50000,
    dailyWriteCap: 1000,
    s3BytesCap: 50 * 1024 * 1024,
    templatePublishCap: 3,
    historyDays: 0,
    liveSyncEnabled: false,
    priceMonthlyCents: 0,
    priceYearlyCents: 0,
  },
  pro: {
    id: "pro",
    notebookCap: 10,
    strokesPerPageCap: 100000,
    strokesPerNotebookCap: 2000000,
    dailyWriteCap: 30000,
    s3BytesCap: 10 * 1024 * 1024 * 1024,
    templatePublishCap: 50,
    historyDays: 0,
    liveSyncEnabled: true,
    priceMonthlyCents: 800,
    priceYearlyCents: 8000,
  },
  studio: {
    id: "studio",
    notebookCap: 20,
    strokesPerPageCap: 200000,
    strokesPerNotebookCap: 2000000,
    dailyWriteCap: 60000,
    s3BytesCap: 30 * 1024 * 1024 * 1024,
    templatePublishCap: -1,
    historyDays: 0,
    liveSyncEnabled: true,
    priceMonthlyCents: 1800,
    priceYearlyCents: 18000,
  },
};

function buildPlanFromTier(
  id: "free" | "pro" | "studio",
  cfg?: TierConfigRow,
): Plan {
  const effective = cfg ?? FALLBACK[id];
  const mo =
    typeof effective.priceMonthlyCents === "number"
      ? `$${(effective.priceMonthlyCents / 100).toFixed(0)}`
      : "—";
  const yr =
    typeof effective.priceYearlyCents === "number"
      ? `$${(effective.priceYearlyCents / 100).toFixed(0)}`
      : "—";
  const features: string[] = [
    `${effective.notebookCap} notebook${effective.notebookCap === 1 ? "" : "s"}`,
    `${effective.dailyWriteCap.toLocaleString()} strokes/day`,
    `${(effective.s3BytesCap / 1024 / 1024 / 1024).toFixed(0)} GB storage`,
    `${effective.strokesPerNotebookCap.toLocaleString()} strokes/notebook`,
    `${effective.strokesPerPageCap.toLocaleString()} strokes/page`,
    effective.templatePublishCap === -1
      ? "Unlimited template publish"
      : `${effective.templatePublishCap} template publish`,
    effective.liveSyncEnabled ? "Live multi-device sync" : "No live sync",
  ];
  const niceName = id === "free" ? "Free" : id === "pro" ? "Pro" : "Studio";
  return {
    id,
    name: niceName,
    priceMonthly: id === "free" ? "$0" : `${mo}/mo`,
    priceYearly: id === "free" ? "$0" : `${yr}/yr`,
    features,
  };
}

export function Billing() {
  const { authStatus } = useAuthenticator((c) => [c.authStatus]);
  if (authStatus !== "authenticated") {
    return (
      <div className="mx-auto max-w-md p-8">
        <h1 className="mb-4 text-2xl font-semibold">Subscription</h1>
        <p className="mb-4 text-sm text-slate-600">Sign in to manage your subscription.</p>
        <Authenticator loginMechanisms={["email"]} signUpAttributes={["email"]} />
      </div>
    );
  }
  return <BillingAuthenticated />;
}

function BillingAuthenticated() {
  const { user } = useAuthenticator((c) => [c.user]);
  const sub = user?.userId;
  const [entitlement, setEntitlement] = useState<UserEntitlementRow | null>(
    null,
  );
  const [tiers, setTiers] = useState<Record<string, TierConfigRow>>({});
  const [loading, setLoading] = useState(true);
  const [actionBusy, setActionBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  // Hooks for the post-load render — must live above the loading /
  // error early returns so React sees the same hook order on every
  // render (Rules of Hooks).
  const [interval, setInterval] = useState<"monthly" | "yearly">("monthly");
  const [confirmTier, setConfirmTier] = useState<"pro" | "studio" | null>(
    null,
  );
  const { usage } = useUsage(sub);

  useEffect(() => {
    let alive = true;
    async function load() {
      try {
        const [ent, tlist] = await Promise.all([
          sub ? client.models.UserEntitlement.get({ id: sub }) : Promise.resolve({ data: null }),
          client.models.TierConfig.list({ limit: 10 }),
        ]);
        if (!alive) return;
        setEntitlement(ent.data);
        const map: Record<string, TierConfigRow> = {};
        for (const t of tlist.data ?? []) {
          // Amplify Data may return null slots for rows the caller
          // can't see (auth-filtered) — drop them defensively.
          if (t && typeof t.id === "string") {
            map[t.id] = t;
          }
        }
        setTiers(map);
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
  }, [sub]);

  if (loading) {
    return <div className="p-8 text-slate-600">Loading subscription…</div>;
  }
  if (error) {
    return <div className="p-8 text-rose-700">Error: {error}</div>;
  }

  const currentTier = (entitlement?.tier ?? "free") as
    | "free"
    | "pro"
    | "studio";
  const plans: Plan[] = [
    buildPlanFromTier("free", tiers.free),
    buildPlanFromTier("pro", tiers.pro),
    buildPlanFromTier("studio", tiers.studio),
  ];
  // Effective caps for the "Current plan" panel — prefer the actual
  // UserEntitlement row, fall back to the tier defaults (covers
  // free-tier users with no row yet).
  const effectiveCaps = entitlement ?? {
    notebookCap: FALLBACK[currentTier].notebookCap,
    dailyWriteCap: FALLBACK[currentTier].dailyWriteCap,
    s3BytesCap: FALLBACK[currentTier].s3BytesCap,
    strokesPerPageCap: FALLBACK[currentTier].strokesPerPageCap,
    strokesPerNotebookCap: FALLBACK[currentTier].strokesPerNotebookCap,
    templatePublishCap: FALLBACK[currentTier].templatePublishCap,
    liveSyncEnabled: FALLBACK[currentTier].liveSyncEnabled,
    periodEnd: null,
    stripeCustomerId: null,
  };

  async function checkout(tier: string, interval: string) {
    setActionBusy(`checkout-${tier}-${interval}`);
    setError(null);
    try {
      const r = await client.mutations.createCheckoutSession({ tier, interval });
      if (r.errors && r.errors.length > 0) {
        setError(r.errors.map((e) => e.message).join("\n"));
        return;
      }
      const url = r.data?.url;
      if (url) {
        window.location.assign(url);
      } else {
        setError("No checkout URL returned.");
      }
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setActionBusy(null);
    }
  }

  async function portal() {
    setActionBusy("portal");
    setError(null);
    try {
      const r = await client.mutations.createPortalSession({});
      if (r.errors && r.errors.length > 0) {
        setError(r.errors.map((e) => e.message).join("\n"));
        return;
      }
      const url = r.data?.url;
      if (url) {
        window.location.assign(url);
      } else {
        setError("No portal URL returned.");
      }
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setActionBusy(null);
    }
  }

  return (
    <div className="h-full overflow-y-auto">
      <div className="mx-auto max-w-6xl space-y-10 p-8">
      <header className="border-b border-slate-200 pb-6">
        <p className="text-xs font-medium uppercase tracking-wide text-slate-500">
          Plans &amp; pricing
        </p>
        <h1 className="mt-2 text-3xl font-semibold text-slate-900">
          Your subscription
        </h1>
        <p className="mt-2 max-w-xl text-sm text-slate-600">
          Local-first stays free forever. Cloud sync, more notebooks, and
          live multi-device — pick what fits, change your mind whenever.
        </p>
      </header>

      {/* Current plan summary */}
      <section className="rounded-lg border border-slate-200 bg-white p-6">
        <div className="flex items-start justify-between gap-4">
          <div>
            <div className="text-xs font-medium uppercase tracking-wide text-slate-500">
              Current plan
            </div>
            <div className="mt-1 flex items-baseline gap-3">
              <span className="text-3xl font-semibold text-slate-900">
                {currentTier === "free"
                  ? "Free"
                  : currentTier === "pro"
                    ? "Pro"
                    : "Studio"}
              </span>
              <StatusBadge status={entitlement?.status ?? "active"} />
            </div>
            {entitlement?.periodEnd && (
              <div className="mt-2 text-xs text-slate-500">
                Renews / ends:{" "}
                {new Date(entitlement.periodEnd).toLocaleDateString()}
              </div>
            )}
          </div>
          {entitlement?.stripeCustomerId && (
            <button
              type="button"
              disabled={actionBusy === "portal"}
              onClick={() => void portal()}
              className="rounded border border-slate-300 bg-white px-4 py-2 text-sm font-medium text-slate-700 hover:bg-slate-100 disabled:opacity-50"
            >
              {actionBusy === "portal" ? "Opening…" : "Manage subscription"}
            </button>
          )}
        </div>
        <dl className="mt-6 grid grid-cols-2 gap-x-8 gap-y-4 border-t border-slate-200 pt-5 text-sm md:grid-cols-3">
          <UsageStat
            label="Notebooks"
            value={
              usage
                ? `${usage.notebooksUsed} / ${effectiveCaps.notebookCap}`
                : effectiveCaps.notebookCap
            }
            progress={
              usage
                ? usage.notebooksUsed / Math.max(effectiveCaps.notebookCap, 1)
                : undefined
            }
          />
          <UsageStat
            label="Daily writes"
            value={
              usage
                ? `${usage.strokesToday.toLocaleString()} / ${effectiveCaps.dailyWriteCap.toLocaleString()}`
                : effectiveCaps.dailyWriteCap.toLocaleString()
            }
            progress={
              usage
                ? usage.strokesToday /
                  Math.max(effectiveCaps.dailyWriteCap, 1)
                : undefined
            }
          />
          <UsageStat
            label="Storage"
            value={`${(effectiveCaps.s3BytesCap / 1024 / 1024 / 1024).toFixed(0)} GB`}
          />
          <UsageStat
            label="Strokes / notebook"
            value={effectiveCaps.strokesPerNotebookCap.toLocaleString()}
          />
          <UsageStat
            label="Strokes / page"
            value={effectiveCaps.strokesPerPageCap.toLocaleString()}
          />
          <UsageStat
            label="Template publish"
            value={
              effectiveCaps.templatePublishCap === -1
                ? "Unlimited"
                : effectiveCaps.templatePublishCap
            }
          />
          <UsageStat
            label="Live sync"
            value={effectiveCaps.liveSyncEnabled ? "Enabled" : "—"}
          />
        </dl>
      </section>

      {/* Plans grid */}
      <section className="space-y-4">
        <div className="flex items-end justify-between gap-6 border-b border-slate-200 pb-3">
          <div>
            <p className="text-xs font-medium uppercase tracking-wide text-slate-500">
              Three editions
            </p>
            <h2 className="mt-1 text-2xl font-semibold text-slate-900">
              Pick a plan
            </h2>
          </div>
          <IntervalToggle interval={interval} onChange={setInterval} />
        </div>
        <div className="grid overflow-hidden rounded-lg border border-slate-200 md:grid-cols-3">
          {plans.map((p, idx) => {
            const isCurrent = p.id === currentTier;
            const isFeatured = p.id === "pro";
            const price =
              interval === "monthly" ? p.priceMonthly : p.priceYearly;
            const altPrice =
              interval === "monthly" ? p.priceYearly : p.priceMonthly;
            const altLabel =
              interval === "monthly" ? "or yearly" : "or monthly";
            const priceFigure = price.split("/")[0].trim();
            return (
              <div
                key={p.id}
                className={`relative flex flex-col p-6 transition-colors ${
                  isCurrent
                    ? "bg-indigo-50/60"
                    : isFeatured
                      ? "bg-slate-50/60"
                      : "bg-white hover:bg-slate-50/60"
                } ${idx > 0 ? "md:border-l md:border-slate-200" : ""}`}
              >
                {isFeatured && !isCurrent && (
                  <span className="absolute right-4 top-4 rounded-full bg-slate-900 px-2 py-0.5 text-[10px] font-semibold uppercase tracking-wide text-white">
                    Recommended
                  </span>
                )}
                <div className="flex items-center justify-between">
                  <h3 className="text-lg font-semibold text-slate-900">
                    {p.name}
                  </h3>
                  {isCurrent && (
                    <span className="rounded-full bg-indigo-600 px-2 py-0.5 text-[10px] font-semibold uppercase tracking-wide text-white">
                      Current
                    </span>
                  )}
                </div>
                <div className="mt-4 flex items-baseline gap-1">
                  <span className="text-4xl font-bold text-slate-900">
                    {priceFigure}
                  </span>
                  {p.id !== "free" && (
                    <span className="text-xs font-medium text-slate-500">
                      / {interval === "monthly" ? "mo" : "yr"}
                    </span>
                  )}
                </div>
                {p.id !== "free" && (
                  <p className="mt-1 text-xs text-slate-500">
                    {altLabel} {altPrice}
                  </p>
                )}
                <hr className="my-5 border-slate-200" />
                <ul className="space-y-1.5 text-sm text-slate-700">
                  {p.features.map((f) => (
                    <li key={f} className="flex items-start gap-2">
                      <span aria-hidden className="mt-1 text-indigo-600">
                        ✓
                      </span>
                      <span>{f}</span>
                    </li>
                  ))}
                </ul>
                {!isCurrent && (p.id === "pro" || p.id === "studio") && (
                  <button
                    type="button"
                    onClick={() =>
                      setConfirmTier(p.id as "pro" | "studio")
                    }
                    className="mt-auto rounded bg-slate-900 px-3 py-2 text-sm font-medium text-white hover:bg-slate-800"
                  >
                    Choose {p.name}
                  </button>
                )}
              </div>
            );
          })}
        </div>
      </section>

      {confirmTier && (
        <ConfirmModal
          tier={confirmTier}
          interval={interval}
          plan={plans.find((p) => p.id === confirmTier)!}
          onCancel={() => setConfirmTier(null)}
          busy={actionBusy?.startsWith(`checkout-${confirmTier}`)}
          onConfirm={() => {
            void checkout(confirmTier, interval);
          }}
        />
      )}

      {/* Add-ons */}
      <section className="space-y-4">
        <div className="border-b border-slate-200 pb-3">
          <p className="text-xs font-medium uppercase tracking-wide text-slate-500">
            À la carte
          </p>
          <h2 className="mt-1 text-2xl font-semibold text-slate-900">
            Add-ons
          </h2>
          <p className="mt-2 max-w-md text-sm text-slate-600">
            Stack on top of your current plan. Cancel one without losing
            the rest.
          </p>
        </div>
        <div className="grid gap-4 md:grid-cols-2">
          <AddonCard
            title="Extra notebook"
            price="$2 / mo"
            description="Adds one notebook to your cloud-sync quota."
            note={
              currentTier === "free"
                ? "Upgrade to Pro first to use add-ons."
                : "Available on Pro and Studio."
            }
            disabled
          />
          <AddonCard
            title="+10 GB storage"
            price="$3 / mo"
            description="Adds 10 GB to your cloud asset storage."
            note={
              currentTier === "free"
                ? "Upgrade to Pro first to use add-ons."
                : "Available on Pro and Studio."
            }
            disabled
          />
        </div>
        <p className="text-xs text-slate-400">
          Add-on Stripe SKUs land in a follow-up — buttons are placeholder
          for now.
        </p>
      </section>

      {error && (
        <div className="rounded border border-rose-200 bg-rose-50 p-4 text-sm text-rose-800">
          <pre className="whitespace-pre-wrap">{error}</pre>
        </div>
      )}
      </div>
    </div>
  );
}

function IntervalToggle({
  interval,
  onChange,
}: {
  interval: "monthly" | "yearly";
  onChange: (i: "monthly" | "yearly") => void;
}) {
  return (
    <div className="inline-flex rounded-full border border-slate-300 bg-white p-0.5 text-sm">
      <button
        type="button"
        onClick={() => onChange("monthly")}
        className={`rounded-full px-3 py-1 font-medium transition-colors ${
          interval === "monthly"
            ? "bg-slate-900 text-white"
            : "text-slate-600 hover:text-slate-900"
        }`}
      >
        Monthly
      </button>
      <button
        type="button"
        onClick={() => onChange("yearly")}
        className={`flex items-center gap-1.5 rounded-full px-3 py-1 font-medium transition-colors ${
          interval === "yearly"
            ? "bg-slate-900 text-white"
            : "text-slate-600 hover:text-slate-900"
        }`}
      >
        Yearly
        <span
          className={`rounded px-1.5 py-0.5 text-[10px] font-semibold ${
            interval === "yearly"
              ? "bg-emerald-400/20 text-emerald-200"
              : "bg-emerald-100 text-emerald-700"
          }`}
        >
          2 mo free
        </span>
      </button>
    </div>
  );
}

function ConfirmModal({
  tier,
  interval,
  plan,
  onCancel,
  onConfirm,
  busy,
}: {
  tier: "pro" | "studio";
  interval: "monthly" | "yearly";
  plan: Plan;
  onCancel: () => void;
  onConfirm: () => void;
  busy?: boolean;
}) {
  const price = interval === "monthly" ? plan.priceMonthly : plan.priceYearly;
  return (
    <div
      className="fixed inset-0 z-30 flex items-center justify-center bg-slate-900/50 p-4"
      onClick={onCancel}
    >
      <div
        className="w-full max-w-md rounded-lg bg-white p-6 shadow-2xl"
        onClick={(e) => e.stopPropagation()}
      >
        <p className="text-xs font-medium uppercase tracking-wide text-slate-500">
          Confirm subscription
        </p>
        <h2 className="mt-1 text-xl font-semibold text-slate-900">
          {plan.name} ({interval})
        </h2>
        <p className="mt-2 text-sm text-slate-600">
          You're about to be redirected to Stripe to start a{" "}
          <strong>{plan.name}</strong> subscription at{" "}
          <strong>{price}</strong>. Cancel anytime via the subscription
          manager.
        </p>
        <ul className="mt-4 space-y-1 text-sm text-slate-700">
          {plan.features.map((f) => (
            <li key={f} className="flex items-start gap-2">
              <span aria-hidden className="mt-1 text-indigo-600">
                ✓
              </span>
              <span>{f}</span>
            </li>
          ))}
        </ul>
        <div className="mt-6 flex gap-2">
          <button
            type="button"
            onClick={onCancel}
            disabled={busy}
            className="flex-1 rounded border border-slate-300 px-3 py-2 text-sm font-medium text-slate-700 hover:bg-slate-100 disabled:opacity-50"
          >
            Cancel
          </button>
          <button
            type="button"
            onClick={onConfirm}
            disabled={busy}
            className="flex-1 rounded bg-slate-900 px-3 py-2 text-sm font-medium text-white hover:bg-slate-800 disabled:opacity-50"
          >
            {busy ? "Redirecting…" : `Continue to Stripe — ${price}`}
          </button>
        </div>
        <p className="mt-3 text-[11px] text-slate-400">
          Tier: {tier}. Interval: {interval}. Billing handled by Stripe.
        </p>
      </div>
    </div>
  );
}

function StatusBadge({ status }: { status: string }) {
  const cls =
    status === "active"
      ? "bg-emerald-100 text-emerald-800"
      : status === "trialing"
        ? "bg-sky-100 text-sky-800"
        : status === "past_due"
          ? "bg-amber-100 text-amber-800"
          : status === "canceled"
            ? "bg-slate-200 text-slate-700"
            : "bg-rose-100 text-rose-800";
  return (
    <span
      className={`rounded-full px-2 py-0.5 text-xs font-medium ${cls}`}
    >
      {status === "past_due" ? "past due" : status}
    </span>
  );
}

function UsageStat({
  label,
  value,
  progress,
}: {
  label: string;
  value: React.ReactNode;
  /** Optional 0..1 ratio. When set, renders a thin progress bar
   *  underneath the value coloured by how close to the cap the
   *  caller is (green / amber / red). Caps blow past 1 are clamped.
   */
  progress?: number;
}) {
  const pct =
    typeof progress === "number" ? Math.min(Math.max(progress, 0), 1) : null;
  const barColor =
    pct === null
      ? ""
      : pct >= 0.9
        ? "bg-rose-500"
        : pct >= 0.7
          ? "bg-amber-500"
          : "bg-emerald-500";
  return (
    <div>
      <dt className="text-[11px] font-medium uppercase tracking-wide text-slate-500">
        {label}
      </dt>
      <dd className="mt-0.5 text-base font-semibold text-slate-900">
        {value}
      </dd>
      {pct !== null && (
        <div className="mt-1.5 h-1 w-full overflow-hidden rounded-full bg-slate-200">
          <div
            className={`h-full ${barColor} transition-all`}
            style={{ width: `${pct * 100}%` }}
          />
        </div>
      )}
    </div>
  );
}

function AddonCard({
  title,
  price,
  description,
  note,
  disabled,
}: {
  title: string;
  price: string;
  description: string;
  note: string;
  disabled?: boolean;
}) {
  return (
    <div className="rounded-lg border border-slate-200 bg-white p-5">
      <div className="flex items-baseline justify-between gap-3">
        <h3 className="text-base font-semibold text-slate-900">{title}</h3>
        <span className="text-sm font-semibold text-slate-700">{price}</span>
      </div>
      <p className="mt-1 text-sm text-slate-600">{description}</p>
      <button
        type="button"
        disabled={disabled}
        className="mt-4 w-full rounded border border-slate-300 bg-white px-3 py-1.5 text-sm font-medium text-slate-700 hover:bg-slate-100 disabled:cursor-not-allowed disabled:opacity-50"
      >
        Add to subscription
      </button>
      <p className="mt-2 text-xs text-slate-500">{note}</p>
    </div>
  );
}
