import { DynamoDBClient } from '@aws-sdk/client-dynamodb';
import {
  DynamoDBDocumentClient,
  ScanCommand,
  PutCommand,
} from '@aws-sdk/lib-dynamodb';
import {
  CognitoIdentityProviderClient,
  ListUsersCommand,
} from '@aws-sdk/client-cognito-identity-provider';

// Nightly aggregator: scan UserEntitlement to enumerate users, scan
// UserDailyUsage (14-day TTL'd → small) for stroke totals, scan
// Notebook for ownership counts, then write top-100 rankings per
// metric to AdminTopUsers. The admin dashboard reads from
// AdminTopUsers via the byMetric GSI so the dashboard call is cheap.
//
// Both ranking tables share `id = "{metric}#{rank}"` so the writes
// always overwrite the previous run in place — no separate cleanup
// pass needed. If the user count later drops below 100 the trailing
// ranks keep the previous run's data; the dashboard read can ignore
// rows whose `refreshedAtIso` is older than the latest write.
//
// Scope per the issue body: `strokes` + `notebooks`. `s3_bytes` is
// deferred (needs an upstream counter or S3 Storage Lens).

interface AggregatorEnv {
  USER_ENTITLEMENT_TABLE_NAME: string;
  USER_DAILY_USAGE_TABLE_NAME: string;
  NOTEBOOK_TABLE_NAME: string;
  ADMIN_TOP_USERS_TABLE_NAME: string;
  USER_POOL_ID: string;
}

const env = process.env as unknown as AggregatorEnv;
const ddb = DynamoDBDocumentClient.from(new DynamoDBClient({}));
const cog = new CognitoIdentityProviderClient({});
const TOP_N = 100;

interface UserEntitlementRow {
  id: string;
  tier?: string;
  status?: string;
}

interface UserDailyUsageRow {
  userId: string;
  strokeWrites?: number;
}

interface NotebookRow {
  id: string;
  owner?: string;
}

export const handler = async (): Promise<void> => {
  console.log('admin-top-users-aggregator: starting run');

  const userIds = await scanUserIds();
  console.log(`enumerated ${userIds.size} users from UserEntitlement`);

  const strokesByUser = await aggregateStrokes();
  console.log(`aggregated stroke totals for ${strokesByUser.size} users`);

  const notebooksByUser = await aggregateNotebookCounts();
  console.log(`aggregated notebook counts for ${notebooksByUser.size} users`);

  // Restrict aggregates to known UserEntitlement holders. Stroke rows
  // from a deleted-but-not-purged user shouldn't surface in the
  // dashboard.
  const strokeRanking = topN(strokesByUser, userIds, TOP_N);
  const notebookRanking = topN(notebooksByUser, userIds, TOP_N);

  // Cognito sub → email lookup. Scoped to the union of top rankings
  // so we don't ListUsers for the whole tenant.
  const subsNeedingEmail = new Set<string>([
    ...strokeRanking.map((r) => r.userId),
    ...notebookRanking.map((r) => r.userId),
  ]);
  const emailBySub = await resolveEmails(subsNeedingEmail);
  console.log(`resolved ${emailBySub.size}/${subsNeedingEmail.size} emails`);

  const refreshedAtIso = new Date().toISOString();
  await writeRanking('strokes', strokeRanking, emailBySub, refreshedAtIso);
  await writeRanking('notebooks', notebookRanking, emailBySub, refreshedAtIso);

  console.log('admin-top-users-aggregator: done');
};

async function scanUserIds(): Promise<Set<string>> {
  const ids = new Set<string>();
  let exclusiveStartKey: Record<string, unknown> | undefined;
  do {
    const out = await ddb.send(
      new ScanCommand({
        TableName: env.USER_ENTITLEMENT_TABLE_NAME,
        ProjectionExpression: 'id',
        ExclusiveStartKey: exclusiveStartKey,
      }),
    );
    for (const row of (out.Items ?? []) as UserEntitlementRow[]) {
      if (row.id) ids.add(row.id);
    }
    exclusiveStartKey = out.LastEvaluatedKey;
  } while (exclusiveStartKey);
  return ids;
}

async function aggregateStrokes(): Promise<Map<string, number>> {
  const totals = new Map<string, number>();
  let exclusiveStartKey: Record<string, unknown> | undefined;
  do {
    const out = await ddb.send(
      new ScanCommand({
        TableName: env.USER_DAILY_USAGE_TABLE_NAME,
        ProjectionExpression: 'userId, strokeWrites',
        ExclusiveStartKey: exclusiveStartKey,
      }),
    );
    for (const row of (out.Items ?? []) as UserDailyUsageRow[]) {
      if (!row.userId) continue;
      const current = totals.get(row.userId) ?? 0;
      totals.set(row.userId, current + (row.strokeWrites ?? 0));
    }
    exclusiveStartKey = out.LastEvaluatedKey;
  } while (exclusiveStartKey);
  return totals;
}

async function aggregateNotebookCounts(): Promise<Map<string, number>> {
  const counts = new Map<string, number>();
  let exclusiveStartKey: Record<string, unknown> | undefined;
  do {
    const out = await ddb.send(
      new ScanCommand({
        TableName: env.NOTEBOOK_TABLE_NAME,
        ProjectionExpression: '#o',
        ExpressionAttributeNames: { '#o': 'owner' },
        ExclusiveStartKey: exclusiveStartKey,
      }),
    );
    for (const row of (out.Items ?? []) as NotebookRow[]) {
      if (!row.owner) continue;
      counts.set(row.owner, (counts.get(row.owner) ?? 0) + 1);
    }
    exclusiveStartKey = out.LastEvaluatedKey;
  } while (exclusiveStartKey);
  return counts;
}

interface RankRow {
  rank: number;
  userId: string;
  value: number;
}

function topN(
  source: Map<string, number>,
  knownUsers: Set<string>,
  n: number,
): RankRow[] {
  const entries: Array<[string, number]> = [];
  for (const [userId, value] of source) {
    if (!knownUsers.has(userId)) continue;
    entries.push([userId, value]);
  }
  entries.sort((a, b) => b[1] - a[1]);
  return entries.slice(0, n).map(([userId, value], idx) => ({
    rank: idx + 1,
    userId,
    value,
  }));
}

async function resolveEmails(subs: Set<string>): Promise<Map<string, string>> {
  const out = new Map<string, string>();
  for (const sub of subs) {
    try {
      const resp = await cog.send(
        new ListUsersCommand({
          UserPoolId: env.USER_POOL_ID,
          Filter: `sub = "${sub}"`,
          Limit: 1,
        }),
      );
      const user = resp.Users?.[0];
      const email = user?.Attributes?.find((a) => a.Name === 'email')?.Value;
      if (email) out.set(sub, email);
    } catch (e) {
      console.warn(`email lookup failed for sub=${sub}: ${(e as Error).message}`);
    }
  }
  return out;
}

async function writeRanking(
  metric: 'strokes' | 'notebooks',
  ranking: RankRow[],
  emailBySub: Map<string, string>,
  refreshedAtIso: string,
): Promise<void> {
  for (const row of ranking) {
    await ddb.send(
      new PutCommand({
        TableName: env.ADMIN_TOP_USERS_TABLE_NAME,
        Item: {
          id: `${metric}#${row.rank}`,
          metric,
          rank: row.rank,
          userId: row.userId,
          email: emailBySub.get(row.userId) ?? null,
          value: row.value,
          refreshedAtIso,
        },
      }),
    );
  }
}
