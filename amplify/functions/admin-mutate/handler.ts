import { DynamoDBClient } from '@aws-sdk/client-dynamodb';
import {
  DynamoDBDocumentClient,
  GetCommand,
  UpdateCommand,
  PutCommand,
  DeleteCommand,
} from '@aws-sdk/lib-dynamodb';
import {
  CognitoIdentityProviderClient,
  AdminDisableUserCommand,
  AdminDeleteUserCommand,
} from '@aws-sdk/client-cognito-identity-provider';
import Stripe from 'stripe';
import type { Schema } from '../../data/resource';

interface Env {
  USER_ENTITLEMENT_TABLE_NAME: string;
  USER_DAILY_USAGE_TABLE_NAME: string;
  ADMIN_AUDIT_LOG_TABLE_NAME: string;
  TIER_CONFIG_TABLE_NAME: string;
  USER_POOL_ID: string;
  STRIPE_SECRET_KEY: string;
}

const env = process.env as unknown as Env;
const ddb = DynamoDBDocumentClient.from(new DynamoDBClient({}));
const cognito = new CognitoIdentityProviderClient({});
const stripe = new Stripe(env.STRIPE_SECRET_KEY, {
  apiVersion: '2025-02-24.acacia',
});

interface AdminMutateArgs {
  action: string;
  targetUserId: string;
  payload?: string;
  reason: string;
}

type EntitlementRow = Record<string, unknown> & {
  id?: string;
  tier?: string;
  status?: string;
  stripeSubscriptionId?: string;
  educationVerified?: boolean;
  capOverridesJson?: string;
};

export const handler: Schema['adminMutate']['functionHandler'] = async (
  event,
) => {
  const groups =
    event.identity && 'groups' in event.identity
      ? ((event.identity.groups ?? []) as string[])
      : [];
  if (!groups.includes('superadmin')) {
    throw new Error('FORBIDDEN');
  }
  const adminUserId =
    event.identity && 'sub' in event.identity ? event.identity.sub : '';
  const adminEmail =
    event.identity && 'claims' in event.identity
      ? ((event.identity.claims as Record<string, string>).email ?? '')
      : '';

  const { action, targetUserId, payload, reason } =
    event.arguments as AdminMutateArgs;
  if (!reason || reason.trim().length === 0) {
    throw new Error('REASON_REQUIRED');
  }

  const before = await loadEntitlement(targetUserId);
  let after: EntitlementRow | null = before;

  switch (action) {
    case 'grantTier':
      after = await actionGrantTier(targetUserId, parsePayload(payload), adminUserId);
      break;
    case 'setStatus':
      after = await actionSetStatus(targetUserId, parsePayload(payload));
      break;
    case 'markEducation':
      after = await actionMarkEducation(targetUserId, parsePayload(payload));
      break;
    case 'resetDailyUsage':
      await actionResetDailyUsage(targetUserId);
      break;
    case 'setEntitlementCaps':
      after = await actionSetEntitlementCaps(targetUserId, parsePayload(payload));
      break;
    case 'disableUser':
      await cognito.send(
        new AdminDisableUserCommand({
          UserPoolId: env.USER_POOL_ID,
          Username: targetUserId,
        }),
      );
      after = await actionSetStatus(targetUserId, { status: 'paused' });
      break;
    case 'deleteUser':
      // Cognito delete first — once the user is gone, the row can be
      // dropped without leaving orphan auth state.
      await cognito.send(
        new AdminDeleteUserCommand({
          UserPoolId: env.USER_POOL_ID,
          Username: targetUserId,
        }),
      );
      await ddb.send(
        new DeleteCommand({
          TableName: env.USER_ENTITLEMENT_TABLE_NAME,
          Key: { id: targetUserId },
        }),
      );
      after = null;
      break;
    case 'extendTrial': {
      const subId = (before ?? {}).stripeSubscriptionId;
      const { days } = parsePayload(payload) as { days?: number };
      if (!subId) throw new Error('NO_STRIPE_SUBSCRIPTION');
      if (!days || days <= 0) throw new Error('INVALID_DAYS');
      const sub = await stripe.subscriptions.retrieve(subId);
      const currentTrialEnd =
        sub.trial_end ?? Math.floor(Date.now() / 1000);
      await stripe.subscriptions.update(subId, {
        trial_end: currentTrialEnd + days * 86400,
        proration_behavior: 'none',
      });
      // Trial extension lands back via the Stripe webhook; we don't
      // mirror it locally here to avoid drift.
      break;
    }
    default:
      throw new Error(`UNKNOWN_ACTION: ${action}`);
  }

  await writeAuditLog({
    adminUserId,
    adminEmail,
    action,
    targetUserId,
    reason,
    before,
    after,
  });

  return { after: after ? JSON.stringify(after) : null };
};

