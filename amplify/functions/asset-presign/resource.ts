import { defineFunction } from '@aws-amplify/backend';

export const assetPresign = defineFunction({
  name: 'asset-presign',
  entry: './handler.ts',
  timeoutSeconds: 15,
  environment: {
    MAX_ASSET_BYTES: '52428800',
  },
});
