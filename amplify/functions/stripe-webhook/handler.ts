import Stripe from 'stripe';
import { DynamoDBClient } from '@aws-sdk/client-dynamodb';
import {
  DynamoDBDocumentClient,
  GetCommand,
  ScanCommand,
  UpdateCommand,
} from '@aws-sdk/lib-dynamodb';

// Stripe webhook handler.
//
// Receives subscription lifecycle events and projects them onto the
// `UserEntitlement` DDB row keyed by Cognito sub. The link between a
// Stripe customer and a Cognito user is carried in subscription
// metadata (`cognitoSub`) set when the Checkout session was created;
// without it, the event is acknowledged but ignored.
//
// Caps written by this handler are the tier defaults from `TierConfig`.
// Admin-set fields (educationVerified, capOverridesJson, compedBy,
// addonsJson) are preserved across webhook updates via DDB
// `if_not_exists` on first-write and explicit non-write on subsequent
// updates — admin overrides survive renewals. Resolving addons +
// overrides into effective caps is deferred to the EntitlementService
// (step 7) so the webhook stays simple.

interface StripeEventEnv {
  STRIPE_SECRET_KEY: string;
  STRIPE_WEBHOOK_SECRET: string;
  USER_ENTITLEMENT_TABLE_NAME: string;
  TIER_CONFIG_TABLE_NAME: string;
}

const env = process.env as unknown as StripeEventEnv;
const stripe = new Stripe(env.STRIPE_SECRET_KEY, {
  apiVersion: '2025-02-24.acacia',
});
const ddb = DynamoDBDocumentClient.from(new DynamoDBClient({}));

interface LambdaUrlEvent {
  body: string | null;
  isBase64Encoded?: boolean;
  headers?: Record<string, string | undefined>;
}

interface LambdaUrlResponse {
  statusCode: number;
  body: string;
  headers?: Record<string, string>;
}

export const handler = async (
  event: LambdaUrlEvent,
): Promise<LambdaUrlResponse> => {
  const sig =
    event.headers?.['stripe-signature'] ?? event.headers?.['Stripe-Signature'];
  if (!sig) return reply(400, 'missing signature');
  if (!event.body) return reply(400, 'missing body');

  const rawBody = event.isBase64Encoded
    ? Buffer.from(event.body, 'base64').toString('utf8')
    : event.body;

  let stripeEvent: Stripe.Event;
  try {
    stripeEvent = stripe.webhooks.constructEvent(
      rawBody,
      sig,
      env.STRIPE_WEBHOOK_SECRET,
    );
  } catch (err) {
    console.error('[stripe-webhook] signature verify failed', err);
    return reply(400, 'invalid signature');
  }

  console.log(`[stripe-webhook] ${stripeEvent.type} ${stripeEvent.id}`);

  try {
    switch (stripeEvent.type) {
      case 'customer.subscription.created':
      case 'customer.subscription.updated':
      case 'customer.subscription.trial_will_end':
        await handleSubscriptionUpsert(
          stripeEvent.data.object as Stripe.Subscription,
        );
        break;
      case 'customer.subscription.deleted':
        await handleSubscriptionCanceled(
          stripeEvent.data.object as Stripe.Subscription,
        );
        break;
      case 'invoice.paid':
        await handleInvoicePaid(stripeEvent.data.object as Stripe.Invoice);
        break;
      case 'invoice.payment_failed':
        await handleInvoiceFailed(stripeEvent.data.object as Stripe.Invoice);
        break;
      default:
        // Acknowledge unhandled events so Stripe doesn't retry.
        break;
    }
    return reply(200, 'ok');
  } catch (err) {
    console.error('[stripe-webhook] handler error', err);
    return reply(500, 'handler error');
  }
};

async function handleSubscriptionUpsert(
  sub: Stripe.Subscription,
): Promise<void> {
  const cognitoSub = sub.metadata?.cognitoSub;
  if (!cognitoSub) {
    console.warn(
      `[stripe-webhook] subscription ${sub.id} missing cognitoSub metadata; skipping`,
    );
    return;
  }

  const priceId = sub.items.data[0]?.price.id;
  if (!priceId) {
    console.warn(`[stripe-webhook] subscription ${sub.id} has no items`);
    return;
  }

  const tier = await resolveTierFromPriceId(priceId);
  if (!tier) {
    console.warn(
      `[stripe-webhook] no TierConfig matches priceId ${priceId}; subscription ${sub.id}`,
    );
    return;
  }

  const tierConfig = await loadTierConfig(tier);
  await writeEntitlement(cognitoSub, sub, tier, tierConfig, sub.status);
}

async function handleSubscriptionCanceled(
  sub: Stripe.Subscription,
): Promise<void> {
  const cognitoSub = sub.metadata?.cognitoSub;
  if (!cognitoSub) return;

  const freeConfig = await loadTierConfig('free');
  await writeEntitlement(cognitoSub, sub, 'free', freeConfig, 'canceled');
}

async function handleInvoicePaid(inv: Stripe.Invoice): Promise<void> {
  const subId = typeof inv.subscription === 'string' ? inv.subscription : null;
  if (!subId) return;

  const sub = await stripe.subscriptions.retrieve(subId);
  const cognitoSub = sub.metadata?.cognitoSub;
  if (!cognitoSub) return;

  await ddb.send(
    new UpdateCommand({
      TableName: env.USER_ENTITLEMENT_TABLE_NAME,
      Key: { id: cognitoSub },
      UpdateExpression:
        'SET #status = :active, periodEnd = :pe, updatedAtSort = :u',
      ExpressionAttributeNames: { '#status': 'status' },
      ExpressionAttributeValues: {
        ':active': 'active',
        ':pe': new Date(sub.current_period_end * 1000).toISOString(),
        ':u': new Date().toISOString(),
      },
    }),
  );
}

