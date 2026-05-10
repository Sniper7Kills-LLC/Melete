import { type ClientSchema, a, defineData } from '@aws-amplify/backend';
import { assetPresign } from '../functions/asset-presign/resource';
import { syncStrokesBatch } from '../functions/sync-strokes-batch/resource';

const Visibility = a.enum(['PRIVATE', 'UNLISTED', 'PUBLIC']);
const SavedKind = a.enum(['PageTemplate', 'NotebookTemplate', 'Brush', 'Notebook']);

const schema = a.schema({
  // `updatedAtSort` is an RFC3339 string mirror of `updatedAt`, used as the GSI
  // sort key so byVisibility/byOwner/byCategory return rows ordered by recency.
  // Amplify Gen 2 won't accept the auto-managed `updatedAt` field as a sortKey,
  // so we maintain this explicit field on every write. Clients (Rust + seed-publish
  // CLI) must set it on create/update; fork* and publish* resolvers set it server-side.
  PageTemplate: a
    .model({
      id: a.id().required(),
      owner: a.string(),
      name: a.string().required(),
      description: a.string(),
      category: a.string(),
      visibility: a.ref('Visibility').required(),
      bodyToml: a.string().required(),
      assets: a.json(),
      forkedFrom: a.id(),
      forkCount: a.integer().default(0),
      viewCount: a.integer().default(0),
      updatedAtSort: a.string().required(),
    })
    .secondaryIndexes((index) => [
      index('visibility').sortKeys(['updatedAtSort']).queryField('listPageTemplatesByVisibility'),
      index('owner').sortKeys(['updatedAtSort']).queryField('listPageTemplatesByOwner'),
      index('category').sortKeys(['updatedAtSort']).queryField('listPageTemplatesByCategory'),
    ])
    .authorization((allow) => [
      // identityClaim('sub') stores owner as the bare Cognito sub. The
      // default `<sub>::<username>` compound is what listByOwner({owner: sub})
      // can't match, so this keeps queries straightforward.
      allow.owner().identityClaim('sub'),
      allow.publicApiKey().to(['read']),
      allow.authenticated().to(['read']),
    ]),

  NotebookTemplate: a
    .model({
      id: a.id().required(),
      owner: a.string(),
      name: a.string().required(),
      description: a.string(),
      visibility: a.ref('Visibility').required(),
      bodyToml: a.string().required(),
      forkedFrom: a.id(),
      forkCount: a.integer().default(0),
      viewCount: a.integer().default(0),
      updatedAtSort: a.string().required(),
    })
    .secondaryIndexes((index) => [
      index('visibility').sortKeys(['updatedAtSort']).queryField('listNotebookTemplatesByVisibility'),
      index('owner').sortKeys(['updatedAtSort']).queryField('listNotebookTemplatesByOwner'),
    ])
    .authorization((allow) => [
      // identityClaim('sub') stores owner as the bare Cognito sub. The
      // default `<sub>::<username>` compound is what listByOwner({owner: sub})
      // can't match, so this keeps queries straightforward.
      allow.owner().identityClaim('sub'),
      allow.publicApiKey().to(['read']),
      allow.authenticated().to(['read']),
    ]),

  Brush: a
    .model({
      id: a.id().required(),
      owner: a.string(),
      name: a.string().required(),
      description: a.string(),
      visibility: a.ref('Visibility').required(),
      bodyToml: a.string().required(),
      forkedFrom: a.id(),
      forkCount: a.integer().default(0),
      viewCount: a.integer().default(0),
      updatedAtSort: a.string().required(),
    })
    .secondaryIndexes((index) => [
      index('visibility').sortKeys(['updatedAtSort']).queryField('listBrushesByVisibility'),
      index('owner').sortKeys(['updatedAtSort']).queryField('listBrushesByOwner'),
    ])
    .authorization((allow) => [
      // identityClaim('sub') stores owner as the bare Cognito sub. The
      // default `<sub>::<username>` compound is what listByOwner({owner: sub})
      // can't match, so this keeps queries straightforward.
      allow.owner().identityClaim('sub'),
      allow.publicApiKey().to(['read']),
      allow.authenticated().to(['read']),
    ]),

  // User notebook (the actual document, not a template). Holds just
  // the notebook header — sections / pages / strokes live in their
  // own per-row DDB models below so the SQLite data mirrors row-by-
  // row to the cloud (no S3 binary blobs). `kindJson` carries the
  // serde-tagged `NotebookKind` enum (Standard | Planner) verbatim
  // so the desktop deserializer reads it back without a translator.
  Notebook: a
    .model({
      id: a.id().required(),
      owner: a.string(),
      name: a.string().required(),
      description: a.string(),
      visibility: a.ref('Visibility').required(),
      kindJson: a.string(),
      assignedTemplatesJson: a.string(),
      updatedAtSort: a.string().required(),
    })
    .secondaryIndexes((index) => [
      index('visibility').sortKeys(['updatedAtSort']).queryField('listNotebooksByVisibility'),
      index('owner').sortKeys(['updatedAtSort']).queryField('listNotebooksByOwner'),
    ])
    .authorization((allow) => [
      allow.owner().identityClaim('sub'),
      allow.publicApiKey().to(['read']),
      // Pre-paywall: any signed-in user can read/update/delete every
      // notebook row. Eraser was hitting "Not Authorized" on
      // deleteRemoteStroke because owner-only delete failed when the
      // caller's JWT sub didn't match (token churn during testing).
      // Tighten back to owner-only when paid plans land (#44).
      allow.authenticated().to(['read', 'update', 'delete']),
    ]),

  // Mirror of the desktop's `sections` SQLite table. One row per
  // local section. `parentSectionId` is null for root sections.
  // `byNotebook` GSI feeds the web viewer's section list.
  RemoteSection: a
    .model({
      id: a.id().required(),
      owner: a.string(),
      notebookId: a.id().required(),
      parentSectionId: a.id(),
      name: a.string().required(),
      position: a.integer().required(),
      allowedTemplatesJson: a.string(),
    })
    .secondaryIndexes((index) => [
      index('notebookId').sortKeys(['position']).queryField('listRemoteSectionsByNotebook'),
    ])
    .authorization((allow) => [
      allow.owner().identityClaim('sub'),
      allow.publicApiKey().to(['read']),
      // Pre-paywall: any signed-in user can read/update/delete every
      // notebook row. Eraser was hitting "Not Authorized" on
      // deleteRemoteStroke because owner-only delete failed when the
      // caller's JWT sub didn't match (token churn during testing).
      // Tighten back to owner-only when paid plans land (#44).
      allow.authenticated().to(['read', 'update', 'delete']),
    ]),

  // Mirror of `pages`. JSON columns hold the serde-tagged structs
  // verbatim so the desktop round-trips losslessly.
  RemotePage: a
    .model({
      id: a.id().required(),
      owner: a.string(),
      notebookId: a.id().required(),
      sectionId: a.id().required(),
      templateId: a.id(),
      position: a.integer().required(),
      name: a.string(),
      plannerAddressJson: a.string(),
      widgetOverridesJson: a.string(),
      widgetDataJson: a.string(),
      flagged: a.boolean(),
      createdAtIso: a.string(),
      modifiedAtIso: a.string(),
    })
    .secondaryIndexes((index) => [
      index('notebookId').sortKeys(['position']).queryField('listRemotePagesByNotebook'),
      index('sectionId').sortKeys(['position']).queryField('listRemotePagesBySection'),
    ])
    .authorization((allow) => [
      allow.owner().identityClaim('sub'),
      allow.publicApiKey().to(['read']),
      // Pre-paywall: any signed-in user can read/update/delete every
      // notebook row. Eraser was hitting "Not Authorized" on
      // deleteRemoteStroke because owner-only delete failed when the
      // caller's JWT sub didn't match (token churn during testing).
      // Tighten back to owner-only when paid plans land (#44).
      allow.authenticated().to(['read', 'update', 'delete']),
    ]),

  // One row per stroke. Body is the JSON-serialized
  // `journal_core::Stroke` (matches the desktop's serde shape so the
  // web viewer reads it directly). AppSync auto-generates
  // `onCreateRemoteStroke` / `observeQuery` for live subscribers.
  RemoteStroke: a
    .model({
      id: a.id().required(),
      owner: a.string(),
      notebookId: a.id().required(),
      pageId: a.id().required(),
      strokeJson: a.string().required(),
      // Last-writer-wins clock. Every mutation bumps `updatedAtIso`
      // so subscribers can ignore out-of-order events. Soft-delete
      // sets `deletedAtIso`; the row stays in the table forever as a
      // tombstone (web filters `deletedAtIso IS NULL`). Hard delete
      // never happens — racing creates / updates can't bring a
      // soft-deleted stroke back without a newer `updatedAtIso`.
      createdAt: a.string().required(),
      updatedAtIso: a.string(),
      deletedAtIso: a.string(),
    })
    .secondaryIndexes((index) => [
      index('notebookId').sortKeys(['createdAt']).queryField('listRemoteStrokesByNotebook'),
      index('pageId').sortKeys(['createdAt']).queryField('listRemoteStrokesByPage'),
    ])
    .authorization((allow) => [
      allow.owner().identityClaim('sub'),
      allow.publicApiKey().to(['read']),
      // Pre-paywall: any signed-in user can read/update/delete every
      // notebook row. Eraser was hitting "Not Authorized" on
      // deleteRemoteStroke because owner-only delete failed when the
      // caller's JWT sub didn't match (token churn during testing).
      // Tighten back to owner-only when paid plans land (#44).
      allow.authenticated().to(['read', 'update', 'delete']),
    ]),

  // Owner-scoped reference to a source PageTemplate / NotebookTemplate /
  // Brush row. Holds no body — the desktop / web client fetches the
  // current bodyToml from the source on demand, so updates propagate
  // automatically. "Fork" still copies; "Save" subscribes.
  SavedTemplate: a
    .model({
      id: a.id().required(),
      owner: a.string(),
      kind: a.ref('SavedKind').required(),
      sourceId: a.id().required(),
      // Cached display fields written at save time so /my can render
      // the list without N extra fetches. Refreshed when the client
      // re-saves; staleness is acceptable for the list view.
      sourceName: a.string(),
      savedAt: a.string().required(),
    })
    .secondaryIndexes((index) => [
      index('owner').sortKeys(['savedAt']).queryField('listSavedTemplatesByOwner'),
    ])
    .authorization((allow) => [allow.owner().identityClaim('sub')]),

  Visibility,
  SavedKind,

  // Two-step pipeline: load source row + authorize → put new owner-scoped
  // copy. Single-resolver shapes can't read-then-write in one DynamoDB
  // operation, so the previous single-file resolvers (kept for git
  // history) returned a synthesized newRow without persisting it.
  forkPageTemplate: a
    .mutation()
    .arguments({ id: a.id().required() })
    .returns(a.ref('PageTemplate'))
    .authorization((allow) => [allow.authenticated()])
    .handler([
      a.handler.custom({
        entry: './fork-page-template-load.js',
        dataSource: a.ref('PageTemplate'),
      }),
      a.handler.custom({
        entry: './fork-page-template-write.js',
        dataSource: a.ref('PageTemplate'),
      }),
    ]),

  forkNotebookTemplate: a
    .mutation()
    .arguments({ id: a.id().required() })
    .returns(a.ref('NotebookTemplate'))
    .authorization((allow) => [allow.authenticated()])
    .handler([
      a.handler.custom({
        entry: './fork-notebook-template-load.js',
        dataSource: a.ref('NotebookTemplate'),
      }),
      a.handler.custom({
        entry: './fork-notebook-template-write.js',
        dataSource: a.ref('NotebookTemplate'),
      }),
    ]),

  forkBrush: a
    .mutation()
    .arguments({ id: a.id().required() })
    .returns(a.ref('Brush'))
    .authorization((allow) => [allow.authenticated()])
    .handler([
      a.handler.custom({
        entry: './fork-brush-load.js',
        dataSource: a.ref('Brush'),
      }),
      a.handler.custom({
        entry: './fork-brush-write.js',
        dataSource: a.ref('Brush'),
      }),
    ]),

  // Two-step pipeline (same shape + reason as forkPageTemplate above).
  publishPageTemplate: a
    .mutation()
    .arguments({ id: a.id().required(), visibility: a.ref('Visibility').required() })
    .returns(a.ref('PageTemplate'))
    .authorization((allow) => [allow.authenticated()])
    .handler([
      a.handler.custom({
        entry: './publish-page-template-load.js',
        dataSource: a.ref('PageTemplate'),
      }),
      a.handler.custom({
        entry: './publish-page-template-write.js',
        dataSource: a.ref('PageTemplate'),
      }),
    ]),

  publishNotebookTemplate: a
    .mutation()
    .arguments({ id: a.id().required(), visibility: a.ref('Visibility').required() })
    .returns(a.ref('NotebookTemplate'))
    .authorization((allow) => [allow.authenticated()])
    .handler([
      a.handler.custom({
        entry: './publish-notebook-template-load.js',
        dataSource: a.ref('NotebookTemplate'),
      }),
      a.handler.custom({
        entry: './publish-notebook-template-write.js',
        dataSource: a.ref('NotebookTemplate'),
      }),
    ]),

  publishBrush: a
    .mutation()
    .arguments({ id: a.id().required(), visibility: a.ref('Visibility').required() })
    .returns(a.ref('Brush'))
    .authorization((allow) => [allow.authenticated()])
    .handler([
      a.handler.custom({
        entry: './publish-brush-load.js',
        dataSource: a.ref('Brush'),
      }),
      a.handler.custom({
        entry: './publish-brush-write.js',
        dataSource: a.ref('Brush'),
      }),
    ]),

  getAssetUploadUrl: a
    .mutation()
    .arguments({
      templateId: a.id().required(),
      sha256: a.string().required(),
      contentType: a.string().required(),
      sizeBytes: a.integer().required(),
    })
    .returns(
      a.customType({
        uploadUrl: a.string().required(),
        s3Key: a.string().required(),
      }),
    )
    .authorization((allow) => [allow.authenticated()])
    // Lambda validates args + mints a presigned PUT URL whose key
    // is bound to the caller's `cognito-identity.amazonaws.com:sub`.
    // Owner-of-templateId is NOT checked here — the storage policy
    // (`protected/{entity_id}/*`) physically prevents one user from
    // writing into another user's prefix, so a misclaimed templateId
    // only fills the caller's own quota. We avoided a JS-pipeline
    // owner check because Amplify Gen 2 doesn't allow mixing JS +
    // Lambda handlers on the same field, and giving the Lambda DDB
    // read access on PageTemplate re-introduces the CFN circular
    // dependency between the data + function nested stacks.
    .handler(a.handler.function(assetPresign)),

  // Bulk CRUD for the desktop's stroke worker — replaces N
  // round-trip per-stroke create/delete mutations with a single
  // BatchWriteItem chunked at 25 ops. Lambda env is wired in
  // `amplify/backend.ts` to the RemoteStroke table name.
  // Single upsert path — `items` is an array of full stroke states
  // (id, pageId, strokeJson, updatedAtIso, deletedAtIso). Lambda
  // writes each as a PutRequest in BatchWriteItem chunks. Cloud
  // last-writer-wins on `updatedAtIso`. There is no separate delete
  // mutation; tombstones are upserts with `deletedAtIso` set.
  upsertStrokesBatch: a
    .mutation()
    .arguments({
      notebookId: a.id().required(),
      items: a.json().required(),
    })
    .returns(a.ref('StrokesBatchResult'))
    .authorization((allow) => [
      allow.authenticated(),
      allow.publicApiKey(),
    ])
    .handler(a.handler.function(syncStrokesBatch)),

  StrokesBatchResult: a.customType({
    notebookId: a.id().required(),
    upserted: a.integer().required(),
    unprocessed: a.integer().required(),
    /** Affected ids — fan-out to subscribers as a single payload. */
    ids: a.string().array(),
    /** Ids the worker should requeue. Stale-skipped ids are omitted —
     *  those are correct LWW losses, requeue would loop forever. */
    failedIds: a.string().array(),
  }),

  // Fan-out subscription: every successful syncStrokesBatch call
  // re-emits its (createdIds, deletedIds) tuple to all subscribers
  // for the given notebook. Replaces the `onCreate/onDelete` per-row
  // subscriptions that the auto-resolvers used to fire (Lambda
  // BatchWriteItem bypasses AppSync, so those don't fire for
  // batched ops).
  onStrokesBatchSync: a
    .subscription()
    .for(a.ref('upsertStrokesBatch'))
    .arguments({ notebookId: a.id().required() })
    .authorization((allow) => [
      allow.authenticated(),
      allow.publicApiKey(),
    ])
    .handler(
      a.handler.custom({
        entry: './on-strokes-batch-sync.js',
        // NoneDataSource — AppSync passes the source mutation's
        // return value through to subscribers without touching
        // backing storage.
        dataSource: 'NONE_DS',
      }),
    ),
});

export type Schema = ClientSchema<typeof schema>;

export const data = defineData({
  schema,
  authorizationModes: {
    defaultAuthorizationMode: 'userPool',
    apiKeyAuthorizationMode: {
      expiresInDays: 30,
    },
  },
});
