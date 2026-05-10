// Helpers for fetching published template assets from S3.
//
// Published page-template assets live at
//   `s3://<bucket>/public/templates/<templateId>/<sha256>`
// per `amplify/storage/resource.ts` (the `public/templates/*` rule
// grants guest+authenticated read). The bucket name + region come
// from `amplify_outputs.json` → `storage`.
//
// The SPA never PUTs to this prefix — uploads go through the
// `getAssetUploadUrl` Lambda + a presigned URL. This module is read-only
// by design.

import { amplifyOutputs } from './amplify-config';

interface AssetMetaWire {
  name: string;
  mime: string;
  sha256: string;
  size: number;
}

/** Per-asset metadata as returned by AppSync's `assets` JSON column. */
export type AssetMeta = AssetMetaWire;

/**
 * Normalize the loosely-typed `assets` value AppSync returns. The
 * column is `a.json()` — depending on backend transport it can come
 * back as an array, a JSON-encoded string, null, or undefined.
 */
export function normalizeAssets(raw: unknown): AssetMeta[] {
  if (!raw) return [];
  if (typeof raw === 'string') {
    if (!raw) return [];
    try {
      const parsed = JSON.parse(raw);
      return normalizeAssets(parsed);
    } catch {
      return [];
    }
  }
  if (!Array.isArray(raw)) return [];
  const out: AssetMeta[] = [];
  for (const item of raw) {
    if (!item || typeof item !== 'object') continue;
    const o = item as Record<string, unknown>;
    if (
      typeof o.name === 'string' &&
      typeof o.mime === 'string' &&
      typeof o.sha256 === 'string' &&
      typeof o.size === 'number'
    ) {
      out.push({
        name: o.name,
        mime: o.mime,
        sha256: o.sha256,
        size: o.size,
      });
    }
  }
  return out;
}

interface StorageOutputs {
  bucket_name?: string;
  aws_region?: string;
}

function storage(): StorageOutputs | null {
  // amplify_outputs has a top-level `storage` block when storage is
  // configured — see amplify_outputs.json.
  const o = amplifyOutputs as unknown as { storage?: StorageOutputs };
  return o.storage ?? null;
}

/**
 * Public URL for an asset in the `public/templates/<templateId>/` prefix.
 * Returns null when storage isn't configured (stub mode).
 */
export function publicAssetUrl(
  templateId: string,
  sha256: string,
): string | null {
  const s = storage();
  if (!s?.bucket_name || !s.aws_region) return null;
  // Path-style URL hits the regional S3 endpoint directly. We don't
  // route through CloudFront because the Amplify Gen 2 storage block
  // doesn't provision a CDN distribution by default.
  // The bucket is bound to allow guest GET on `public/templates/*` so
  // unauthenticated visitors can fetch.
  return `https://${s.bucket_name}.s3.${s.aws_region}.amazonaws.com/public/templates/${encodeURIComponent(templateId)}/${encodeURIComponent(sha256)}`;
}

/**
 * Look up an asset by file name (e.g. the path the desktop wrote into
 * `Background::Image{path}`) and return its public URL, or null if no
 * matching asset is uploaded.
 */
export function publicAssetUrlByName(
  templateId: string,
  name: string,
  assets: AssetMeta[],
): string | null {
  // Background paths can be filesystem paths on the desktop side —
  // strip everything before the last `/` so a published asset name
  // like `bg.png` matches a desktop path like `~/img/bg.png`.
  const basename = name.split('/').pop() ?? name;
  const hit = assets.find((a) => a.name === basename || a.name === name);
  if (!hit) return null;
  return publicAssetUrl(templateId, hit.sha256);
}
