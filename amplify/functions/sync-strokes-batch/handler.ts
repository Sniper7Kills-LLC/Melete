import {
  DynamoDBClient,
  PutItemCommand,
  ConditionalCheckFailedException,
} from '@aws-sdk/client-dynamodb';
import { marshall } from '@aws-sdk/util-dynamodb';
import {
  DynamoDBDocumentClient,
  GetCommand,
  UpdateCommand,
} from '@aws-sdk/lib-dynamodb';

// Single-path upsert against the RemoteStroke table. Every item
// (create / update / soft-delete) is a conditional PutItem. Cloud is
// the last-writer-wins record: `updatedAtIso` is the truth clock,
// `deletedAtIso` (when set) marks the row as a tombstone.
//
// We use individual PutItem with a ConditionExpression instead of
// BatchWriteItem because BatchWriteItem PutRequests are unconditional
// overwrites — concurrent worker batches racing on the same id can
// land a stale create AFTER a tombstone, resurrecting an erased
// stroke. The condition `attribute_not_exists OR existing < new`
// ensures stale writers silently lose, which is what LWW means.

const ddb = new DynamoDBClient({});
const docDdb = DynamoDBDocumentClient.from(ddb);

const TABLE_ENV = 'REMOTE_STROKE_TABLE_NAME';
const ENTITLEMENT_TABLE_ENV = 'USER_ENTITLEMENT_TABLE_NAME';
const USAGE_TABLE_ENV = 'USER_DAILY_USAGE_TABLE_NAME';

const DEFAULT_FREE_DAILY_CAP = 1000;

interface EntitlementRow {
  id: string;
  dailyWriteCap?: number;
  status?: string;
}

// Structured error body shape carried across the paywall. AppSync's
// direct-Lambda integration only ferries `errorType` (= Error.name)
// and `errorMessage` (= Error.message). To get a structured payload
// we JSON-encode the body in the message; the Rust client decodes it
// when `errorType` is one of {QuotaExceeded, SubscriptionInactive}.
// JS resolver pipeline steps use `util.error(msg, type, data, info)`
// instead — `info` lands natively in `errors[].errorInfo`.
const UPGRADE_URL =
  process.env.APP_BILLING_BASE_URL ?? 'http://localhost:3000';

interface PaywallErrorBody {
  error: string;
  code: string;
  message: string;
  limit?: number;
  current?: number;
  resetsAt?: string;
  upgradeUrl: string;
}

function throwStructured(name: string, body: PaywallErrorBody): never {
  const err = new Error(JSON.stringify(body));
  err.name = name;
  throw err;
}

class QuotaExceededError extends Error {
  code: string;
  limit: number;
  current: number;
  resetsAt: string;
  constructor(limit: number, current: number) {
    const resetsAt = new Date(
      Math.floor(Date.now() / 86400000 + 1) * 86400000,
    ).toISOString();
    const msg = `Daily write limit (${limit}) reached. Resets at ${resetsAt}.`;
    super(msg);
    this.name = 'QuotaExceeded';
    this.code = 'DAILY_WRITE_LIMIT';
    this.limit = limit;
    this.current = current;
    this.resetsAt = resetsAt;
  }
}

interface QuotaPlan {
  cap: number;
  current: number;
  acceptable: number;
}

/// Compute how many items of `bumpBy` fit under the daily-write cap.
/// Snapshot (enforce=false) accepts everything; live accepts up to
/// `cap - current`. Subscription-inactive blocks ALL writes (manual or
/// live) — past_due / canceled users get SUBSCRIPTION_INACTIVE.
async function planDailyUsage(
  sub: string,
  bumpBy: number,
  enforceCap: boolean,
): Promise<QuotaPlan> {
  const entTable = process.env[ENTITLEMENT_TABLE_ENV];
  const usageTable = process.env[USAGE_TABLE_ENV];
  if (!entTable || !usageTable) {
    throw new Error(
      `SERVER_MISCONFIGURED: missing entitlement/usage table env`,
    );
  }

  if (!enforceCap) {
    // Snapshot: no cap, no status block. Counter still bumps after
    // the writes succeed.
    return { cap: 0, current: 0, acceptable: bumpBy };
  }

  const entRow = await docDdb.send(
    new GetCommand({ TableName: entTable, Key: { id: sub } }),
  );
  const ent = entRow.Item as EntitlementRow | undefined;
  const cap = ent?.dailyWriteCap ?? DEFAULT_FREE_DAILY_CAP;
  if (
    ent &&
    ent.status &&
    ent.status !== 'active' &&
    ent.status !== 'trialing'
  ) {
    throwStructured('SubscriptionInactive', {
      error: 'SubscriptionInactive',
      code: 'SUBSCRIPTION_INACTIVE',
      message: `Subscription is ${ent.status}.`,
      upgradeUrl: UPGRADE_URL,
    });
  }

  const today = new Date().toISOString().slice(0, 10);
  const id = `${sub}#${today}`;
  const usage = await docDdb.send(
    new GetCommand({ TableName: usageTable, Key: { id } }),
  );
  const current = (usage.Item?.strokeWrites as number | undefined) ?? 0;
  const remaining = Math.max(cap - current, 0);
  const acceptable = Math.min(bumpBy, remaining);
  return { cap, current, acceptable };
}

