// Step 1 of the `publishBrush` pipeline. See
// publish-page-template-load.js for the contract.
import { util } from '@aws-appsync/utils';
import { get } from '@aws-appsync/utils/dynamodb';

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
  return row;
}
