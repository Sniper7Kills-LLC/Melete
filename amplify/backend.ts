import { defineBackend } from '@aws-amplify/backend';
import {
  Function as LambdaFunction,
  FunctionUrlAuthType,
} from 'aws-cdk-lib/aws-lambda';
import { CfnUserPoolClient } from 'aws-cdk-lib/aws-cognito';
import { PolicyStatement, Effect } from 'aws-cdk-lib/aws-iam';
import { StreamViewType } from 'aws-cdk-lib/aws-dynamodb';
import {
  StartingPosition,
  EventSourceMapping,
} from 'aws-cdk-lib/aws-lambda';
import { auth } from './auth/resource';
import { data } from './data/resource';
import { storage } from './storage/resource';
import { assetPresign } from './functions/asset-presign/resource';
import { syncStrokesBatch } from './functions/sync-strokes-batch/resource';
import { stripeWebhook } from './functions/stripe-webhook/resource';
import { stripeCheckout } from './functions/stripe-checkout/resource';
import { stripePortal } from './functions/stripe-portal/resource';
import { adminStatsStream } from './functions/admin-stats-stream/resource';
import { adminSearchUsers } from './functions/admin-search-users/resource';
import { adminMutate } from './functions/admin-mutate/resource';

export const backend = defineBackend({
  auth,
  data,
  storage,
  assetPresign,
  syncStrokesBatch,
  stripeWebhook,
  stripeCheckout,
  stripePortal,
  adminStatsStream,
  adminSearchUsers,
  adminMutate,
});

// Deep-link target for Stripe Checkout success/cancel + Portal return.
// Defaults to localhost so sandbox testing works against a local
// dev server; production deploys override via env. Stripe rejects
// localhost return URLs in live mode but accepts them in test mode.
const APP_BILLING_BASE_URL =
  process.env.APP_BILLING_BASE_URL ?? 'http://localhost:3000';

// Enable USER_PASSWORD_AUTH on the Cognito App Client. The Rust
// desktop client uses Cognito's plain InitiateAuth (USER_PASSWORD_AUTH
// flow) rather than SRP — SRP would require a much larger crypto
// dependency surface in the storage crate. SRP / refresh / custom
// flows stay enabled for the web client.
const userPoolClient = backend.auth.resources.userPoolClient.node
  .defaultChild as CfnUserPoolClient;
userPoolClient.explicitAuthFlows = [
  'ALLOW_USER_PASSWORD_AUTH',
  'ALLOW_USER_SRP_AUTH',
  'ALLOW_REFRESH_TOKEN_AUTH',
  'ALLOW_CUSTOM_AUTH',
];

const presignFn = backend.assetPresign.resources.lambda as LambdaFunction;
const bucket = backend.storage.resources.bucket;

// Lambda only needs the bucket name — owner verification happens
// in the JS pipeline step (`check-page-template-owner.js`) which
// reads PageTemplate via the AppSync DDB dataSource. Granting the
// Lambda DDB access here would re-introduce the CFN circular
// dependency between the data + function nested stacks.
presignFn.addEnvironment('TEMPLATE_ASSETS_BUCKET_NAME', bucket.bucketName);

presignFn.addToRolePolicy(
  new PolicyStatement({
    effect: Effect.ALLOW,
    actions: ['s3:PutObject'],
    resources: [
      `${bucket.bucketArn}/protected/\${cognito-identity.amazonaws.com:sub}/templates/*`,
    ],
  }),
);

// Look up Amplify-generated tables once, near the top so every
// Lambda wiring block below can reference them. Amplify Gen 2
// generates one table per model per environment; `backend.data.resources.tables`
// is the canonical handle.
const remoteStrokeTable =
  backend.data.resources.tables['RemoteStroke'];
const userEntitlementTable =
  backend.data.resources.tables['UserEntitlement'];
const tierConfigTable = backend.data.resources.tables['TierConfig'];
const userDailyUsageTable =
  backend.data.resources.tables['UserDailyUsage'];
const adminStatsTable = backend.data.resources.tables['AdminStats'];
const notebookTable = backend.data.resources.tables['Notebook'];

// Enable DDB TTL on the `ttl` attribute so daily-usage rows auto-prune
// 14 days after creation. The check-quota-bump.js resolver sets the
// epoch-seconds value; DDB does the rest. Without TTL the table grows
// linearly forever. Amplify Gen 2 wraps tables in
// `Custom::AmplifyDynamoDBTable`, so we go through the wrapper instead
// of `node.defaultChild as CfnTable` (which returns undefined for
// these tables).
backend.data.resources.cfnResources.amplifyDynamoDbTables[
  'UserDailyUsage'
].timeToLiveAttribute = {
  attributeName: 'ttl',
  enabled: true,
};
const batchFn = backend.syncStrokesBatch.resources.lambda as LambdaFunction;
batchFn.addEnvironment(
  'REMOTE_STROKE_TABLE_NAME',
  remoteStrokeTable.tableName,
);
batchFn.addToRolePolicy(
  new PolicyStatement({
    effect: Effect.ALLOW,
    actions: [
      'dynamodb:BatchWriteItem',
      'dynamodb:PutItem',
      'dynamodb:DeleteItem',
    ],
    resources: [remoteStrokeTable.tableArn],
  }),
);

