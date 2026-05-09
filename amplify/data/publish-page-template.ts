import { util, type Context } from '@aws-appsync/utils';
import { get, update } from '@aws-appsync/utils/dynamodb';

type PublishArgs = { id: string; visibility: 'PRIVATE' | 'UNLISTED' | 'PUBLIC' };

type PageTemplateRow = {
  id: string;
  owner?: string | null;
  visibility: 'PRIVATE' | 'UNLISTED' | 'PUBLIC';
  assets?: unknown;
};

export function request(ctx: Context<PublishArgs>) {
  const sub = ctx.identity && (ctx.identity as { sub?: string }).sub;
  if (!sub) {
    util.unauthorized();
  }
  ctx.stash.requestedVisibility = ctx.args.visibility;
  return get<PageTemplateRow>({ key: { id: ctx.args.id } });
}

export function response(ctx: Context<PublishArgs>) {
  if (ctx.error) {
    util.error(ctx.error.message, ctx.error.type);
  }
  const row = ctx.result as PageTemplateRow | null;
  if (!row) {
    util.error('NOT_FOUND', 'NotFound');
  }
  const sub = (ctx.identity as { sub: string }).sub;
  if (row!.owner !== sub) {
    util.unauthorized();
  }

  const target = ctx.stash.requestedVisibility as 'PRIVATE' | 'UNLISTED' | 'PUBLIC';
  // TODO(sandbox-creds): when row!.visibility !== 'PUBLIC' && target === 'PUBLIC',
  // invoke an S3-CopyObject Lambda to copy each asset from
  // protected/{owner}/templates/{id}/assets/{sha256} -> public/templates/{id}/assets/{sha256}.
  // Stubbed until the user installs AWS creds and runs `npx ampx sandbox`.

  ctx.stash.update_visibility = update<PageTemplateRow>({
    key: { id: row!.id },
    update: { visibility: target },
  });
  return { ...row!, visibility: target };
}
