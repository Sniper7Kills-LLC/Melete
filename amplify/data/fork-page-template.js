// Custom AppSync JS resolver for `forkPageTemplate`. Loads the source
// row, then in `response` returns a freshly-IDed clone owned by the
// caller. NOTE: the `put_new` / `update_source` stashes are no-ops
// in this single-resolver shape (no follow-on pipeline step consumes
// them); the response just shapes the return value. Wire a real
// 2-step pipeline post-sandbox if forks need to actually persist.
import { util } from '@aws-appsync/utils';
import { get, put, update, operations } from '@aws-appsync/utils/dynamodb';

export function request(ctx) {
  const sub = ctx.identity && ctx.identity.sub;
  if (!sub) {
    util.unauthorized();
  }
  return get({ key: { id: ctx.args.id } });
}

export function response(ctx) {
  if (ctx.error) {
    util.error(ctx.error.message, ctx.error.type);
  }
  const source = ctx.result;
  if (!source) {
    util.error('NOT_FOUND', 'NotFound');
  }
  const sub = ctx.identity.sub;
  if (source.visibility === 'PRIVATE' && source.owner !== sub) {
    util.unauthorized();
  }

  const now = util.time.nowISO8601();
  const newId = util.autoId();
  const newRow = {
    id: newId,
    owner: sub,
    name: source.name,
    description: source.description == null ? null : source.description,
    category: source.category == null ? null : source.category,
    visibility: 'PRIVATE',
    bodyToml: source.bodyToml,
    assets: source.assets == null ? null : source.assets,
    forkedFrom: source.id,
    forkCount: 0,
    viewCount: 0,
    updatedAtSort: now,
    createdAt: now,
    updatedAt: now,
  };

  ctx.stash.put_new = put({ key: { id: newId }, item: newRow });
  ctx.stash.update_source = update({
    key: { id: source.id },
    update: { forkCount: operations.increment(1), updatedAtSort: now },
  });
  return newRow;
}