// Strokes Lambda also enforces the per-user daily-write cap inline.
// Amplify Gen 2 forbids mixing JS resolver pipeline steps with a
// Lambda handler on the same mutation, so the quota check lives in
// the Lambda. Grants for read on UserEntitlement + atomic
// update-with-condition on UserDailyUsage are added below; table
// names land in the same env vars the other Stripe Lambdas use.
batchFn.addEnvironment(
  'USER_ENTITLEMENT_TABLE_NAME',
  userEntitlementTable.tableName,
);
batchFn.addEnvironment(
  'USER_DAILY_USAGE_TABLE_NAME',
  userDailyUsageTable.tableName,
);
batchFn.addEnvironment('APP_BILLING_BASE_URL', APP_BILLING_BASE_URL);
batchFn.addToRolePolicy(
  new PolicyStatement({
    effect: Effect.ALLOW,
    actions: ['dynamodb:GetItem'],
    resources: [userEntitlementTable.tableArn],
  }),
);
batchFn.addToRolePolicy(
  new PolicyStatement({
    effect: Effect.ALLOW,
    actions: ['dynamodb:UpdateItem'],
    resources: [userDailyUsageTable.tableArn],
  }),
);

// Stripe webhook receiver. Public Function URL (auth=NONE) because
// Stripe authenticates each delivery via signature, not IAM. Lambda
// upserts UserEntitlement rows and reads TierConfig to map Stripe
// price IDs → tier names. Webhook URL is emitted via `addOutput` so
// it lands in amplify_outputs.json and the deployer can paste it into
// the Stripe Dashboard endpoint config.
const stripeWebhookFn =
  backend.stripeWebhook.resources.lambda as LambdaFunction;
stripeWebhookFn.addEnvironment(
  'USER_ENTITLEMENT_TABLE_NAME',
  userEntitlementTable.tableName,
);
stripeWebhookFn.addEnvironment(
  'TIER_CONFIG_TABLE_NAME',
  tierConfigTable.tableName,
);
stripeWebhookFn.addToRolePolicy(
  new PolicyStatement({
    effect: Effect.ALLOW,
    actions: [
      'dynamodb:GetItem',
      'dynamodb:PutItem',
      'dynamodb:UpdateItem',
      'dynamodb:Scan',
    ],
    resources: [
      userEntitlementTable.tableArn,
      tierConfigTable.tableArn,
    ],
  }),
);
const stripeWebhookUrl = stripeWebhookFn.addFunctionUrl({
  authType: FunctionUrlAuthType.NONE,
});
backend.addOutput({
  custom: {
    stripeWebhookUrl: stripeWebhookUrl.url,
    tierConfigTableName: tierConfigTable.tableName,
    userEntitlementTableName: userEntitlementTable.tableName,
  },
});

// Checkout + Portal Lambdas share the same TierConfig + UserEntitlement
// access pattern as the webhook (read TierConfig, read/upsert
// UserEntitlement). Wired identically so the resolver-bound functions
// can resolve price IDs + the caller's Stripe customer.
const stripeCheckoutFn =
  backend.stripeCheckout.resources.lambda as LambdaFunction;
const stripePortalFn =
  backend.stripePortal.resources.lambda as LambdaFunction;

for (const fn of [stripeCheckoutFn, stripePortalFn]) {
  fn.addEnvironment(
    'USER_ENTITLEMENT_TABLE_NAME',
    userEntitlementTable.tableName,
  );
  fn.addEnvironment('TIER_CONFIG_TABLE_NAME', tierConfigTable.tableName);
  fn.addEnvironment('APP_BILLING_BASE_URL', APP_BILLING_BASE_URL);
  fn.addToRolePolicy(
    new PolicyStatement({
      effect: Effect.ALLOW,
      actions: ['dynamodb:GetItem'],
      resources: [
        userEntitlementTable.tableArn,
        tierConfigTable.tableArn,
      ],
    }),
  );
}

// AdminStats maintainer — subscribes to DDB streams on
// UserEntitlement + Notebook so the admin dashboard can read a single
// pre-aggregated row instead of scanning the source tables. Streams
// must be enabled on each source table first (NEW_AND_OLD_IMAGES so
// the handler can diff transitions); then we hang an
// EventSourceMapping off the Lambda for each stream ARN. Amplify
// Gen 2 uses `Custom::AmplifyDynamoDBTable`, so stream config goes
// through the wrapper.
for (const name of ['UserEntitlement', 'Notebook']) {
  backend.data.resources.cfnResources.amplifyDynamoDbTables[
    name
  ].streamSpecification = {
    streamViewType: StreamViewType.NEW_AND_OLD_IMAGES,
  };
}

