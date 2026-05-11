// Shared usage fetcher. Pulls today's UserDailyUsage row + the
// caller's notebook count off the data API, exposed via a small hook
// so both the Billing page and the AccountChip dropdown render the
// same numbers without re-implementing the queries.
//
// Refresh strategy: load on mount, refetch on a 30s interval while
// the consumer is mounted. Cheap to call (1-2 DDB reads).

import { useEffect, useState } from "react";
import { client, type UserDailyUsageRow } from "./amplify-client";

export interface Usage {
  notebooksUsed: number;
  strokesToday: number;
}

export function useUsage(sub: string | undefined): {
  usage: Usage | null;
  loading: boolean;
  refetch: () => void;
} {
  const [usage, setUsage] = useState<Usage | null>(null);
  const [loading, setLoading] = useState(true);
  const [tick, setTick] = useState(0);

  useEffect(() => {
    let alive = true;
    if (!sub) {
      setLoading(false);
      return;
    }
    async function load() {
      try {
        const today = new Date().toISOString().slice(0, 10);
        const [usageRow, notebooks] = await Promise.all([
          client.models.UserDailyUsage.get({
            id: `${sub}#${today}`,
          }) as Promise<{ data: UserDailyUsageRow | null }>,
          client.models.Notebook.listNotebooksByOwner({ owner: sub! }) as Promise<{
            data: unknown[];
          }>,
        ]);
        if (!alive) return;
        setUsage({
          notebooksUsed: notebooks.data?.filter(Boolean).length ?? 0,
          strokesToday: usageRow.data?.strokeWrites ?? 0,
        });
      } catch {
        if (!alive) return;
        // Soft-fail: render whatever was last seen; banner caller can
        // decide whether to nag.
        setUsage((u) => u ?? { notebooksUsed: 0, strokesToday: 0 });
      } finally {
        if (alive) setLoading(false);
      }
    }
    void load();
    return () => {
      alive = false;
    };
  }, [sub, tick]);

  // Auto-refresh every 30s. Stops when consumer unmounts via the
  // effect cleanup above.
  useEffect(() => {
    const id = window.setInterval(() => setTick((t) => t + 1), 30_000);
    return () => window.clearInterval(id);
  }, []);

  return { usage, loading, refetch: () => setTick((t) => t + 1) };
}
