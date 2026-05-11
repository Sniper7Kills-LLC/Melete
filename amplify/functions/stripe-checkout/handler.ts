import Stripe from 'stripe';
import { DynamoDBClient } from '@aws-sdk/client-dynamodb';
import { DynamoDBDocumentClient, GetCommand } from '@aws-sdk/lib-dynamodb';
import type { Schema } from '../../data/resource';

interface CheckoutEnv {
  STRIPE_SECRET_KEY: string;
  TIER_CONFIG_TABLE_NAME: string;
  USER_ENTITLEMENT_TABLE_NAME: string;
  APP_BILLING_BASE_URL: string;
}

const env = process.env as unknown as CheckoutEnv;
const stripe = new Stripe(env.STRIPE_SECRET_KEY, {
  apiVersion: '2025-02-24.acacia',
});
const ddb = DynamoDBDocumentClient.from(new DynamoDBClient({}));

interface TierConfigRow {
  id: string;
  stripePriceIdMonthly?: string;
  stripePriceIdYearly?: string;
}

interface EntitlementRow {
  id: string;
  stripeCustomerId?: string;
}

export const handler: Schema['createCheckoutSession']['functionHandler'] =
  async (event) => {
    const sub = event.identity && 'sub' in event.identity
      ? event.identity.sub
      : undefined;
    const email = event.identity && 'claims' in event.identity
      ? (event.identity.claims as Record<string, string>).email
      : undefined;
    if (!sub) throw new Error('UNAUTHENTICATED');

    const { tier, interval } = event.arguments;
    if (tier !== 'pro' && tier !== 'studio') {
      throw new Error(`INVALID_TIER: ${tier}`);
    }
    if (interval !== 'monthly' && interval !== 'yearly') {
      throw new Error(`INVALID_INTERVAL: ${interval}`);
    }

    const tierRow = await ddb.send(
      new GetCommand({
        TableName: env.TIER_CONFIG_TABLE_NAME,
        Key: { id: tier },
      }),
    );
    const config = tierRow.Item as TierConfigRow | undefined;
    if (!config) throw new Error(`TIER_CONFIG_MISSING: ${tier}`);

    const priceId =
      interval === 'monthly'
        ? config.stripePriceIdMonthly
        : config.stripePriceIdYearly;
    if (!priceId) {
      throw new Error(`PRICE_ID_MISSING: ${tier}/${interval}`);
    }

    // Reuse an existing Stripe customer if the user has one (from a
    // previous subscription). Avoids the orphan-customer pile-up
    // Stripe is famous for when you don't pass `customer` to Checkout.
    const existing = await ddb.send(
      new GetCommand({
        TableName: env.USER_ENTITLEMENT_TABLE_NAME,
        Key: { id: sub },
      }),
    );
    const existingCustomerId = (existing.Item as EntitlementRow | undefined)
      ?.stripeCustomerId;

    const session = await stripe.checkout.sessions.create({
      mode: 'subscription',
      line_items: [{ price: priceId, quantity: 1 }],
      success_url: `${env.APP_BILLING_BASE_URL}/billing/success?session_id={CHECKOUT_SESSION_ID}`,
      cancel_url: `${env.APP_BILLING_BASE_URL}/billing/cancel`,
      ...(existingCustomerId
        ? { customer: existingCustomerId }
        : email
          ? { customer_email: email, customer_creation: 'always' }
          : { customer_creation: 'always' }),
      client_reference_id: sub,
      subscription_data: {
        metadata: { cognitoSub: sub },
      },
      metadata: { cognitoSub: sub },
      allow_promotion_codes: true,
    });

    if (!session.url) {
      throw new Error('CHECKOUT_URL_MISSING');
    }
    return { url: session.url };
  };
