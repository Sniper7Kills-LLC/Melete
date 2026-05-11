/**
 * One-shot seed for the `TierConfig` DDB table.
 *
 * Idempotent — re-running overwrites the same three rows. Run after
 * the first sandbox/prod deploy to populate the tier defaults that
 * the Stripe webhook + Checkout + EntitlementService all read.
 *
 *   tsx amplify/scripts/seed-tier-config.ts <env-name>
 *
 * Where <env-name> is the Amplify sandbox name (defaults to the
 * Amplify-Gen-2 default). The script reads the table name from
 * `amplify_outputs.json` so it works against whatever stack the CLI
 * just deployed.
 *
 * Stripe price IDs are read from the environment:
 *   STRIPE_PRICE_PRO_MONTHLY     STRIPE_PRICE_STUDIO_MONTHLY
 *   STRIPE_PRICE_PRO_YEARLY      STRIPE_PRICE_STUDIO_YEARLY
 *
 * Set them in Stripe Dashboard first, then export before running.
 */

import { readFile } from 'node:fs/promises';
import { DynamoDBClient } from '@aws-sdk/client-dynamodb';
import { DynamoDBDocumentClient, PutCommand } from '@aws-sdk/lib-dynamodb';

interface TierRow {
  id: 'free' | 'pro' | 'studio';
  notebookCap: number;
  strokesPerPageCap: number;
  strokesPerNotebookCap: number;
  dailyWriteCap: number;
  s3BytesCap: number;
  templatePublishCap: number;
  historyDays: number;
  liveSyncEnabled: boolean;
  priceMonthlyCents: number;
  priceYearlyCents: number;
  stripePriceIdMonthly: string | null;
  stripePriceIdYearly: string | null;
}

const TIERS: TierRow[] = [
  {
    id: 'free',
    notebookCap: 1,
    strokesPerPageCap: 10_000,
    strokesPerNotebookCap: 50_000,
    dailyWriteCap: 1_000,
    s3BytesCap: 50 * 1024 * 1024, // 50 MB
    templatePublishCap: 3,
    historyDays: 0,
    liveSyncEnabled: false,
    priceMonthlyCents: 0,
    priceYearlyCents: 0,
    stripePriceIdMonthly: null,
    stripePriceIdYearly: null,
  },
  {
    id: 'pro',
    notebookCap: 10,
    strokesPerPageCap: 100_000,
    strokesPerNotebookCap: 2_000_000,
    dailyWriteCap: 30_000,
    s3BytesCap: 10 * 1024 * 1024 * 1024, // 10 GB
    templatePublishCap: 50,
    historyDays: 0,
    liveSyncEnabled: true,
    priceMonthlyCents: 800,
    priceYearlyCents: 8000,
    // Placeholder so the Checkout Lambda gets past its local
    // `PRICE_ID_MISSING` guard and hits the actual Stripe API (which
    // will then reject with a real Stripe error if the key is also
    // a placeholder). Replace via env when wiring real Stripe.
    stripePriceIdMonthly:
      process.env.STRIPE_PRICE_PRO_MONTHLY ?? 'price_placeholder_pro_monthly',
    stripePriceIdYearly:
      process.env.STRIPE_PRICE_PRO_YEARLY ?? 'price_placeholder_pro_yearly',
  },
  {
    id: 'studio',
    notebookCap: 20,
    strokesPerPageCap: 200_000,
    strokesPerNotebookCap: 2_000_000,
    dailyWriteCap: 60_000,
    s3BytesCap: 30 * 1024 * 1024 * 1024, // 30 GB
    templatePublishCap: -1, // unlimited
    historyDays: 0,
    liveSyncEnabled: true,
    priceMonthlyCents: 1800,
    priceYearlyCents: 18000,
    stripePriceIdMonthly:
      process.env.STRIPE_PRICE_STUDIO_MONTHLY ??
      'price_placeholder_studio_monthly',
    stripePriceIdYearly:
      process.env.STRIPE_PRICE_STUDIO_YEARLY ??
      'price_placeholder_studio_yearly',
  },
];

interface AmplifyOutputs {
  custom?: { tierConfigTableName?: string };
}

async function resolveTableName(): Promise<string> {
  const envName = process.env.TIER_CONFIG_TABLE_NAME;
  if (envName) return envName;

  // Fallback: read from `amplify_outputs.json` — the CLI writes this
  // after every deploy. The custom output is added in backend.ts.
  const outputs = JSON.parse(
    await readFile('amplify_outputs.json', 'utf-8'),
  ) as AmplifyOutputs;
  const name = outputs.custom?.tierConfigTableName;
  if (!name) {
    throw new Error(
      'Set TIER_CONFIG_TABLE_NAME env var or expose `tierConfigTableName` via backend.addOutput in backend.ts',
    );
  }
  return name;
}

async function main(): Promise<void> {
  const tableName = await resolveTableName();
  const ddb = DynamoDBDocumentClient.from(new DynamoDBClient({}));

  // Amplify Gen 2's auto-generated list/get resolvers filter out rows
  // missing the required system fields (`__typename`, `createdAt`,
  // `updatedAt`). Seed rows written via raw DDB PutItem without these
  // attributes are invisible to GraphQL queries — surface them here
  // so the billing page can read TierConfig via the data client.
  const nowIso = new Date().toISOString();
  for (const tier of TIERS) {
    await ddb.send(
      new PutCommand({
        TableName: tableName,
        Item: {
          ...tier,
          __typename: 'TierConfig',
          createdAt: nowIso,
          updatedAt: nowIso,
        },
      }),
    );
    console.log(`seeded ${tier.id}`);
  }
  console.log('TierConfig seeded.');
}

void main().catch((err) => {
  console.error(err);
  process.exit(1);
});
