import { defineFunction } from '@aws-amplify/backend';

// Batch-CRUD on the RemoteStroke table. Replaces the per-row AppSync
// mutation chatter the desktop's worker pool used to fire — eraser
// bursts of hundreds of strokes now take one round-trip per 25 ops
// instead of one per stroke. Backed by DynamoDB BatchWriteItem.
//
// `RemoteStrokeTableName` is wired in `amplify/backend.ts` after
// `defineBackend` resolves the table ARN/name.
export const syncStrokesBatch = defineFunction({
  name: 'sync-strokes-batch',
  entry: './handler.ts',
  timeoutSeconds: 30,
  // Pin to the data stack so the CFN nested-stack dependency runs
  // function → data table without forming the function ↔ data
  // circular reference (Amplify Gen 2 requires this when the Lambda
  // calls into a model's table).
  resourceGroupName: 'data',
});
