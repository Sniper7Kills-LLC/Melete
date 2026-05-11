import { defineFunction, secret } from '@aws-amplify/backend';

// Stripe Checkout link generator. Authenticated AppSync mutation
// produces a hosted Checkout Session URL for the requested
// (tier, interval). Subscription metadata carries the Cognito sub
// so the webhook can project the resulting subscription onto a
// UserEntitlement row. `APP_BILLING_BASE_URL` (deep-link redirect
// target after success/cancel) is wired in `amplify/backend.ts`.
export const stripeCheckout = defineFunction({
  name: 'stripe-checkout',
  entry: './handler.ts',
  timeoutSeconds: 15,
  environment: {
    STRIPE_SECRET_KEY: secret('STRIPE_SECRET_KEY'),
  },
  resourceGroupName: 'data',
});