const adminStatsStreamFn =
  backend.adminStatsStream.resources.lambda as LambdaFunction;
adminStatsStreamFn.addEnvironment(
  'ADMIN_STATS_TABLE_NAME',
  adminStatsTable.tableName,
);
adminStatsStreamFn.addEnvironment(
  'USER_ENTITLEMENT_TABLE_ARN',
  userEntitlementTable.tableArn,
);
adminStatsStreamFn.addEnvironment(
  'NOTEBOOK_TABLE_ARN',
  notebookTable.tableArn,
);
adminStatsStreamFn.addToRolePolicy(
  new PolicyStatement({
    effect: Effect.ALLOW,
    actions: ['dynamodb:UpdateItem'],
    resources: [adminStatsTable.tableArn],
  }),
);
adminStatsStreamFn.addToRolePolicy(
  new PolicyStatement({
    effect: Effect.ALLOW,
    actions: [
      'dynamodb:DescribeStream',
      'dynamodb:GetRecords',
      'dynamodb:GetShardIterator',
      'dynamodb:ListStreams',
    ],
    // Stream ARNs aren't resolvable at synth time when defined via
    // streamSpecification; wildcard-scope to the two source tables.
    resources: [
      `${userEntitlementTable.tableArn}/stream/*`,
      `${notebookTable.tableArn}/stream/*`,
    ],
  }),
);

new EventSourceMapping(
  adminStatsStreamFn.stack,
  'AdminStatsUserEntitlementStream',
  {
    target: adminStatsStreamFn,
    eventSourceArn: userEntitlementTable.tableStreamArn,
    batchSize: 25,
    startingPosition: StartingPosition.LATEST,
    retryAttempts: 3,
  },
);
new EventSourceMapping(
  adminStatsStreamFn.stack,
  'AdminStatsNotebookStream',
  {
    target: adminStatsStreamFn,
    eventSourceArn: notebookTable.tableStreamArn,
    batchSize: 25,
    startingPosition: StartingPosition.LATEST,
    retryAttempts: 3,
  },
);

// Admin search-by-email Lambda needs ListUsers on the Cognito User
// Pool that defineAuth provisioned. The user-pool ID is exposed via
// the auth resource; the IAM policy scopes to that ARN only.
const adminSearchFn =
  backend.adminSearchUsers.resources.lambda as LambdaFunction;
const userPool = backend.auth.resources.userPool;
adminSearchFn.addEnvironment('USER_POOL_ID', userPool.userPoolId);
adminSearchFn.addToRolePolicy(
  new PolicyStatement({
    effect: Effect.ALLOW,
    actions: ['cognito-idp:ListUsers'],
    resources: [userPool.userPoolArn],
  }),
);

// Superadmin mutation dispatcher. Reads/writes UserEntitlement +
// UserDailyUsage, writes to AdminAuditLog (every action), calls
// Cognito (disable/delete) and Stripe (extendTrial). Permissions
// stack accordingly.
const adminAuditLogTable = backend.data.resources.tables['AdminAuditLog'];
const adminMutateFn =
  backend.adminMutate.resources.lambda as LambdaFunction;
adminMutateFn.addEnvironment(
  'USER_ENTITLEMENT_TABLE_NAME',
  userEntitlementTable.tableName,
);
adminMutateFn.addEnvironment(
  'USER_DAILY_USAGE_TABLE_NAME',
  userDailyUsageTable.tableName,
);
adminMutateFn.addEnvironment(
  'ADMIN_AUDIT_LOG_TABLE_NAME',
  adminAuditLogTable.tableName,
);
adminMutateFn.addEnvironment(
  'TIER_CONFIG_TABLE_NAME',
  tierConfigTable.tableName,
);
adminMutateFn.addEnvironment('USER_POOL_ID', userPool.userPoolId);
adminMutateFn.addToRolePolicy(
  new PolicyStatement({
    effect: Effect.ALLOW,
    actions: [
      'dynamodb:GetItem',
      'dynamodb:PutItem',
      'dynamodb:UpdateItem',
      'dynamodb:DeleteItem',
    ],
    resources: [
      userEntitlementTable.tableArn,
      userDailyUsageTable.tableArn,
      adminAuditLogTable.tableArn,
      tierConfigTable.tableArn,
    ],
  }),
);
adminMutateFn.addToRolePolicy(
  new PolicyStatement({
    effect: Effect.ALLOW,
    actions: [
      'cognito-idp:AdminDisableUser',
      'cognito-idp:AdminDeleteUser',
      'cognito-idp:AdminGetUser',
    ],
    resources: [userPool.userPoolArn],
  }),
);
