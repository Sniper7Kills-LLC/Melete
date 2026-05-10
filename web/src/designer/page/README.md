# Page-template designer (placeholder)

This directory is reserved for the Amplify-backed page-template designer
that lands as part of issue #10. The earlier WASM-only designer at
`src/pages/Designer.tsx` will be folded into this dir or replaced by it.

## Entry contract (proposed)

The designer is mounted at `/designer/page/:templateId?` and exports a
single React component:

```ts
export interface PageDesignerProps {
  // Existing template id when editing; undefined when creating fresh.
  templateId?: string;

  // Owner sub of the signed-in user. Required — designer is auth-gated.
  ownerSub: string;
}

export function PageDesigner(props: PageDesignerProps): JSX.Element;
```

## Persistence

On save, the designer:

1. Serializes the current state to TOML via the WASM `Shim.serializeTemplateToml`.
2. If `templateId` is present: `client.models.PageTemplate.update({ id, bodyToml, ... })`.
3. If creating: `client.models.PageTemplate.create({ name, bodyToml, visibility: 'PRIVATE', ... })`.
4. For asset uploads (background images, etc.), call the
   `getAssetUploadUrl` mutation, PUT the bytes to the returned presigned
   URL, then patch the template row's `assets` field with the resulting
   `s3Key`.

## Auth mode

All writes use `userPool` (the schema default). Read of the editing row
can use the same.

## What's NOT in scope here

- Marketplace listing UI (issue #8).
- Notebook-template designer — see the sibling `notebook/` dir.
