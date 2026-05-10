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

export type SavedKind =
  | 'PageTemplate'
  | 'NotebookTemplate'
  | 'Brush'
  | 'Notebook';

export interface NotebookRow {
  id: string;
  owner?: string | null;
  name: string;
  description?: string | null;
  visibility: Visibility;
  /** JSON-encoded `journal_core::NotebookKind` (Standard | Planner). */
  kindJson?: string | null;
  /** JSON-encoded `Vec<TemplateId>` for the assignedTemplates list. */
  assignedTemplatesJson?: string | null;
  updatedAtSort: string;
  createdAt?: string | null;
  updatedAt?: string | null;
}

export interface RemoteSectionRow {
  id: string;
  owner?: string | null;
  notebookId: string;
  parentSectionId?: string | null;
  name: string;
  position: number;
  allowedTemplatesJson?: string | null;
}

export interface RemotePageRow {
  id: string;
  owner?: string | null;
  notebookId: string;
  sectionId: string;
  templateId?: string | null;
  position: number;
  name?: string | null;
  plannerAddressJson?: string | null;
  widgetOverridesJson?: string | null;
  widgetDataJson?: string | null;
  flagged?: boolean | null;
  createdAtIso?: string | null;
  modifiedAtIso?: string | null;
}

export interface RemoteStrokeRow {
  id: string;
  owner?: string | null;
  notebookId: string;
  pageId: string;
  /** JSON-encoded `journal_core::Stroke`. */
  strokeJson: string;
  createdAt: string;
  /** LWW clock — every mutation bumps this. */
  updatedAtIso?: string | null;
  /** Set when the stroke is soft-deleted; readers filter on this. */
  deletedAtIso?: string | null;
}

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

interface UpsertStrokesBatchInput {
  notebookId: string;
  /** AWSJSON — stringified array of UpsertItem on the wire. */
  items: string;
}
interface StrokesBatchResult {
  notebookId: string;
  upserted: number;
  unprocessed: number;
  ids?: string[] | null;
}
interface SubscribeStrokesBatchArgs {
  notebookId: string;
}

interface MutationOps {
  upsertStrokesBatch(
    args: UpsertStrokesBatchInput,
    opts?: { authMode?: AuthMode },
  ): Promise<MutationResult<StrokesBatchResult>>;
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

interface NotebookUpdateInput {
  id: string;
  name?: string;
  description?: string | null;
  visibility?: Visibility;
  kindJson?: string | null;
  assignedTemplatesJson?: string | null;
  updatedAtSort: string;
}

interface NotebookOps {
  list(opts?: ListOpts<NotebookRow>): Promise<ListResult<NotebookRow>>;
  listNotebooksByOwner(
    args: ByOwnerArgs,
    opts?: ByOwnerOpts,
  ): Promise<ListResult<NotebookRow>>;
  get(args: GetArgs, opts?: GetOpts): Promise<GetResult<NotebookRow>>;
  update(
    input: NotebookUpdateInput,
    opts?: UpdateOpts,
  ): Promise<GetResult<NotebookRow>>;
  delete(
    args: DeleteArgs,
    opts?: DeleteOpts,
  ): Promise<GetResult<NotebookRow>>;
}

interface ByNotebookArgs {
  notebookId: string;
}
interface ByNotebookOpts {
  authMode?: AuthMode;
  limit?: number;
}
interface ByPageArgs {
  pageId: string;
}

interface ObserveSubscription {
  unsubscribe(): void;
}
interface ObserveOpts<T> {
  filter?: Partial<Record<keyof T, { eq?: unknown }>>;
  authMode?: AuthMode;
}

interface RemoteSectionOps {
  listRemoteSectionsByNotebook(
    args: ByNotebookArgs,
    opts?: ByNotebookOpts,
  ): Promise<ListResult<RemoteSectionRow>>;
}
interface RemotePageOps {
  listRemotePagesByNotebook(
    args: ByNotebookArgs,
    opts?: ByNotebookOpts,
  ): Promise<ListResult<RemotePageRow>>;
}

interface RemoteStrokeOps {
  listRemoteStrokesByNotebook(
    args: ByNotebookArgs,
    opts?: ByNotebookOpts,
  ): Promise<ListResult<RemoteStrokeRow>>;
  listRemoteStrokesByPage(
    args: ByPageArgs,
    opts?: ByNotebookOpts,
  ): Promise<ListResult<RemoteStrokeRow>>;
  /**
   * Default Amplify-generated `list` with filter/limit/authMode. Used
   * for the `id IN (...)` lookup after `onStrokesBatchSync` fires —
   * we only want the rows whose ids the event carried, not the whole
   * notebook. The filter shape is permissive (`unknown`) because the
   * `in` operator on the auto-generated type isn't surfaced in our
   * hand-rolled interface yet.
   */
  list(opts?: {
    filter?: { id?: { in?: unknown[]; eq?: unknown } };
    limit?: number;
    authMode?: AuthMode;
  }): Promise<ListResult<RemoteStrokeRow>>;
  observeQuery(opts: ObserveOpts<RemoteStrokeRow>): {
    subscribe(handler: {
      next?: (snap: { items: RemoteStrokeRow[] }) => void;
      error?: (e: unknown) => void;
    }): ObserveSubscription;
  };
  /**
   * AppSync auto-generated `onCreate{Model}` subscription. Emits a
   * single row per server-side create. Filter is optional but using
   * one keeps the wire chatter scoped to a single notebook.
   */
  onCreate(opts: ObserveOpts<RemoteStrokeRow>): {
    subscribe(handler: {
      next?: (item: RemoteStrokeRow) => void;
      error?: (e: unknown) => void;
    }): ObserveSubscription;
  };
  onUpdate(opts: ObserveOpts<RemoteStrokeRow>): {
    subscribe(handler: {
      next?: (item: RemoteStrokeRow) => void;
      error?: (e: unknown) => void;
    }): ObserveSubscription;
  };
  /**
   * `onDelete` emits the deleted row's id (other fields may be null
   * — only the key columns are guaranteed to be populated by AppSync).
   */
  onDelete(opts: ObserveOpts<RemoteStrokeRow>): {
    subscribe(handler: {
      next?: (item: RemoteStrokeRow) => void;
      error?: (e: unknown) => void;
    }): ObserveSubscription;
  };
}

interface SubscriptionsOps {
  /**
   * Custom subscription wired in `amplify/data/resource.ts` to fan
   * out the Lambda-backed `syncStrokesBatch` mutation's affected
   * id lists. Replaces the per-row onCreate/onDelete subscriptions
   * for batched ops (BatchWriteItem bypasses AppSync subscriptions
   * so we route via this instead).
   */
  onStrokesBatchSync(args: SubscribeStrokesBatchArgs, opts?: { authMode?: AuthMode }): {
    subscribe(handler: {
      next?: (item: StrokesBatchResult) => void;
      error?: (e: unknown) => void;
    }): { unsubscribe(): void };
  };
}

interface AmplifyDataClient {
  models: {
    PageTemplate: PageTemplateOps;
    NotebookTemplate: NotebookTemplateOps;
    Brush: BrushOps;
    SavedTemplate: SavedTemplateOps;
    Notebook: NotebookOps;
    RemoteSection: RemoteSectionOps;
    RemotePage: RemotePageOps;
    RemoteStroke: RemoteStrokeOps;
  };
  mutations: MutationOps;
  subscriptions: SubscriptionsOps;
}

// eslint-disable-next-line @typescript-eslint/no-explicit-any -- intentional, see comment above
export const client = generateClient<any>() as unknown as AmplifyDataClient;