function parsePayload(raw: string | undefined): Record<string, unknown> {
  if (!raw) return {};
  try {
    return JSON.parse(raw) as Record<string, unknown>;
  } catch {
    throw new Error('INVALID_PAYLOAD_JSON');
  }
}

async function loadEntitlement(
  userId: string,
): Promise<EntitlementRow | null> {
  const r = await ddb.send(
    new GetCommand({
      TableName: env.USER_ENTITLEMENT_TABLE_NAME,
      Key: { id: userId },
    }),
  );
  return (r.Item as EntitlementRow | undefined) ?? null;
}

async function loadTierConfig(tier: string): Promise<Record<string, unknown>> {
  const r = await ddb.send(
    new GetCommand({
      TableName: env.TIER_CONFIG_TABLE_NAME,
      Key: { id: tier },
    }),
  );
  if (!r.Item) throw new Error(`TIER_CONFIG_MISSING: ${tier}`);
  return r.Item as Record<string, unknown>;
}

async function actionGrantTier(
  userId: string,
  payload: Record<string, unknown>,
  adminUserId: string,
): Promise<EntitlementRow> {
  const tier = String(payload.tier ?? '');
  if (!['free', 'pro', 'studio'].includes(tier)) {
    throw new Error(`INVALID_TIER: ${tier}`);
  }
  const config = await loadTierConfig(tier);
  const nowIso = new Date().toISOString();
  await ddb.send(
    new UpdateCommand({
      TableName: env.USER_ENTITLEMENT_TABLE_NAME,
      Key: { id: userId },
      UpdateExpression: `SET
        #owner = :owner,
        tier = :tier,
        #status = :status,
        notebookCap = :nb,
        strokesPerPageCap = :spp,
        strokesPerNotebookCap = :spn,
        dailyWriteCap = :dwc,
        s3BytesCap = :s3,
        templatePublishCap = :tpc,
        historyDays = :hd,
        liveSyncEnabled = :lse,
        compedBy = :admin,
        updatedAtSort = :u`,
      ExpressionAttributeNames: {
        '#owner': 'owner',
        '#status': 'status',
      },
      ExpressionAttributeValues: {
        ':owner': userId,
        ':tier': tier,
        ':status': 'active',
        ':nb': config.notebookCap,
        ':spp': config.strokesPerPageCap,
        ':spn': config.strokesPerNotebookCap,
        ':dwc': config.dailyWriteCap,
        ':s3': config.s3BytesCap,
        ':tpc': config.templatePublishCap,
        ':hd': config.historyDays ?? 0,
        ':lse': config.liveSyncEnabled ?? false,
        ':admin': adminUserId,
        ':u': nowIso,
      },
    }),
  );
  return (await loadEntitlement(userId))!;
}

