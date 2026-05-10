// Step 1 of the `forkNotebookTemplate` pipeline. See
// fork-page-template-load.js for the contract.
import { util } from '@aws-appsync/utils';
import { get } from '@aws-appsync/utils/dynamodb';

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
  return source;
}
