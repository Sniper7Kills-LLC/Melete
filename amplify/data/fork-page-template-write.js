// Step 2 of the `forkPageTemplate` pipeline: PutItem a fresh PRIVATE
// copy owned by the caller, with `forkedFrom` pointing at the source.
// Returning the put result also returns the new row to the client.
import { util } from '@aws-appsync/utils';
import { put } from '@aws-appsync/utils/dynamodb';

export function request(ctx) {
  const source = ctx.prev.result;
  const sub = ctx.identity.sub;
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
  return put({ key: { id: newId }, item: newRow });
}

export function response(ctx) {
  if (ctx.error) {
    util.error(ctx.error.message, ctx.error.type);
  }
  return ctx.result;
}
