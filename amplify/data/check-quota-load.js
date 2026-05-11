// Reusable pipeline step 1: load the caller's UserEntitlement so the
// next step can compare counters against tier caps. Pass-through on
// success; pipeline aborts via util.error on missing row.
//
// Bind via `a.handler.custom({ entry: './check-quota-load.js',
// dataSource: a.ref('UserEntitlement') })` as the first step in any
// mutation pipeline that needs quota enforcement.
//
// Result shape (visible to subsequent steps via ctx.prev.result):
//   { sub, entitlement }
import { util } from '@aws-appsync/utils';
import { get } from '@aws-appsync/utils/dynamodb';

export function request(ctx) {
  const sub = ctx.identity && ctx.identity.sub;
  if (!sub) {
    util.unauthorized();
  }
  return get({ key: { id: sub } });
}

export function response(ctx) {
  if (ctx.error) {
    util.error(ctx.error.message, ctx.error.type);
  }
  const entitlement = ctx.result;
  if (!entitlement) {
    // First-time user — no row yet. Webhook writes one on first
    // paid signup; until then we synthesise a `free` cap profile so
    // the rest of the pipeline can proceed without a 500.
    return {
      sub: ctx.identity.sub,
      entitlement: {
        tier: 'free',
        status: 'active',
        notebookCap: 1,
        strokesPerPageCap: 10000,
        strokesPerNotebookCap: 50000,
        dailyWriteCap: 1000,
        s3BytesCap: 52428800,
        templatePublishCap: 3,
        historyDays: 0,
        liveSyncEnabled: false,
      },
    };
  }
  return { sub: ctx.identity.sub, entitlement };
}
