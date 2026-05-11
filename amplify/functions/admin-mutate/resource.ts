import { defineFunction, secret } from '@aws-amplify/backend';

// Single-Lambda dispatcher for every superadmin mutation. Each action
// reads the target's current state for the `before` snapshot, applies
// the change, writes an AdminAuditLog row, and returns the new state.
// Keeping the dispatcher in one place means one IAM policy, one
// secret binding, and a uniform audit shape across actions.
export const adminMutate = defineFunction({
  name: 'admin-mutate',
  entry: './handler.ts',
  timeoutSeconds: 30,
  environment: {
    STRIPE_SECRET_KEY: secret('STRIPE_SECRET_KEY'),
  },
  resourceGroupName: 'data',
});
