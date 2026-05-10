// Step 2 of the `publishNotebookTemplate` pipeline. See
// publish-page-template-write.js for the contract.
import { util } from '@aws-appsync/utils';
import { update } from '@aws-appsync/utils/dynamodb';

export function request(ctx) {
  const target = ctx.stash.requestedVisibility;
  const id = ctx.prev.result.id;
  const now = util.time.nowISO8601();
  return update({
    key: { id },
    update: { visibility: target, updatedAtSort: now },
  });
}

export function response(ctx) {
  if (ctx.error) {
    util.error(ctx.error.message, ctx.error.type);
  }
  return ctx.result;
}
