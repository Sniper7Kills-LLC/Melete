// Single shared Amplify Data client.
//
// We deliberately do NOT import the canonical `Schema` type from
// `amplify/data/resource.ts`. That file lives at the repo root next to the
// `amplify/` Gen 2 backend, and its imports (`@aws-amplify/backend`,
// `aws-cdk-lib`, etc.) are not installed under `web/node_modules`. Pulling
// them in just to typecheck the web bundle would bloat `web/`'s install
// and entangle the two builds.
//
// Instead we mirror the **public read shape** of the three models (the only
// thing the web/ portal touches) below. If the schema gains fields we want
// to surface here, mirror them. The wire shape is enforced server-side
// regardless — these types are purely for client-code ergonomics.
//
// `amplify_outputs.json` (or its stub) must be configured into the Amplify
// runtime before this client is used; that happens in `main.tsx` via
// `Amplify.configure(amplifyOutputs)`.

import { generateClient } from 'aws-amplify/api';

export type Visibility = 'PRIVATE' | 'UNLISTED' | 'PUBLIC';

export interface PageTemplateRow {
  id: string;
  owner?: string | null;
  name: string;
  description?: string | null;
  category?: string | null;
  visibility: Visibility;
  bodyToml: string;
  forkedFrom?: string | null;
  forkCount?: number | null;
  viewCount?: number | null;
  updatedAtSort: string;
  createdAt?: string | null;
  updatedAt?: string | null;
}

export interface NotebookTemplateRow {
  id: string;
  owner?: string | null;
  name: string;
  description?: string | null;
  visibility: Visibility;
  bodyToml: string;
  forkedFrom?: string | null;
  forkCount?: number | null;
  viewCount?: number | null;
  updatedAtSort: string;
  createdAt?: string | null;
  updatedAt?: string | null;
}

export interface BrushRow {
  id: string;
  owner?: string | null;
  name: string;
  description?: string | null;
  visibility: Visibility;
  bodyToml: string;
  forkedFrom?: string | null;
  forkCount?: number | null;
  viewCount?: number | null;
  updatedAtSort: string;
  createdAt?: string | null;
  updatedAt?: string | null;
}

// Loosely-typed client. Amplify's `generateClient<Schema>()` would give
// per-model autocompletion; we trade that for build-isolation from the
// `amplify/` workspace.
//
// At runtime, `client.models.<Name>.list({...})` and the per-index query
// methods (`listPageTemplatesByOwner`, etc.) are dynamically dispatched —
// AppSync resolvers ignore the static client type entirely.
//
interface ListResult<T> {
  data: T[] | null;
  errors?: { message: string }[];
}

type AuthMode = 'apiKey' | 'userPool' | 'iam' | 'oidc' | 'lambda';

interface ListOpts<T> {
  filter?: Partial<Record<keyof T, { eq?: unknown; contains?: string }>>;
  authMode?: AuthMode;
  limit?: number;
  nextToken?: string | null;
}

type ByOwnerOpts = { authMode?: AuthMode; limit?: number };
type ByOwnerArgs = { owner: string };

interface PageTemplateOps {
  list(opts?: ListOpts<PageTemplateRow>): Promise<ListResult<PageTemplateRow>>;
  listPageTemplatesByOwner(
    args: ByOwnerArgs,
    opts?: ByOwnerOpts,
  ): Promise<ListResult<PageTemplateRow>>;
}

interface NotebookTemplateOps {
  list(
    opts?: ListOpts<NotebookTemplateRow>,
  ): Promise<ListResult<NotebookTemplateRow>>;
  listNotebookTemplatesByOwner(
    args: ByOwnerArgs,
    opts?: ByOwnerOpts,
  ): Promise<ListResult<NotebookTemplateRow>>;
}

interface BrushOps {
  list(opts?: ListOpts<BrushRow>): Promise<ListResult<BrushRow>>;
  listBrushesByOwner(
    args: ByOwnerArgs,
    opts?: ByOwnerOpts,
  ): Promise<ListResult<BrushRow>>;
}

interface AmplifyDataClient {
  models: {
    PageTemplate: PageTemplateOps;
    NotebookTemplate: NotebookTemplateOps;
    Brush: BrushOps;
  };
}

// eslint-disable-next-line @typescript-eslint/no-explicit-any -- intentional, see comment above
export const client = generateClient<any>() as unknown as AmplifyDataClient;
