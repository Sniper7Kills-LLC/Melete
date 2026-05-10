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
  /**
   * Per-asset metadata for any image / PDF backgrounds the template
   * references. Wire shape from AppSync's `a.json()` column — may
   * arrive as an array, a JSON-encoded string, or null. Use
   * `normalizeAssets` from `@/amplify-storage` to coerce.
   */
  assets?: unknown;
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

export type SavedKind = 'PageTemplate' | 'NotebookTemplate' | 'Brush';

export interface SavedTemplateRow {
  id: string;
  owner?: string | null;
  kind: SavedKind;
  sourceId: string;
  sourceName?: string | null;
  savedAt: string;
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
type GetArgs = { id: string };
type GetOpts = { authMode?: AuthMode };
interface GetResult<T> {
  data: T | null;
  errors?: { message: string }[];
}

type DeleteArgs = { id: string };
type DeleteOpts = { authMode?: AuthMode };

type CreateOpts = { authMode?: AuthMode };
type UpdateOpts = { authMode?: AuthMode };

interface PageTemplateCreateInput {
  id: string;
  name: string;
  description?: string | null;
  category?: string | null;
  visibility: Visibility;
  bodyToml: string;
  assets?: unknown;
  forkedFrom?: string | null;
  updatedAtSort: string;
}

interface PageTemplateUpdateInput {
  id: string;
  name?: string;
  description?: string | null;
  category?: string | null;
  visibility?: Visibility;
  bodyToml?: string;
  assets?: unknown;
  updatedAtSort: string;
}

interface NotebookTemplateCreateInput {
  id: string;
  name: string;
  description?: string | null;
  visibility: Visibility;
  bodyToml: string;
  forkedFrom?: string | null;
  updatedAtSort: string;
}

interface NotebookTemplateUpdateInput {
  id: string;
  name?: string;
  description?: string | null;
  visibility?: Visibility;
  bodyToml?: string;
  updatedAtSort: string;
}

interface BrushCreateInput {
  id: string;
  name: string;
  description?: string | null;
  visibility: Visibility;
  bodyToml: string;
  forkedFrom?: string | null;
  updatedAtSort: string;
}

interface BrushUpdateInput {
  id: string;
  name?: string;
  description?: string | null;
  visibility?: Visibility;
  bodyToml?: string;
  updatedAtSort: string;
}

interface PageTemplateOps {
  list(opts?: ListOpts<PageTemplateRow>): Promise<ListResult<PageTemplateRow>>;
  listPageTemplatesByOwner(
    args: ByOwnerArgs,
    opts?: ByOwnerOpts,
  ): Promise<ListResult<PageTemplateRow>>;
  get(args: GetArgs, opts?: GetOpts): Promise<GetResult<PageTemplateRow>>;
  create(
    input: PageTemplateCreateInput,
    opts?: CreateOpts,
  ): Promise<GetResult<PageTemplateRow>>;
  update(
    input: PageTemplateUpdateInput,
    opts?: UpdateOpts,
  ): Promise<GetResult<PageTemplateRow>>;
  delete(
    args: DeleteArgs,
    opts?: DeleteOpts,
  ): Promise<GetResult<PageTemplateRow>>;
}

interface NotebookTemplateOps {
  list(
    opts?: ListOpts<NotebookTemplateRow>,
  ): Promise<ListResult<NotebookTemplateRow>>;
  listNotebookTemplatesByOwner(
    args: ByOwnerArgs,
    opts?: ByOwnerOpts,
  ): Promise<ListResult<NotebookTemplateRow>>;
  get(
    args: GetArgs,
    opts?: GetOpts,
  ): Promise<GetResult<NotebookTemplateRow>>;
  create(
    input: NotebookTemplateCreateInput,
    opts?: CreateOpts,
  ): Promise<GetResult<NotebookTemplateRow>>;
  update(
    input: NotebookTemplateUpdateInput,
    opts?: UpdateOpts,
  ): Promise<GetResult<NotebookTemplateRow>>;
  delete(
    args: DeleteArgs,
    opts?: DeleteOpts,
  ): Promise<GetResult<NotebookTemplateRow>>;
}

interface BrushOps {
  list(opts?: ListOpts<BrushRow>): Promise<ListResult<BrushRow>>;
  listBrushesByOwner(
    args: ByOwnerArgs,
    opts?: ByOwnerOpts,
  ): Promise<ListResult<BrushRow>>;
  get(args: GetArgs, opts?: GetOpts): Promise<GetResult<BrushRow>>;
  create(
    input: BrushCreateInput,
    opts?: CreateOpts,
  ): Promise<GetResult<BrushRow>>;
  update(
    input: BrushUpdateInput,
    opts?: UpdateOpts,
  ): Promise<GetResult<BrushRow>>;
  delete(args: DeleteArgs, opts?: DeleteOpts): Promise<GetResult<BrushRow>>;
}

interface SavedCreateInput {
  kind: SavedKind;
  sourceId: string;
  sourceName?: string | null;
  savedAt: string;
}

interface SavedTemplateOps {
  list(
    opts?: ListOpts<SavedTemplateRow>,
  ): Promise<ListResult<SavedTemplateRow>>;
  listSavedTemplatesByOwner(
    args: ByOwnerArgs,
    opts?: ByOwnerOpts,
  ): Promise<ListResult<SavedTemplateRow>>;
  create(
    input: SavedCreateInput,
    opts?: GetOpts,
  ): Promise<GetResult<SavedTemplateRow>>;
  delete(
    args: DeleteArgs,
    opts?: DeleteOpts,
  ): Promise<GetResult<SavedTemplateRow>>;
}

interface MutationResult<T> {
  data: T | null;
  errors?: { message: string }[];
}

interface PublishArgs {
  id: string;
  visibility: Visibility;
}

interface ForkArgs {
  id: string;
}

interface MutationOps {
  publishPageTemplate(args: PublishArgs): Promise<MutationResult<PageTemplateRow>>;
  publishNotebookTemplate(
    args: PublishArgs,
  ): Promise<MutationResult<NotebookTemplateRow>>;
  publishBrush(args: PublishArgs): Promise<MutationResult<BrushRow>>;
  forkPageTemplate(args: ForkArgs): Promise<MutationResult<PageTemplateRow>>;
  forkNotebookTemplate(
    args: ForkArgs,
  ): Promise<MutationResult<NotebookTemplateRow>>;
  forkBrush(args: ForkArgs): Promise<MutationResult<BrushRow>>;
}

interface AmplifyDataClient {
  models: {
    PageTemplate: PageTemplateOps;
    NotebookTemplate: NotebookTemplateOps;
    Brush: BrushOps;
    SavedTemplate: SavedTemplateOps;
  };
  mutations: MutationOps;
}

// eslint-disable-next-line @typescript-eslint/no-explicit-any -- intentional, see comment above
export const client = generateClient<any>() as unknown as AmplifyDataClient;
