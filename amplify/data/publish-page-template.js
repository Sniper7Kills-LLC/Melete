// Custom AppSync JS resolver for `publishPageTemplate`. Verifies the
// caller owns the row, then updates `visibility` + `updatedAtSort`.
//
// Authored as raw JS (not TS) because Amplify Gen 2 uploads custom
// `a.handler.custom({ entry: ... })` files to S3 verbatim — AppSync's
// APPSYNC_JS runtime parses them as plain JavaScript and chokes on
// TypeScript syntax (type aliases, `as` casts, generics).
import { util } from '@aws-appsync/utils';
import { get, update } from '@aws-appsync/utils/dynamodb';

export function request(ctx) {
  const sub = ctx.identity && ctx.identity.sub;
  if (!sub) {
    util.unauthorized();
  }
  ctx.stash.requestedVisibility = ctx.args.visibility;
  return get({ key: { id: ctx.args.id } });
}

export function response(ctx) {
  if (ctx.error) {
    util.error(ctx.error.message, ctx.error.type);
  }
  const row = ctx.result;
  if (!row) {
    util.error('NOT_FOUND', 'NotFound');
  }
  const sub = ctx.identity.sub;
  if (row.owner !== sub) {
    util.unauthorized();
  }

  const target = ctx.stash.requestedVisibility;
  // TODO(post-sandbox): when row.visibility !== 'PUBLIC' && target === 'PUBLIC',
  // invoke an S3-CopyObject Lambda to copy each asset from
  // protected/{owner}/templates/{id}/assets/{sha256} -> public/templates/{id}/assets/{sha256}.

  const now = util.time.nowISO8601();
  ctx.stash.update_visibility = update({
    key: { id: row.id },
    update: { visibility: target, updatedAtSort: now },
  });
  return Object.assign({}, row, { visibility: target, updatedAtSort: now });
}
