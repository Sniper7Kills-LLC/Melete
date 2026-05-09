import { type ClientSchema, a, defineData } from '@aws-amplify/backend';
import { assetPresign } from '../functions/asset-presign/resource';

const Visibility = a.enum(['PRIVATE', 'UNLISTED', 'PUBLIC']);

const schema = a.schema({
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
    })
    .secondaryIndexes((index) => [
      index('visibility').queryField('listPageTemplatesByVisibility'),
      index('owner').queryField('listPageTemplatesByOwner'),
      index('category').queryField('listPageTemplatesByCategory'),
    ])
    .authorization((allow) => [
      allow.owner(),
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
    })
    .secondaryIndexes((index) => [
      index('visibility').queryField('listNotebookTemplatesByVisibility'),
      index('owner').queryField('listNotebookTemplatesByOwner'),
    ])
    .authorization((allow) => [
      allow.owner(),
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
    })
    .secondaryIndexes((index) => [
      index('visibility').queryField('listBrushesByVisibility'),
      index('owner').queryField('listBrushesByOwner'),
    ])
    .authorization((allow) => [
      allow.owner(),
      allow.publicApiKey().to(['read']),
      allow.authenticated().to(['read']),
    ]),

  Visibility,

  forkPageTemplate: a
    .mutation()
    .arguments({ id: a.id().required() })
    .returns(a.ref('PageTemplate'))
    .authorization((allow) => [allow.authenticated()])
    .handler(a.handler.custom({ entry: './fork-page-template.ts', dataSource: a.ref('PageTemplate') })),

  forkNotebookTemplate: a
    .mutation()
    .arguments({ id: a.id().required() })
    .returns(a.ref('NotebookTemplate'))
    .authorization((allow) => [allow.authenticated()])
    .handler(a.handler.custom({ entry: './fork-notebook-template.ts', dataSource: a.ref('NotebookTemplate') })),

  forkBrush: a
    .mutation()
    .arguments({ id: a.id().required() })
    .returns(a.ref('Brush'))
    .authorization((allow) => [allow.authenticated()])
    .handler(a.handler.custom({ entry: './fork-brush.ts', dataSource: a.ref('Brush') })),

  publishPageTemplate: a
    .mutation()
    .arguments({ id: a.id().required(), visibility: a.ref('Visibility').required() })
    .returns(a.ref('PageTemplate'))
    .authorization((allow) => [allow.authenticated()])
    .handler(a.handler.custom({ entry: './publish-page-template.ts', dataSource: a.ref('PageTemplate') })),

  publishNotebookTemplate: a
    .mutation()
    .arguments({ id: a.id().required(), visibility: a.ref('Visibility').required() })
    .returns(a.ref('NotebookTemplate'))
    .authorization((allow) => [allow.authenticated()])
    .handler(a.handler.custom({ entry: './publish-notebook-template.ts', dataSource: a.ref('NotebookTemplate') })),

  publishBrush: a
    .mutation()
    .arguments({ id: a.id().required(), visibility: a.ref('Visibility').required() })
    .returns(a.ref('Brush'))
    .authorization((allow) => [allow.authenticated()])
    .handler(a.handler.custom({ entry: './publish-brush.ts', dataSource: a.ref('Brush') })),

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
    .handler(a.handler.function(assetPresign)),
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
