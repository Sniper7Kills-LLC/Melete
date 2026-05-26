import { defineFunction } from '@aws-amplify/backend';

// Nightly aggregator that walks UserEntitlement + UserDailyUsage +
// Notebook (byOwner GSI) and writes top-100 rankings to AdminTopUsers
// for the admin dashboard to read without re-aggregating per request.
// Wired into an EventBridge schedule (02:00 UTC) and IAM-scoped in
// `amplify/backend.ts`. Tabular metrics: `strokes` (14-day rolling
// strokeWrites total) and `notebooks` (count of Notebook rows owned).
// `s3_bytes` is deferred per the issue body.
export const adminTopUsersAggregator = defineFunction({
  name: 'admin-top-users-aggregator',
  entry: './handler.ts',
  timeoutSeconds: 600,
  memoryMB: 512,
  resourceGroupName: 'data',
});