async function actionSetStatus(
  userId: string,
  payload: Record<string, unknown>,
): Promise<EntitlementRow> {
  const status = String(payload.status ?? '');
  if (
    !['active', 'trialing', 'past_due', 'canceled', 'paused'].includes(status)
  ) {
    throw new Error(`INVALID_STATUS: ${status}`);
  }
  await ddb.send(
    new UpdateCommand({
      TableName: env.USER_ENTITLEMENT_TABLE_NAME,
      Key: { id: userId },
      UpdateExpression: 'SET #status = :s, updatedAtSort = :u',
      ExpressionAttributeNames: { '#status': 'status' },
      ExpressionAttributeValues: {
        ':s': status,
        ':u': new Date().toISOString(),
      },
    }),
  );
  return (await loadEntitlement(userId))!;
}

async function actionMarkEducation(
  userId: string,
  payload: Record<string, unknown>,
): Promise<EntitlementRow> {
  const verified = Boolean(payload.verified);
  await ddb.send(
    new UpdateCommand({
      TableName: env.USER_ENTITLEMENT_TABLE_NAME,
      Key: { id: userId },
      UpdateExpression: 'SET educationVerified = :v, updatedAtSort = :u',
      ExpressionAttributeValues: {
        ':v': verified,
        ':u': new Date().toISOString(),
      },
    }),
  );
  return (await loadEntitlement(userId))!;
}

async function actionResetDailyUsage(userId: string): Promise<void> {
  const today = new Date().toISOString().slice(0, 10);
  await ddb.send(
    new DeleteCommand({
      TableName: env.USER_DAILY_USAGE_TABLE_NAME,
      Key: { id: `${userId}#${today}` },
    }),
  );
}

async function actionSetEntitlementCaps(
  userId: string,
  payload: Record<string, unknown>,
): Promise<EntitlementRow> {
  // Stores the cap overrides verbatim on the row. The
  // EntitlementService merges these into the effective caps at read
  // time. Resolved cap fields are also written so older clients see
  // the override without needing the merge path.
  const overrides = JSON.stringify(payload);
  const set: string[] = [
    'capOverridesJson = :overrides',
    'updatedAtSort = :u',
  ];
  const values: Record<string, unknown> = {
    ':overrides': overrides,
    ':u': new Date().toISOString(),
  };
  const fields: Array<[string, string]> = [
    ['notebookCap', 'notebookCap'],
    ['strokesPerPageCap', 'strokesPerPageCap'],
    ['strokesPerNotebookCap', 'strokesPerNotebookCap'],
    ['dailyWriteCap', 'dailyWriteCap'],
    ['s3BytesCap', 's3BytesCap'],
    ['templatePublishCap', 'templatePublishCap'],
    ['historyDays', 'historyDays'],
  ];
  for (const [payloadKey, dbField] of fields) {
    if (payload[payloadKey] !== undefined) {
      set.push(`${dbField} = :${dbField}`);
      values[`:${dbField}`] = payload[payloadKey];
    }
  }
  await ddb.send(
    new UpdateCommand({
      TableName: env.USER_ENTITLEMENT_TABLE_NAME,
      Key: { id: userId },
      UpdateExpression: `SET ${set.join(', ')}`,
      ExpressionAttributeValues: values,
    }),
  );
  return (await loadEntitlement(userId))!;
}

interface AuditEntry {
  adminUserId: string;
  adminEmail: string;
  action: string;
  targetUserId: string;
  reason: string;
  before: EntitlementRow | null;
  after: EntitlementRow | null;
}

async function writeAuditLog(e: AuditEntry): Promise<void> {
  const nowIso = new Date().toISOString();
  const yearMonth = nowIso.slice(0, 7);
  const id = `${yearMonth}#${nowIso}#${Math.random().toString(36).slice(2, 10)}`;
  await ddb.send(
    new PutCommand({
      TableName: env.ADMIN_AUDIT_LOG_TABLE_NAME,
      Item: {
        id,
        yearMonth,
        timestampIso: nowIso,
        adminUserId: e.adminUserId,
        adminEmail: e.adminEmail,
        action: e.action,
        targetUserId: e.targetUserId,
        beforeJson: e.before ? JSON.stringify(e.before) : null,
        afterJson: e.after ? JSON.stringify(e.after) : null,
        reason: e.reason,
      },
    }),
  );
}
