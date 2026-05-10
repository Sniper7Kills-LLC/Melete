// Custom AppSync JS resolver for `publishBrush`. Same shape as
// publish-page-template.js but for the Brush model (no asset re-copy).
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
  const now = util.time.nowISO8601();
  ctx.stash.update_visibility = update({
    key: { id: row.id },
    update: { visibility: target, updatedAtSort: now },
  });
  return Object.assign({}, row, { visibility: target, updatedAtSort: now });
}