/// Bump the daily-usage counter by `delta`. Idempotent retries are
/// caller's problem — we trust the count produced by the actual
/// PutItem success path. No condition expression: we already
/// pre-checked via `planDailyUsage`. Counter always bumps regardless
/// of snapshot vs live so the billing page sees real totals.
async function commitDailyUsage(sub: string, delta: number): Promise<void> {
  if (delta <= 0) return;
  const usageTable = process.env[USAGE_TABLE_ENV];
  if (!usageTable) {
    throw new Error(`SERVER_MISCONFIGURED: missing ${USAGE_TABLE_ENV}`);
  }
  const today = new Date().toISOString().slice(0, 10);
  const id = `${sub}#${today}`;
  const ttlEpoch = Math.floor(Date.now() / 1000) + 14 * 24 * 3600;
  const nowIso = new Date().toISOString();
  // System fields (`__typename`, `createdAt`, `updatedAt`) MUST be
  // set on first create — Amplify Gen 2's auto-generated list/get
  // resolvers filter rows missing these out, so the billing page
  // would never see the counter bump. `if_not_exists` keeps them
  // stable across subsequent increments.
  await docDdb.send(
    new UpdateCommand({
      TableName: usageTable,
      Key: { id },
      UpdateExpression:
        'SET strokeWrites = if_not_exists(strokeWrites, :zero) + :n, ' +
        'mutationCount = if_not_exists(mutationCount, :zero) + :one, ' +
        'userId = :uid, #date = :date, #owner = :uid, #ttl = :ttl, ' +
        '#typename = if_not_exists(#typename, :typename), ' +
        'createdAt = if_not_exists(createdAt, :now), ' +
        'updatedAt = :now',
      ExpressionAttributeNames: {
        '#date': 'date',
        '#owner': 'owner',
        '#ttl': 'ttl',
        '#typename': '__typename',
      },
      ExpressionAttributeValues: {
        ':zero': 0,
        ':one': 1,
        ':n': delta,
        ':uid': sub,
        ':date': today,
        ':ttl': ttlEpoch,
        ':typename': 'UserDailyUsage',
        ':now': nowIso,
      },
    }),
  );
}

interface UpsertItem {
  id: string;
  pageId: string;
  /** Stroke body (JSON string of `journal_core::Stroke`). Empty
   *  for tombstones — the row keeps its prior payload but
   *  subscribers / list filter it out via `deletedAtIso IS NOT NULL`.
   */
  strokeJson?: string;
  createdAt?: string;
  updatedAtIso: string;
  deletedAtIso?: string | null;
}

interface BatchArgs {
  notebookId: string;
  // AppSync's AWSJSON scalar arrives in the Lambda event as either
  // an already-parsed object/array (older runtimes) or as a JSON
  // string (newer). Accept both.
  items: string | UpsertItem[];
  /** "snapshot" = explicit manual save; counts toward usage but
   *  bypasses the daily-write cap rejection. "live" (default) =
   *  streaming live sync, both counted and cap-enforced. */
  kind?: string;
}

interface BatchResult {
  notebookId: string;
  upserted: number;
  unprocessed: number;
  ids: string[];
  /** Ids the worker should requeue. Stale-skipped ids are NOT here —
   *  those are correct LWW losses, requeueing would loop forever. */
  failedIds: string[];
}

