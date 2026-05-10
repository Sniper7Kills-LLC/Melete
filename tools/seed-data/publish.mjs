#!/usr/bin/env node
//
// Seed-publish script (issue #51).
//
// Walks `tools/seed-data/{page_templates,notebook_templates}/*.toml`
// and writes each entry to the corresponding DynamoDB table with
// visibility = 'PUBLIC'. Idempotent: re-running upserts the same id.
//
// Why DynamoDB direct (not AppSync GraphQL): the seed entries have
// stable ids and need to land as system-owned (no human user). Going
// through AppSync would require Cognito sign-in and would tag the
// rows with a user sub. Writing to DDB directly with the Amplify
// admin role bypasses both.
//
// Usage:
//   AWS creds in env / ~/.aws/, then:
//     node tools/seed-data/publish.mjs
//
//   Discovers DDB table names by listing tables in the configured
//   region that match the Amplify naming convention:
//   `<Model>-<api-id>-NONE`. If multiple sandboxes are deployed in
//   the same region you can pin one with TABLE_SUFFIX=<api-id>.

import { readFile, readdir } from 'node:fs/promises';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createHash } from 'node:crypto';

import { DynamoDBClient, ListTablesCommand } from '@aws-sdk/client-dynamodb';
import { DynamoDBDocumentClient, PutCommand } from '@aws-sdk/lib-dynamodb';
import * as toml from 'toml';

const __dirname = dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = join(__dirname, '..', '..');

async function main() {
  const outputsPath = join(REPO_ROOT, 'amplify_outputs.json');
  const outputsRaw = await readFile(outputsPath, 'utf8');
  const outputs = JSON.parse(outputsRaw);
  const region = outputs?.data?.aws_region;
  if (!region) {
    throw new Error('amplify_outputs.json missing data.aws_region — is the sandbox deployed?');
  }

  const ddbRaw = new DynamoDBClient({ region });
  const ddb = DynamoDBDocumentClient.from(ddbRaw, {
    marshallOptions: { removeUndefinedValues: true },
  });

  const tables = await discoverTables(ddbRaw, process.env.TABLE_SUFFIX);
  console.error(`tables: page=${tables.page} notebook=${tables.notebook} brush=${tables.brush}`);

  let written = 0;
  let failed = 0;

  const pageDir = join(REPO_ROOT, 'tools', 'seed-data', 'page_templates');
  for (const file of await tomlFiles(pageDir)) {
    try {
      const item = await buildPageItem(file);
      console.error(`  page → ${item.name} (${item.id})`);
      await ddb.send(new PutCommand({ TableName: tables.page, Item: item }));
      written++;
    } catch (e) {
      console.error(`  failed ${file}: ${e.message ?? e}`);
      failed++;
    }
  }

  const nbDir = join(REPO_ROOT, 'tools', 'seed-data', 'notebook_templates');
  for (const file of await tomlFiles(nbDir)) {
    try {
      const item = await buildNotebookItem(file);
      console.error(`  notebook → ${item.name} (${item.id})`);
      await ddb.send(new PutCommand({ TableName: tables.notebook, Item: item }));
      written++;
    } catch (e) {
      console.error(`  failed ${file}: ${e.message ?? e}`);
      failed++;
    }
  }

  console.error(`done: ${written} written, ${failed} failed`);
  if (failed > 0) process.exit(2);
}

async function discoverTables(ddbRaw, pinSuffix) {
  const out = await ddbRaw.send(new ListTablesCommand({}));
  const all = out.TableNames ?? [];
  const pick = (model) => {
    const matches = all.filter((n) => n.startsWith(`${model}-`) && n.endsWith('-NONE'));
    if (pinSuffix) {
      const suffixed = matches.find((n) => n === `${model}-${pinSuffix}-NONE`);
      if (suffixed) return suffixed;
      throw new Error(`no ${model} table for TABLE_SUFFIX=${pinSuffix}`);
    }
    if (matches.length === 0) throw new Error(`no ${model}-*-NONE table found`);
    if (matches.length > 1) {
      throw new Error(
        `multiple ${model}-*-NONE tables found; set TABLE_SUFFIX=<api-id> to disambiguate. Found: ${matches.join(
          ', ',
        )}`,
      );
    }
    return matches[0];
  };
  return {
    page: pick('PageTemplate'),
    notebook: pick('NotebookTemplate'),
    brush: pick('Brush'),
  };
}

async function tomlFiles(dir) {
  let entries;
  try {
    entries = await readdir(dir);
  } catch (e) {
    if (e.code === 'ENOENT') return [];
    throw e;
  }
  return entries
    .filter((f) => f.endsWith('.toml'))
    .sort()
    .map((f) => join(dir, f));
}

function nowIso() {
  return new Date().toISOString();
}

function sha256Hex(s) {
  return createHash('sha256').update(s, 'utf8').digest('hex');
}

async function buildPageItem(path) {
  const raw = await readFile(path, 'utf8');
  const parsed = toml.parse(raw);
  const id = required(parsed.id, `${path}: missing id`);
  const now = nowIso();
  // Re-derive sha256 of the body so cross-checks (DDB vs local) match.
  // The Amplify Brush/Template models declared `bodyToml` as a String
  // column carrying the raw TOML; nothing in the schema requires it to
  // be re-formatted server-side.
  return {
    id,
    name: parsed.name ?? 'Untitled',
    description: parsed.description ?? '',
    category: parsed.category ?? '',
    visibility: 'PUBLIC',
    bodyToml: raw,
    assets: null,
    forkedFrom: null,
    forkCount: 0,
    viewCount: 0,
    updatedAtSort: now,
    createdAt: now,
    updatedAt: now,
    __typename: 'PageTemplate',
    bodySha256: sha256Hex(raw),
  };
}

async function buildNotebookItem(path) {
  const raw = await readFile(path, 'utf8');
  const parsed = toml.parse(raw);
  const id = required(parsed.id, `${path}: missing id`);
  const now = nowIso();
  return {
    id,
    name: parsed.name ?? 'Untitled',
    description: parsed.description ?? '',
    visibility: 'PUBLIC',
    bodyToml: raw,
    forkedFrom: null,
    forkCount: 0,
    viewCount: 0,
    updatedAtSort: now,
    createdAt: now,
    updatedAt: now,
    __typename: 'NotebookTemplate',
    bodySha256: sha256Hex(raw),
  };
}

function required(v, msg) {
  if (v == null || v === '') {
    throw new Error(msg);
  }
  return v;
}

await main();
