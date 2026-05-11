// Reusable pipeline step 2: atomically increment today's UserDailyUsage
// row by `ctx.stash.bumpBy` (set by the calling mutation; default 1)
// with a ConditionExpression that rejects when the new total would
// exceed the caller's `dailyWriteCap`. On condition failure, AppSync
// surfaces the DDB error which we translate into a structured
// `QuotaExceeded` error with code DAILY_WRITE_LIMIT.
//
// Subsequent pipeline steps see ctx.prev.result = the original
// entitlement+sub object from check-quota-load.js (we pass it through
// via ctx.stash) so they can reuse the loaded entitlement for other
// cap checks without re-reading the table.
//
// Bind as the SECOND step of a pipeline whose first step is
// `./check-quota-load.js`, with dataSource: a.ref('UserDailyUsage').
import { util } from '@aws-appsync/utils';

export function request(ctx) {
  const prev = ctx.prev.result;
  const sub = prev.sub;
  const entitlement = prev.entitlement;
  const bumpBy = (ctx.stash && ctx.stash.bumpBy) || 1;
  const cap = entitlement.dailyWriteCap;
  const today = util.time.nowFormatted('yyyy-MM-dd', '+00:00');
  const id = sub + '#' + today;
  // 14 days from now in epoch seconds — wired to the table's TTL
  // attribute via backend.ts CDK override.
  const ttl = Math.floor(util.time.nowEpochSeconds()) + 14 * 24 * 3600;

  // Stash the loaded entitlement so step 3+ can read it without a
  // re-fetch (each pipeline step has its own ctx.prev).
  ctx.stash.entitlement = entitlement;
  ctx.stash.sub = sub;

  return {
    operation: 'UpdateItem',
    key: util.dynamodb.toMapValues({ id }),
    update: {
      expression:
        'SET strokeWrites = if_not_exists(strokeWrites, :zero) + :n, ' +
        'mutationCount = if_not_exists(mutationCount, :zero) + :one, ' +
        '#userId = :uid, ' +
        '#date = :date, ' +
        '#owner = :uid, ' +
        '#ttl = :ttl',
      expressionNames: {
        '#userId': 'userId',
        '#date': 'date',
        '#owner': 'owner',
        '#ttl': 'ttl',
      },
      expressionValues: util.dynamodb.toMapValues({
        ':zero': 0,
        ':one': 1,
        ':n': bumpBy,
        ':uid': sub,
        ':date': today,
        ':ttl': ttl,
      }),
    },
    condition: {
      expression: 'if_not_exists(strokeWrites, :zero) + :n <= :cap',
      expressionValues: util.dynamodb.toMapValues({
        ':zero': 0,
        ':n': bumpBy,
        ':cap': cap,
      }),
    },
  };
}

export function response(ctx) {
  if (ctx.error) {
    if (ctx.error.type === 'DynamoDB:ConditionalCheckFailedException') {
      const entitlement = ctx.stash.entitlement || {};
      const cap = entitlement.dailyWriteCap || 0;
      // Compute resets-at = next UTC midnight.
      const nextDay = util.time.epochMilliSecondsToISO8601(
        Math.floor(util.time.nowEpochMilliSeconds() / 86400000 + 1) *
          86400000,
      );
      util.error(
        'Daily write limit reached.',
        'QuotaExceeded',
        null,
        {
          code: 'DAILY_WRITE_LIMIT',
          limit: cap,
          resetsAt: nextDay,
        },
      );
    }
    util.error(ctx.error.message, ctx.error.type);
  }
  // Pass entitlement + sub through to subsequent steps untouched.
  return {
    sub: ctx.stash.sub,
    entitlement: ctx.stash.entitlement,
    usage: ctx.result,
  };
}