export const handler = async (event: {
  arguments: BatchArgs;
  identity?: { sub?: string };
}): Promise<BatchResult> => {
  const { arguments: args, identity } = event;
  const sub = identity?.sub;
  const tableName = process.env[TABLE_ENV];
  if (!sub) throw new Error('UNAUTHENTICATED');
  if (!tableName) throw new Error(`SERVER_MISCONFIGURED: missing ${TABLE_ENV}`);

  let items: UpsertItem[] = [];
  const raw = args.items;
  if (typeof raw === 'string') {
    try {
      items = JSON.parse(raw) as UpsertItem[];
    } catch (e) {
      throw new Error(`INVALID_ITEMS_JSON: ${(e as Error).message}`);
    }
  } else if (Array.isArray(raw)) {
    items = raw;
  }
  if (!Array.isArray(items)) items = [];
  console.log(
    `[sync-strokes-batch] received ${items.length} item(s) for notebook ${args.notebookId}`,
  );

  // Dedupe by id within a single invocation — same id can show up
  // twice in a worker batch (e.g. StrokeReplaced emits a child create
  // that's later erased in the same drag tick). Keep the latest by
  // `updatedAtIso` (LWW) so we only fire one PutItem per id.
  const byId = new Map<string, UpsertItem>();
  for (const it of items) {
    if (!it?.id) continue;
    const existing = byId.get(it.id);
    if (
      !existing ||
      (it.updatedAtIso ?? '') >= (existing.updatedAtIso ?? '')
    ) {
      byId.set(it.id, it);
    }
  }
  const dedupedItems = [...byId.values()];
  if (dedupedItems.length !== items.length) {
    console.log(
      `[sync-strokes-batch] deduped ${items.length} → ${dedupedItems.length}`,
    );
  }
  items = dedupedItems;

  // Soft cap. Live batches are trimmed at the cap — anything beyond
  // gets reported as `unprocessed` so the desktop's worker requeues
  // it for the next quota window. Snapshot batches always accept
  // every item (cap only applies to recurring live cost). Counter
  // bumps after the writes succeed, by the actual processed count.
  const isSnapshot = args.kind === 'snapshot';
  let quotaRejected: UpsertItem[] = [];
  if (items.length > 0) {
    const plan = await planDailyUsage(sub, items.length, /* enforceCap = */ !isSnapshot);
    if (plan.acceptable < items.length) {
      quotaRejected = items.slice(plan.acceptable);
      items = items.slice(0, plan.acceptable);
      console.log(
        `[sync-strokes-batch] soft-cap trimmed ${quotaRejected.length} item(s) past daily-write cap (cap=${plan.cap} current=${plan.current})`,
      );
    }
  }

  const now = new Date().toISOString();
  const ids: string[] = [];
  const failedIds: string[] = [];
  let upserted = 0;
  let staleSkipped = 0;
  let failed = 0;

  // Fire all PutItem calls in parallel — small fan-out (≤25 typical),
  // each conditional, each independent. Total wall time is one DDB
  // round-trip plus retries on the slowest item.
  const results = await Promise.allSettled(
    items.map(async (it) => {
      if (!it?.id || !it?.pageId) return { kind: 'skip-bad' as const };
      const item: Record<string, unknown> = {
        id: it.id,
        notebookId: args.notebookId,
        pageId: it.pageId,
        strokeJson: it.strokeJson ?? '',
        createdAt: it.createdAt ?? now,
        updatedAtIso: it.updatedAtIso,
        owner: sub,
        __typename: 'RemoteStroke',
        updatedAt: now,
      };
      if (it.deletedAtIso) {
        item.deletedAtIso = it.deletedAtIso;
      }
      try {
        await ddb.send(
          new PutItemCommand({
            TableName: tableName,
            Item: marshall(item, { removeUndefinedValues: true }),
            ConditionExpression:
              'attribute_not_exists(updatedAtIso) OR updatedAtIso < :new',
            ExpressionAttributeValues: marshall({
              ':new': it.updatedAtIso,
            }),
          }),
        );
        ids.push(it.id);
        return { kind: 'ok' as const };
      } catch (e) {
        if (e instanceof ConditionalCheckFailedException) {
          // Stale writer — newer record already in DDB. Expected under
          // LWW, not an error.
          return { kind: 'stale' as const, id: it.id };
        }
        console.error(
          `[sync-strokes-batch] PutItem failed for ${it.id}: ${(e as Error).message}`,
        );
        return { kind: 'fail' as const, id: it.id };
      }
    }),
  );

  for (const r of results) {
    if (r.status !== 'fulfilled') {
      failed += 1;
      continue;
    }
    switch (r.value.kind) {
      case 'ok':
        upserted += 1;
        break;
      case 'stale':
        staleSkipped += 1;
        break;
      case 'fail':
        failed += 1;
        failedIds.push(r.value.id);
        break;
    }
  }

  // Counter bumps by what actually landed in DDB, not what the caller
  // sent. Soft-cap trimmed items are reported separately via
  // `unprocessed`/`failedIds` so the client can requeue them.
  if (upserted > 0) {
    try {
      await commitDailyUsage(sub, upserted);
    } catch (e) {
      console.warn(
        `[sync-strokes-batch] commitDailyUsage failed (writes already landed): ${(e as Error).message}`,
      );
    }
  }

  for (const it of quotaRejected) {
    failedIds.push(it.id);
  }

  const result = {
    notebookId: args.notebookId,
    upserted,
    unprocessed: failed + quotaRejected.length,
    ids,
    failedIds,
  };
  console.log(
    `[sync-strokes-batch] result: upserted=${upserted} stale_skipped=${staleSkipped} failed=${failed} quota_rejected=${quotaRejected.length} ids=${ids.length}`,
  );
  return result;
};
