# Journal — Web Portal

Vite + React + TypeScript SPA that hosts:

- **Viewer / Designer / Templeter / Tooler / Gallery** — the original
  WASM-backed POC views (mocked WASM bridge until `bash build-wasm.sh`
  is run).
- **Public** — anonymous browse of public page templates, notebook
  templates, and brushes from the Amplify Gen 2 backend, served via
  AppSync API-key auth mode.
- **My** — authenticated view of the signed-in user's own templates,
  served via Cognito User Pool JWT.

## Prereqs

```bash
# One-time WASM toolchain (only needed for Viewer / Designer / etc.):
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.120  # match wasm-bindgen crate
```

## Run

```bash
# 1. (Optional) Build the WASM crates for the viewer / designer paths:
bash web/build-wasm.sh

# 2. Install JS deps:
cd web
npm install

# 3. Dev server:
npm run dev          # vite on :5173

# 4. Type-check + production build:
npm run typecheck    # tsc --noEmit
npm run build        # tsc + vite build → dist/
```

## Amplify backend wiring

The Amplify Gen 2 backend (under `amplify/` at the repo root) emits
`amplify_outputs.json` to the **repo root** when you run either:

```bash
# Personal sandbox (per-developer stack):
npx ampx sandbox

# CI / Hosting deploy:
npx ampx pipeline-deploy --branch main --app-id <YOUR_APP_ID>
```

`amplify_outputs.json` is **gitignored** (per Amplify Gen 2
convention) — every contributor needs their own.

`web/src/amplify-config.ts` reads the file via Vite's `import.meta.glob`
trick, so:

- Real outputs present at repo root → `isStubBackend === false`, live
  network calls go to AppSync.
- No file present → falls back to `web/src/amplify-outputs.stub.json`,
  `isStubBackend === true`, and the Public/My pages render a "Backend
  not configured" banner.

This means **the web build always succeeds** even on a fresh checkout
without AWS credentials. You just won't get live data until you run the
sandbox.

## Auth

`web/src/pages/My.tsx` uses `<Authenticator>` from
`@aws-amplify/ui-react` with email login. The email + Sign Out are
shown in the page header once authenticated.

The Amplify Data client (`web/src/amplify-client.ts`) is shared — its
`Schema` type comes from `amplify/data/resource.ts` so all GraphQL calls
are fully typed.

Auth modes used:

- **Public page**: `authMode: 'apiKey'` for `*.list({ filter: { visibility: { eq: 'PUBLIC' } } })`.
- **My page**: default `userPool` for `*.listByOwner({ owner: <sub> })`.

## Routes

| Path                  | Component               | Auth          | Purpose                                                       |
| --------------------- | ----------------------- | ------------- | ------------------------------------------------------------- |
| `/`                   | `pages/Viewer.tsx`      | none          | (existing) Loads `public/sample-notebook.json` into WASM viewer. |
| `/designer`           | `pages/Designer.tsx`    | none          | (existing) Drag-drop page-template designer (mock WASM).      |
| `/templeter`          | `pages/Templeter.tsx`   | none          | (existing) Templete editor UI scaffolding.                    |
| `/tooler`             | `pages/Tooler.tsx`      | none          | (existing) Tool/brush editor UI scaffolding.                  |
| `/gallery`            | `pages/Gallery.tsx`     | none          | (existing) Static gallery preview.                            |
| `/public`             | `pages/Public.tsx`      | API key       | **NEW.** Anonymous browse of `visibility = PUBLIC` rows for page templates, notebook templates, and brushes. |
| `/my`                 | `pages/My.tsx`          | Cognito (UI)  | **NEW.** Signed-in user's own templates and brushes.          |

Designer placeholders (empty for now) live at:

- `src/designer/page/README.md` — entry contract for the future Amplify-backed page-template designer (#10).
- `src/designer/notebook/README.md` — entry contract for the notebook-template designer (#11).

## Layout

```
src/
  main.tsx                    # router + nav shell + Amplify.configure()
  amplify-config.ts           # outputs loader (real → stub fallback)
  amplify-outputs.stub.json   # fallback config when sandbox not running
  amplify-client.ts           # shared generateClient<Schema>() instance
  index.css                   # tailwind imports
  types/index.ts              # TS mirrors of journal-core types
  wasm/index.ts               # Viewer / Shim interfaces + mock impls
  store/designerStore.ts      # zustand store + undo/redo
  designer/
    page/README.md            # placeholder for #10
    notebook/README.md        # placeholder for #11
  pages/
    Viewer.tsx
    Designer.tsx
    Templeter.tsx
    Tooler.tsx
    Gallery.tsx
    Public.tsx                # NEW
    My.tsx                    # NEW
  components/
    WidgetPalette.tsx
    DesignSurface.tsx
    PropertyPanel.tsx
    SaveModal.tsx
public/
  sample-notebook.json        # NotebookBundle envelope (see web-portal.md §5.4)
```

## WASM contract

When the real WASM build lands, replace the body of `src/wasm/index.ts`
(or the singletons it exports) with the wasm-bindgen-generated module.
The interface to honor:

```ts
interface Viewer {
  init(canvas: HTMLCanvasElement): Promise<void>;
  loadNotebook(bytes: Uint8Array): Promise<void>;
  renderPage(index: number, w: number, h: number): void;
  pan(dx: number, dy: number): void;
  zoomAt(sx: number, sy: number, factor: number): void;
}

interface Shim {
  parseTemplateToml(toml: string): PageTemplate;
  serializeTemplateToml(t: PageTemplate): string;
}
```

The TS types in `src/types` are byte-faithful to `crates/journal-core` —
field names, casing, and discriminator tags all match what `serde`
emits. The WASM agent should not need to invent any new shape.

## What's intentionally still missing

- Marketplace UI (issue #8) — fork buttons, search, categories.
- The Amplify-aware page + notebook designers (issues #10 / #11).
- Real asset thumbnail rendering — current Public list shows id +
  name + description only; no image previews.
- Amplify Hosting CI configuration. Code is ready for it; deployment
  gating is on the maintainer.
