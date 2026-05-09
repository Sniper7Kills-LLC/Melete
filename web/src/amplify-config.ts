// Amplify Gen 2 outputs loader.
//
// Real `amplify_outputs.json` is emitted at the repo root by `ampx sandbox`
// or `ampx pipeline-deploy`. It is gitignored (per Amplify Gen 2 convention),
// so a fresh checkout of this repo will not have it. We fall back to a stub
// shipped alongside this file so the build still typechecks + runs locally.
//
// Runtime behavior:
//   - If the real outputs are present, `isStubBackend` is false and Amplify
//     calls go to the real AppSync endpoint.
//   - If the stub is in use, `isStubBackend` is true. Pages should show a
//     "Backend not configured" banner and skip live network calls.
//
// We deliberately attempt the real import first via Vite's static-glob trick
// so the build cleanly drops the stub when real outputs exist.

import stub from './amplify-outputs.stub.json';

// Vite's `import.meta.glob` resolves at build time. The pattern points to
// the repo-root file. Vite returns an empty record when the file is absent,
// so the bundle still builds in fresh checkouts.
const realOutputsModules = import.meta.glob<{ default: unknown }>(
  '../../../amplify_outputs.json',
  { eager: true },
);

const realOutputsKey = Object.keys(realOutputsModules)[0];
const realOutputs = realOutputsKey
  ? (realOutputsModules[realOutputsKey]!.default as typeof stub)
  : null;

export const amplifyOutputs = realOutputs ?? stub;
export const isStubBackend = realOutputs === null;
