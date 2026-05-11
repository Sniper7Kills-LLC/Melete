import { defineFunction, secret } from '@aws-amplify/backend';

// Stripe webhook receiver. Public Function URL (auth=NONE) — Stripe
// authenticates each delivery via `Stripe-Signature` header verified
// against `STRIPE_WEBHOOK_SECRET`. Lambda writes the resolved entitlement
// row directly to the UserEntitlement table; tier mapping comes from
// the TierConfig table (price IDs → tier name). DDB table names + the
// IAM grants are wired in `amplify/backend.ts` after `defineBackend`
// resolves the data stack.
export const stripeWebhook = defineFunction({
  name: 'stripe-webhook',
  entry: './handler.ts',
  timeoutSeconds: 30,
  environment: {
    STRIPE_WEBHOOK_SECRET: secret('STRIPE_WEBHOOK_SECRET'),
    STRIPE_SECRET_KEY: secret('STRIPE_SECRET_KEY'),
  },
  resourceGroupName: 'data',
});
