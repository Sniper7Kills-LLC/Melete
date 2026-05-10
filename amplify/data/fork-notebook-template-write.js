// Step 2 of the `forkNotebookTemplate` pipeline. See
// fork-page-template-write.js for the contract.
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
    visibility: 'PRIVATE',
    bodyToml: source.bodyToml,
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
