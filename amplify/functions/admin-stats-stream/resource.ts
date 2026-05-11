import { defineFunction } from '@aws-amplify/backend';

// DDB-stream consumer that maintains the singleton AdminStats row
// (PK = "global") in response to UserEntitlement + Notebook lifecycle
// events. Stream sources, table grants, and `eventSourceMapping`
// wiring all happen in `amplify/backend.ts` after the data stack
// resolves.
export const adminStatsStream = defineFunction({
  name: 'admin-stats-stream',
  entry: './handler.ts',
  timeoutSeconds: 60,
  resourceGroupName: 'data',
});
