import type { Schema } from '../../data/resource';
import { DynamoDBClient } from '@aws-sdk/client-dynamodb';
import { GetCommand, DynamoDBDocumentClient } from '@aws-sdk/lib-dynamodb';
import { S3Client, PutObjectCommand } from '@aws-sdk/client-s3';
import { getSignedUrl } from '@aws-sdk/s3-request-presigner';

const SHA256_HEX = /^[0-9a-f]{64}$/;
const MAX_BYTES = Number(process.env.MAX_ASSET_BYTES ?? '52428800');

const ddb = DynamoDBDocumentClient.from(new DynamoDBClient({}));
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
  const tableName = process.env.PAGE_TEMPLATE_TABLE_NAME;
  const bucketName = process.env.TEMPLATE_ASSETS_BUCKET_NAME;

  if (!sub) {
    throw new Error('UNAUTHENTICATED');
  }
  if (!tableName || !bucketName) {
    throw new Error('SERVER_MISCONFIGURED: missing PAGE_TEMPLATE_TABLE_NAME or TEMPLATE_ASSETS_BUCKET_NAME');
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

  const row = await ddb.send(
    new GetCommand({ TableName: tableName, Key: { id: args.templateId } }),
  );
  if (!row.Item) {
    throw new Error('NOT_FOUND');
  }
  if (row.Item.owner !== sub) {
    throw new Error('FORBIDDEN');
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
