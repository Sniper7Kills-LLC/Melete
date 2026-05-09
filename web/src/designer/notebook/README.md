# Notebook-template designer (placeholder)

This directory is reserved for the Amplify-backed notebook-template
designer that lands as part of issue #11.

A "notebook template" is a planner-structure spec — what pages should
auto-generate for year/month/week/day periods, plus any shared section
layout. See `crates/journal-templates/src/notebook_template.rs` and
`docs/web-portal.md` for the canonical schema.

## Entry contract (proposed)

Mounted at `/designer/notebook/:templateId?` and exports:

```ts
export interface NotebookDesignerProps {
  templateId?: string;
  ownerSub: string;
}

export function NotebookDesigner(props: NotebookDesignerProps): JSX.Element;
```

## Persistence

On save: `client.models.NotebookTemplate.{create|update}` with `bodyToml`
serialized from the in-memory designer state.

## Auth mode

`userPool` for all writes; same for reads of the editing row.

## What's NOT in scope here

- The page-template designer — see the sibling `page/` dir.
- The marketplace UI — issue #8.
- Drag-and-drop calendar/section authoring polish — that's iterative
  work after the basic create/edit flow is wired.
