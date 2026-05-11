import { defineFunction, secret } from '@aws-amplify/backend';

// Stripe Customer Portal link generator. Reads the caller's
// `stripeCustomerId` from their UserEntitlement row and mints a
// portal session URL — the user manages their plan / payment
// method / invoices in Stripe's hosted UI. Avoids re-implementing
// billing UX inside the app.
export const stripePortal = defineFunction({
  name: 'stripe-portal',
  entry: './handler.ts',
  timeoutSeconds: 15,
  environment: {
    STRIPE_SECRET_KEY: secret('STRIPE_SECRET_KEY'),
  },
  resourceGroupName: 'data',
});
