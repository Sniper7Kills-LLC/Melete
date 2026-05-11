import Stripe from 'stripe';
import { DynamoDBClient } from '@aws-sdk/client-dynamodb';
import { DynamoDBDocumentClient, GetCommand } from '@aws-sdk/lib-dynamodb';
import type { Schema } from '../../data/resource';

interface PortalEnv {
  STRIPE_SECRET_KEY: string;
  USER_ENTITLEMENT_TABLE_NAME: string;
  APP_BILLING_BASE_URL: string;
}

const env = process.env as unknown as PortalEnv;
const stripe = new Stripe(env.STRIPE_SECRET_KEY, {
  apiVersion: '2025-02-24.acacia',
});
const ddb = DynamoDBDocumentClient.from(new DynamoDBClient({}));

interface EntitlementRow {
  id: string;
  stripeCustomerId?: string;
}

export const handler: Schema['createPortalSession']['functionHandler'] =
  async (event) => {
    const sub = event.identity && 'sub' in event.identity
      ? event.identity.sub
      : undefined;
    if (!sub) throw new Error('UNAUTHENTICATED');

    const row = await ddb.send(
      new GetCommand({
        TableName: env.USER_ENTITLEMENT_TABLE_NAME,
        Key: { id: sub },
      }),
    );
    const customerId = (row.Item as EntitlementRow | undefined)
      ?.stripeCustomerId;
    if (!customerId) {
      // No customer = never subscribed. Caller should route to
      // Checkout instead. Returning a typed error so the desktop
      // can distinguish from a generic failure.
      throw new Error('NO_STRIPE_CUSTOMER');
    }

    const session = await stripe.billingPortal.sessions.create({
      customer: customerId,
      return_url: `${env.APP_BILLING_BASE_URL}/billing`,
    });

    return { url: session.url };
  };