async function handleInvoiceFailed(inv: Stripe.Invoice): Promise<void> {
  const subId = typeof inv.subscription === 'string' ? inv.subscription : null;
  if (!subId) return;

  const sub = await stripe.subscriptions.retrieve(subId);
  const cognitoSub = sub.metadata?.cognitoSub;
  if (!cognitoSub) return;

  await ddb.send(
    new UpdateCommand({
      TableName: env.USER_ENTITLEMENT_TABLE_NAME,
      Key: { id: cognitoSub },
      UpdateExpression: 'SET #status = :pd, updatedAtSort = :u',
      ExpressionAttributeNames: { '#status': 'status' },
      ExpressionAttributeValues: {
        ':pd': 'past_due',
        ':u': new Date().toISOString(),
      },
    }),
  );
}

// Upsert the entitlement row with system-managed fields. Admin fields
// (educationVerified, capOverridesJson, compedBy, addonsJson) are kept
// via `if_not_exists` on first create and untouched on update — once an
// admin patches them, this handler will not overwrite.
async function writeEntitlement(
  cognitoSub: string,
  sub: Stripe.Subscription,
  tier: string,
  tierConfig: TierConfigRow,
  status: string,
): Promise<void> {
  const customerId =
    typeof sub.customer === 'string' ? sub.customer : sub.customer.id;
  const nowIso = new Date().toISOString();
  const periodEnd = new Date(sub.current_period_end * 1000).toISOString();
  const trialEndsAt = sub.trial_end
    ? new Date(sub.trial_end * 1000).toISOString()
    : null;

  await ddb.send(
    new UpdateCommand({
      TableName: env.USER_ENTITLEMENT_TABLE_NAME,
      Key: { id: cognitoSub },
      UpdateExpression: `SET
        #owner = :owner,
        tier = :tier,
        #status = :status,
        stripeCustomerId = :cid,
        stripeSubscriptionId = :sid,
        periodEnd = :pe,
        trialEndsAt = :te,
        notebookCap = :nbCap,
        strokesPerPageCap = :spp,
        strokesPerNotebookCap = :spn,
        dailyWriteCap = :dwc,
        s3BytesCap = :s3,
        templatePublishCap = :tpc,
        historyDays = :hd,
        liveSyncEnabled = :lse,
        updatedAtSort = :u,
        educationVerified = if_not_exists(educationVerified, :false),
        addonsJson = if_not_exists(addonsJson, :nullVal),
        capOverridesJson = if_not_exists(capOverridesJson, :nullVal),
        compedBy = if_not_exists(compedBy, :nullVal)`,
      ExpressionAttributeNames: {
        '#owner': 'owner',
        '#status': 'status',
      },
      ExpressionAttributeValues: {
        ':owner': cognitoSub,
        ':tier': tier,
        ':status': status,
        ':cid': customerId,
        ':sid': sub.id,
        ':pe': periodEnd,
        ':te': trialEndsAt,
        ':nbCap': tierConfig.notebookCap,
        ':spp': tierConfig.strokesPerPageCap,
        ':spn': tierConfig.strokesPerNotebookCap,
        ':dwc': tierConfig.dailyWriteCap,
        ':s3': tierConfig.s3BytesCap,
        ':tpc': tierConfig.templatePublishCap,
        ':hd': tierConfig.historyDays ?? 0,
        ':lse': tierConfig.liveSyncEnabled ?? false,
        ':u': nowIso,
        ':false': false,
        ':nullVal': null,
      },
    }),
  );
}

interface TierConfigRow {
  id: string;
  notebookCap: number;
  strokesPerPageCap: number;
  strokesPerNotebookCap: number;
  dailyWriteCap: number;
  s3BytesCap: number;
  templatePublishCap: number;
  historyDays?: number;
  liveSyncEnabled?: boolean;
  stripePriceIdMonthly?: string;
  stripePriceIdYearly?: string;
}

// Small in-memory cache so repeated webhook deliveries don't Scan
// TierConfig every time. Lambda warm starts reuse it; cold starts
// repopulate on demand. ~3 tier rows fit comfortably.
const tierConfigCache = new Map<string, TierConfigRow>();
const priceIdToTierCache = new Map<string, string>();

async function resolveTierFromPriceId(
  priceId: string,
): Promise<string | null> {
  const cached = priceIdToTierCache.get(priceId);
  if (cached) return cached;

  const result = await ddb.send(
    new ScanCommand({
      TableName: env.TIER_CONFIG_TABLE_NAME,
      FilterExpression:
        'stripePriceIdMonthly = :p OR stripePriceIdYearly = :p',
      ExpressionAttributeValues: { ':p': priceId },
    }),
  );

  const tierId = (result.Items?.[0] as TierConfigRow | undefined)?.id ?? null;
  if (tierId) priceIdToTierCache.set(priceId, tierId);
  return tierId;
}

async function loadTierConfig(tier: string): Promise<TierConfigRow> {
  const cached = tierConfigCache.get(tier);
  if (cached) return cached;

  const result = await ddb.send(
    new GetCommand({
      TableName: env.TIER_CONFIG_TABLE_NAME,
      Key: { id: tier },
    }),
  );
  const row = result.Item as TierConfigRow | undefined;
  if (!row) throw new Error(`TierConfig missing for ${tier}`);
  tierConfigCache.set(tier, row);
  return row;
}

function reply(statusCode: number, body: string): LambdaUrlResponse {
  return {
    statusCode,
    body,
    headers: { 'Content-Type': 'text/plain' },
  };
}
