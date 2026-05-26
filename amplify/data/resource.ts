import { type ClientSchema, a, defineData } from '@aws-amplify/backend';
import { assetPresign } from '../functions/asset-presign/resource';
import { syncStrokesBatch } from '../functions/sync-strokes-batch/resource';
import { stripeCheckout } from '../functions/stripe-checkout/resource';
import { stripePortal } from '../functions/stripe-portal/resource';
import { adminSearchUsers } from '../functions/admin-search-users/resource';
import { adminMutate } from '../functions/admin-mutate/resource';

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

  // Per-user subscription + resolved feature caps. PK `id` == Cognito sub.
  // Written by the Stripe webhook Lambda + admin mutations only; clients
  // read their own row to render usage meters and gate features. The
  // resolved caps are tier defaults (from TierConfig) merged with addon
  // purchases and admin overrides — the desktop never has to know the
  // formula, just the final numbers. `historyDays` is reserved for the
  // future history feature; v1 always sets it to 0.
  UserEntitlement: a
    .model({
      id: a.id().required(),
      owner: a.string(),
      tier: a.string().required(),
      status: a.string().required(),
      stripeCustomerId: a.string(),
      stripeSubscriptionId: a.string(),
      periodEnd: a.string(),
      trialEndsAt: a.string(),
      educationVerified: a.boolean().default(false),
      notebookCap: a.integer().required(),
      strokesPerPageCap: a.integer().required(),
      strokesPerNotebookCap: a.integer().required(),
      dailyWriteCap: a.integer().required(),
      s3BytesCap: a.integer().required(),
      templatePublishCap: a.integer().required(),
      historyDays: a.integer().default(0),
      liveSyncEnabled: a.boolean().default(false),
      addonsJson: a.json(),
      capOverridesJson: a.json(),
      compedBy: a.string(),
      updatedAtSort: a.string().required(),
    })
    .authorization((allow) => [
      allow.owner().identityClaim('sub').to(['read']),
      allow.groups(['admin', 'superadmin']).to(['read']),
    ]),

  // Per-tier default caps. Editable in DDB to retune without redeploying.
  // Read-public so the desktop pricing page can render plan options
  // without hardcoding numbers. v1 seed values written by a one-shot
  // bootstrap Lambda; admin can update later.
  TierConfig: a
    .model({
      id: a.id().required(),
      notebookCap: a.integer().required(),
      strokesPerPageCap: a.integer().required(),
      strokesPerNotebookCap: a.integer().required(),
      dailyWriteCap: a.integer().required(),
      s3BytesCap: a.integer().required(),
      templatePublishCap: a.integer().required(),
      historyDays: a.integer().default(0),
      liveSyncEnabled: a.boolean().default(false),
      priceMonthlyCents: a.integer(),
      priceYearlyCents: a.integer(),
      stripePriceIdMonthly: a.string(),
      stripePriceIdYearly: a.string(),
    })
    .authorization((allow) => [
      allow.authenticated().to(['read']),
      allow.publicApiKey().to(['read']),
      allow.groups(['superadmin']).to(['read', 'create', 'update', 'delete']),
    ]),

  // Daily write counters used by the check-quota resolver pipeline step.
  // `id` is `{userId}#{YYYY-MM-DD}` so the resolver can compute the row
  // key from the caller's sub + today's date and increment atomically.
  // `ttl` is a UNIX epoch column wired to DDB's TTL feature in the CDK
  // override (14 days after creation) so stale rows auto-prune.
  UserDailyUsage: a
    .model({
      id: a.id().required(),
      owner: a.string(),
      userId: a.string().required(),
      date: a.string().required(),
      strokeWrites: a.integer().default(0),
      mutationCount: a.integer().default(0),
      subMessages: a.integer().default(0),
      ttl: a.integer(),
    })
    .secondaryIndexes((index) => [
      index('userId').sortKeys(['date']).queryField('listUserDailyUsage'),
    ])
    .authorization((allow) => [
      allow.owner().identityClaim('sub').to(['read']),
      allow.groups(['admin', 'superadmin']).to(['read']),
    ]),

  // Admin-portal aggregate row. Singleton (PK = "global"), maintained
  // by DDB-stream Lambdas on UserEntitlement + Notebook so the admin
  // dashboard renders without scanning either table. Backed by atomic
  // counters incremented/decremented as user lifecycle events fire.
  AdminStats: a
    .model({
      id: a.id().required(),
      totalUsers: a.integer().default(0),
      freeUsers: a.integer().default(0),
      proUsers: a.integer().default(0),
      studioUsers: a.integer().default(0),
      trialingUsers: a.integer().default(0),
      pastDueUsers: a.integer().default(0),
      canceledUsers: a.integer().default(0),
      totalNotebooks: a.integer().default(0),
      mrrCents: a.integer().default(0),
      lastUpdatedIso: a.string(),
    })
    .authorization((allow) => [
      allow.groups(['admin', 'superadmin']).to(['read']),
    ]),

  // Append-only audit trail for every superadmin write mutation.
  // PK = yearMonth ("2026-05"), SK = timestamp#actionId so the
  // admin panel paginates by month. Every entry carries the
  // adminUserId / targetUserId pair and a required `reason` blob
  // (enforced by the mutation resolver).
  AdminAuditLog: a
    .model({
      id: a.id().required(),
      yearMonth: a.string().required(),
      timestampIso: a.string().required(),
      adminUserId: a.string().required(),
      adminEmail: a.string(),
      action: a.string().required(),
      targetUserId: a.string(),
      targetEmail: a.string(),
      beforeJson: a.string(),
      afterJson: a.string(),
      reason: a.string().required(),
      ipAddress: a.string(),
    })
    .secondaryIndexes((index) => [
      index('yearMonth')
        .sortKeys(['timestampIso'])
        .queryField('listAdminAuditLogByMonth'),
    ])
    .authorization((allow) => [
      allow.groups(['admin', 'superadmin']).to(['read']),
    ]),

  // Top-N rankings for the admin dashboard's "biggest users" panel.
  // Computed nightly by an aggregator Lambda — admin reads are pure
  // table-scan-free Query-on-GSI. `metric` ∈ {strokes, notebooks,
  // s3_bytes}.
  AdminTopUsers: a
    .model({
      id: a.id().required(),
      metric: a.string().required(),
      rank: a.integer().required(),
      userId: a.string().required(),
      email: a.string(),
      value: a.integer().required(),
      refreshedAtIso: a.string().required(),
    })
    .secondaryIndexes((index) => [
      index('metric')
        .sortKeys(['rank'])
        .queryField('listAdminTopUsersByMetric'),
    ])
    .authorization((allow) => [
      allow.groups(['admin', 'superadmin']).to(['read']),
    ]),

  // User-submitted feedback (#42). Created by any caller — auth or
  // public API key — so anonymous bug reports work too. Read-restricted
  // to admins; the maintainer triages from the admin portal. No SES
  // wiring yet; that's a follow-up under #42 once volume justifies an
  // email pipeline. PK is auto id; sourceApp ∈ {"web","desktop"}.
  Feedback: a
    .model({
      id: a.id().required(),
      owner: a.string(),
      sourceApp: a.string().required(),
      version: a.string(),
      severity: a.string().required(),
      message: a.string().required(),
      contactEmail: a.string(),
      userAgent: a.string(),
      createdAtIso: a.string().required(),
      triagedAtIso: a.string(),
    })
    .secondaryIndexes((index) => [
      index('sourceApp')
        .sortKeys(['createdAtIso'])
        .queryField('listFeedbackBySource'),
    ])
    .authorization((allow) => [
      allow.publicApiKey().to(['create']),
      allow.authenticated().to(['create']),
      allow.groups(['admin', 'superadmin']).to(['read', 'update']),
    ]),

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

  // Stripe Checkout Session URL. Authenticated user picks a tier +
  // interval; Lambda mints a hosted Checkout URL pre-stamped with the
  // caller's Cognito sub so the webhook (`stripe-webhook`) can project
  // the resulting subscription onto a UserEntitlement row.
  createCheckoutSession: a
    .mutation()
    .arguments({
      tier: a.string().required(),
      interval: a.string().required(),
    })
    .returns(
      a.customType({
        url: a.string().required(),
      }),
    )
    .authorization((allow) => [allow.authenticated()])
    .handler(a.handler.function(stripeCheckout)),

  // Admin search by email. UserEntitlement holds no email column —
  // Cognito owns identity attributes — so the admin panel's search
  // box runs through this Lambda. Auth checked twice: at the
  // resolver via `@aws_auth` group rule (Amplify Gen 2 doesn't yet
  // expose that directly on a custom mutation, so the schema
  // restricts to authenticated() and the Lambda re-checks group
  // membership from the JWT claims).
  AdminUserSummary: a.customType({
    userId: a.string().required(),
    email: a.string().required(),
    enabled: a.boolean().required(),
    status: a.string().required(),
    createdAtIso: a.string(),
  }),
  AdminSearchUsersResult: a.customType({
    items: a.ref('AdminUserSummary').array().required(),
  }),
  adminSearchUsers: a
    .mutation()
    .arguments({ email: a.string().required() })
    .returns(a.ref('AdminSearchUsersResult'))
    .authorization((allow) => [
      allow.groups(['admin', 'superadmin']),
    ])
    .handler(a.handler.function(adminSearchUsers)),

  // Single dispatcher for every superadmin write. `action` ∈
  // {grantTier, setStatus, markEducation, resetDailyUsage,
  // setEntitlementCaps, disableUser, deleteUser, extendTrial}.
  // Per-action payload travels as AWSJSON. Every call writes an
  // AdminAuditLog row carrying before/after snapshots and the
  // mandatory `reason` blob.
  adminMutate: a
    .mutation()
    .arguments({
      action: a.string().required(),
      targetUserId: a.string().required(),
      payload: a.json(),
      reason: a.string().required(),
    })
    .returns(
      a.customType({
        after: a.string(),
      }),
    )
    .authorization((allow) => [allow.groups(['superadmin'])])
    .handler(a.handler.function(adminMutate)),

  // Stripe Customer Portal URL for self-service plan management.
  // Reads the caller's `stripeCustomerId` from UserEntitlement and
  // returns the hosted portal URL. Throws NO_STRIPE_CUSTOMER if the
  // caller has never subscribed; the client routes those to Checkout.
  createPortalSession: a
    .mutation()
    .arguments({})
    .returns(
      a.customType({
        url: a.string().required(),
      }),
    )
    .authorization((allow) => [allow.authenticated()])
    .handler(a.handler.function(stripePortal)),

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
      // "snapshot" = explicit manual save (bypasses daily-write cap
      // but still counts toward usage). "live" or omitted = streaming
      // live sync, both counted and capped.
      kind: a.string(),
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
