import { util, type Context } from '@aws-appsync/utils';
import { get, update } from '@aws-appsync/utils/dynamodb';

type PublishArgs = { id: string; visibility: 'PRIVATE' | 'UNLISTED' | 'PUBLIC' };

type BrushRow = {
  id: string;
  owner?: string | null;
  visibility: 'PRIVATE' | 'UNLISTED' | 'PUBLIC';
};

export function request(ctx: Context<PublishArgs>) {
  const sub = ctx.identity && (ctx.identity as { sub?: string }).sub;
  if (!sub) {
    util.unauthorized();
  }
  ctx.stash.requestedVisibility = ctx.args.visibility;
  return get<BrushRow>({ key: { id: ctx.args.id } });
}

export function response(ctx: Context<PublishArgs>) {
  if (ctx.error) {
    util.error(ctx.error.message, ctx.error.type);
  }
  const row = ctx.result as BrushRow | null;
  if (!row) {
    util.error('NOT_FOUND', 'NotFound');
  }
  const sub = (ctx.identity as { sub: string }).sub;
  if (row!.owner !== sub) {
    util.unauthorized();
  }

  const target = ctx.stash.requestedVisibility as 'PRIVATE' | 'UNLISTED' | 'PUBLIC';
  ctx.stash.update_visibility = update<BrushRow>({
    key: { id: row!.id },
    update: { visibility: target },
  });
  return { ...row!, visibility: target };
}
