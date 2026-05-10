import type { Schema } from '../../data/resource';
import { S3Client, PutObjectCommand } from '@aws-sdk/client-s3';
import { getSignedUrl } from '@aws-sdk/s3-request-presigner';

// Validates args + mints a presigned PUT URL whose key is rooted at
// `protected/{caller_sub}/templates/...`. The bucket policy
// (`protected/{entity_id}/*` → entity('identity').to([read, write,
// delete])) is what physically prevents one user from writing into
// another user's prefix; this Lambda just shapes the URL. Owner-of
// -templateId is intentionally NOT checked here — see the comment on
// `getAssetUploadUrl` in `amplify/data/resource.ts` for why.

const SHA256_HEX = /^[0-9a-f]{64}$/;
const MAX_BYTES = Number(process.env.MAX_ASSET_BYTES ?? '52428800');

const s3 = new S3Client({});

type Args = {
  templateId: string;
  sha256: string;
  contentType: string;
  sizeBytes: number;
};

type Result = {
  uploadUrl: string;
  s3Key: string;
};

export const handler: Schema['getAssetUploadUrl']['functionHandler'] = async (event) => {
  const args = event.arguments as Args;
  const identity = event.identity as { sub?: string } | undefined;
  const sub = identity?.sub;
  const bucketName = process.env.TEMPLATE_ASSETS_BUCKET_NAME;

  if (!sub) {
    throw new Error('UNAUTHENTICATED');
  }
  if (!bucketName) {
    throw new Error('SERVER_MISCONFIGURED: missing TEMPLATE_ASSETS_BUCKET_NAME');
  }
  if (!args.templateId || typeof args.templateId !== 'string') {
    throw new Error('INVALID_TEMPLATE_ID');
  }
  if (!SHA256_HEX.test(args.sha256)) {
    throw new Error('INVALID_SHA256: must be 64-char lowercase hex');
  }
  if (!Number.isInteger(args.sizeBytes) || args.sizeBytes <= 0 || args.sizeBytes > MAX_BYTES) {
    throw new Error(`INVALID_SIZE: must be 1..${MAX_BYTES}`);
  }
  if (!args.contentType || typeof args.contentType !== 'string') {
    throw new Error('INVALID_CONTENT_TYPE');
  }

  const s3Key = `protected/${sub}/templates/${args.templateId}/assets/${args.sha256}`;

  const cmd = new PutObjectCommand({
    Bucket: bucketName,
    Key: s3Key,
    ContentType: args.contentType,
    ContentLength: args.sizeBytes,
    ChecksumSHA256: undefined,
    Metadata: {
      sha256: args.sha256,
      'template-id': args.templateId,
    },
  });

  const uploadUrl = await getSignedUrl(s3, cmd, {
    expiresIn: 300,
    unhoistableHeaders: new Set(['x-amz-content-sha256']),
  });

  const result: Result = { uploadUrl, s3Key };
  return result;
};
