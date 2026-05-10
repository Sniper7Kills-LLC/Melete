import {
  DynamoDBClient,
  PutItemCommand,
  ConditionalCheckFailedException,
} from '@aws-sdk/client-dynamodb';
import { marshall } from '@aws-sdk/util-dynamodb';

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

const TABLE_ENV = 'REMOTE_STROKE_TABLE_NAME';

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

  const result = {
    notebookId: args.notebookId,
    upserted,
    unprocessed: failed,
    ids,
    failedIds,
  };
  console.log(
    `[sync-strokes-batch] result: upserted=${upserted} stale_skipped=${staleSkipped} failed=${failed} ids=${ids.length}`,
  );
  return result;
};
