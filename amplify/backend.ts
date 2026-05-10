import { defineBackend } from '@aws-amplify/backend';
import { Function as LambdaFunction } from 'aws-cdk-lib/aws-lambda';
import { PolicyStatement, Effect } from 'aws-cdk-lib/aws-iam';
import { auth } from './auth/resource';
import { data } from './data/resource';
import { storage } from './storage/resource';
import { assetPresign } from './functions/asset-presign/resource';

export const backend = defineBackend({
  auth,
  data,
  storage,
  assetPresign,
});

const presignFn = backend.assetPresign.resources.lambda as LambdaFunction;
const bucket = backend.storage.resources.bucket;

// Lambda only needs the bucket name — owner verification happens
// in the JS pipeline step (`check-page-template-owner.js`) which
// reads PageTemplate via the AppSync DDB dataSource. Granting the
// Lambda DDB access here would re-introduce the CFN circular
// dependency between the data + function nested stacks.
presignFn.addEnvironment('TEMPLATE_ASSETS_BUCKET_NAME', bucket.bucketName);

presignFn.addToRolePolicy(
  new PolicyStatement({
    effect: Effect.ALLOW,
    actions: ['s3:PutObject'],
    resources: [
      `${bucket.bucketArn}/protected/\${cognito-identity.amazonaws.com:sub}/templates/*`,
    ],
  }),
);
