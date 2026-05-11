import { DynamoDBClient } from '@aws-sdk/client-dynamodb';
import {
  DynamoDBDocumentClient,
  UpdateCommand,
} from '@aws-sdk/lib-dynamodb';
import { unmarshall } from '@aws-sdk/util-dynamodb';
import type { DynamoDBRecord, DynamoDBStreamEvent } from 'aws-lambda';

// The Lambda stream event AttributeValue and the SDK's are nominally
// different types but identical at runtime. Re-cast through unknown
// for the unmarshall call rather than rebuild every Image.
type StreamImage = Record<string, unknown>;

// AdminStats maintainer.
//
// Two streams feed this handler: UserEntitlement (tier / status
// transitions, MRR) and Notebook (count). Both arrive in the same
// Lambda invocation list via separate EventSourceMapping ARNs. We
// discriminate by which table the record's `eventSourceARN` points at
// — passed through to the env so we don't have to parse the ARN.

interface StreamEnv {
  ADMIN_STATS_TABLE_NAME: string;
  USER_ENTITLEMENT_TABLE_ARN: string;
  NOTEBOOK_TABLE_ARN: string;
}

const env = process.env as unknown as StreamEnv;
const ddb = DynamoDBDocumentClient.from(new DynamoDBClient({}));
const SINGLETON_ID = 'global';

// Mapping of tier name to its monthly-equivalent revenue in cents.
// Yearly subscribers contribute monthly_cents = yearly_cents / 12;
// since the webhook only knows tier (not interval directly), we
// approximate at the average rate. Good enough for dashboard.
const TIER_MONTHLY_CENTS: Record<string, number> = {
  free: 0,
  pro: 800,
  studio: 1800,
};

export const handler = async (event: DynamoDBStreamEvent): Promise<void> => {
  let userDelta: Partial<Counters> = {};
  let notebookDelta = 0;
  let mrrDelta = 0;

  for (const rec of event.Records ?? []) {
    const source = rec.eventSourceARN ?? '';
    if (source.startsWith(env.USER_ENTITLEMENT_TABLE_ARN)) {
      const d = applyUserEntitlementRecord(rec);
      mergeCounters(userDelta, d.counters);
      mrrDelta += d.mrrDelta;
    } else if (source.startsWith(env.NOTEBOOK_TABLE_ARN)) {
      notebookDelta += applyNotebookRecord(rec);
    }
  }

  if (isEmptyCounters(userDelta) && notebookDelta === 0 && mrrDelta === 0) {
    return;
  }

  const expressionParts: string[] = [];
  const values: Record<string, number | string> = {
    ':iso': new Date().toISOString(),
  };
  for (const [field, delta] of Object.entries(userDelta) as [
    keyof Counters,
    number,
  ][]) {
    if (delta === 0) continue;
    const key = `:${field}`;
    expressionParts.push(`${field} = if_not_exists(${field}, :zero) + ${key}`);
    values[key] = delta;
  }
  if (notebookDelta !== 0) {
    expressionParts.push(
      `totalNotebooks = if_not_exists(totalNotebooks, :zero) + :nb`,
    );
    values[':nb'] = notebookDelta;
  }
  if (mrrDelta !== 0) {
    expressionParts.push(
      `mrrCents = if_not_exists(mrrCents, :zero) + :mrr`,
    );
    values[':mrr'] = mrrDelta;
  }
  values[':zero'] = 0;
  expressionParts.push(`lastUpdatedIso = :iso`);

  await ddb.send(
    new UpdateCommand({
      TableName: env.ADMIN_STATS_TABLE_NAME,
      Key: { id: SINGLETON_ID },
      UpdateExpression: `SET ${expressionParts.join(', ')}`,
      ExpressionAttributeValues: values,
    }),
  );
};

interface Counters {
  totalUsers: number;
  freeUsers: number;
  proUsers: number;
  studioUsers: number;
  trialingUsers: number;
  pastDueUsers: number;
  canceledUsers: number;
}

function emptyCounters(): Counters {
  return {
    totalUsers: 0,
    freeUsers: 0,
    proUsers: 0,
    studioUsers: 0,
    trialingUsers: 0,
    pastDueUsers: 0,
    canceledUsers: 0,
  };
}

function isEmptyCounters(c: Partial<Counters>): boolean {
  return Object.values(c).every((v) => !v || v === 0);
}

function mergeCounters(into: Partial<Counters>, from: Partial<Counters>): void {
  for (const [key, val] of Object.entries(from) as [keyof Counters, number][]) {
    if (!val) continue;
    into[key] = (into[key] ?? 0) + val;
  }
}

interface EntitlementImage {
  tier?: string;
  status?: string;
}

function imageToEntitlement(
  img: StreamImage | undefined,
): EntitlementImage | undefined {
  if (!img) return undefined;
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  return unmarshall(img as any) as EntitlementImage;
}

function tierCounter(tier?: string): keyof Counters | null {
  switch (tier) {
    case 'free':
      return 'freeUsers';
    case 'pro':
      return 'proUsers';
    case 'studio':
      return 'studioUsers';
    default:
      return null;
  }
}

function statusCounter(status?: string): keyof Counters | null {
  switch (status) {
    case 'trialing':
      return 'trialingUsers';
    case 'past_due':
      return 'pastDueUsers';
    case 'canceled':
      return 'canceledUsers';
    default:
      return null;
  }
}

interface EntitlementDelta {
  counters: Partial<Counters>;
  mrrDelta: number;
}

function applyUserEntitlementRecord(rec: DynamoDBRecord): EntitlementDelta {
  const counters = emptyCounters();
  const oldImg = imageToEntitlement(rec.dynamodb?.OldImage);
  const newImg = imageToEntitlement(rec.dynamodb?.NewImage);

  if (rec.eventName === 'INSERT' && newImg) {
    counters.totalUsers += 1;
    const tk = tierCounter(newImg.tier);
    if (tk) counters[tk] += 1;
    const sk = statusCounter(newImg.status);
    if (sk) counters[sk] += 1;
    const mrr = mrrFor(newImg);
    return { counters, mrrDelta: mrr };
  }
  if (rec.eventName === 'REMOVE' && oldImg) {
    counters.totalUsers -= 1;
    const tk = tierCounter(oldImg.tier);
    if (tk) counters[tk] -= 1;
    const sk = statusCounter(oldImg.status);
    if (sk) counters[sk] -= 1;
    return { counters, mrrDelta: -mrrFor(oldImg) };
  }
  if (rec.eventName === 'MODIFY' && oldImg && newImg) {
    if (oldImg.tier !== newImg.tier) {
      const oldKey = tierCounter(oldImg.tier);
      const newKey = tierCounter(newImg.tier);
      if (oldKey) counters[oldKey] -= 1;
      if (newKey) counters[newKey] += 1;
    }
    if (oldImg.status !== newImg.status) {
      const oldKey = statusCounter(oldImg.status);
      const newKey = statusCounter(newImg.status);
      if (oldKey) counters[oldKey] -= 1;
      if (newKey) counters[newKey] += 1;
    }
    const mrrDelta = mrrFor(newImg) - mrrFor(oldImg);
    return { counters, mrrDelta };
  }
  return { counters, mrrDelta: 0 };
}

function mrrFor(img: EntitlementImage): number {
  if (img.status === 'canceled' || img.status === 'paused') return 0;
  return TIER_MONTHLY_CENTS[img.tier ?? 'free'] ?? 0;
}

function applyNotebookRecord(rec: DynamoDBRecord): number {
  if (rec.eventName === 'INSERT') return 1;
  if (rec.eventName === 'REMOVE') return -1;
  return 0;
}
