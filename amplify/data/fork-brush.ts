import { util, type Context } from '@aws-appsync/utils';
import { get, put, update, operations } from '@aws-appsync/utils/dynamodb';

type ForkArgs = { id: string };

type BrushRow = {
  id: string;
  owner?: string | null;
  name: string;
  description?: string | null;
  visibility: 'PRIVATE' | 'UNLISTED' | 'PUBLIC';
  bodyToml: string;
  forkedFrom?: string | null;
  forkCount?: number;
  viewCount?: number;
  updatedAtSort: string;
  createdAt?: string;
  updatedAt?: string;
};

export function request(ctx: Context<ForkArgs>) {
  const sub = ctx.identity && (ctx.identity as { sub?: string }).sub;
  if (!sub) {
    util.unauthorized();
  }
  return get<BrushRow>({ key: { id: ctx.args.id } });
}

export function response(ctx: Context<ForkArgs>) {
  if (ctx.error) {
    util.error(ctx.error.message, ctx.error.type);
  }
  const source = ctx.result as BrushRow | null;
  if (!source) {
    util.error('NOT_FOUND', 'NotFound');
  }
  const sub = (ctx.identity as { sub: string }).sub;
  if (source!.visibility === 'PRIVATE' && source!.owner !== sub) {
    util.unauthorized();
  }

  const now = util.time.nowISO8601();
  const newId = util.autoId();
  const newRow: BrushRow = {
    id: newId,
    owner: sub,
    name: source!.name,
    description: source!.description ?? null,
    visibility: 'PRIVATE',
    bodyToml: source!.bodyToml,
    forkedFrom: source!.id,
    forkCount: 0,
    viewCount: 0,
    updatedAtSort: now,
    createdAt: now,
    updatedAt: now,
  };

  ctx.stash.put_new = put<BrushRow>({ key: { id: newId }, item: newRow });
  ctx.stash.update_source = update<BrushRow>({
    key: { id: source!.id },
    update: { forkCount: operations.increment(1), updatedAtSort: now },
  });
  return newRow;
}
