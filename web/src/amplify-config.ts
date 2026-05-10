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

// Vite's `import.meta.glob` only sees files inside the project root
// (web/), so we resolve through a `web/amplify_outputs.json` symlink
// that points at the repo-root file. The symlink is created by the
// dev script (or by hand: `ln -sf ../amplify_outputs.json
// web/amplify_outputs.json`) and is gitignored. Vite returns an
// empty record when the file is absent, so the bundle still builds
// in fresh checkouts without a deployed sandbox.
const realOutputsModules = import.meta.glob<{ default: unknown }>(
  '../amplify_outputs.json',
  { eager: true },
);

const realOutputsKey = Object.keys(realOutputsModules)[0];
const realOutputs = realOutputsKey
  ? (realOutputsModules[realOutputsKey]!.default as typeof stub)
  : null;

export const amplifyOutputs = realOutputs ?? stub;
export const isStubBackend = realOutputs === null;
