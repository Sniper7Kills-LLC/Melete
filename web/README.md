# Melete — Web Portal

Vite + React + TypeScript SPA that hosts:

- **Viewer / Designer / Templeter / Tooler** — WASM-backed POC views.
  `Designer` authors page templates, `Templeter` authors notebook
  templates, `Tooler` authors brushes. WASM bridge falls back to a
  mock until `bash build-wasm.sh` is run.
- **Gallery** — anonymous browse of public page templates, notebook
  templates, and brushes from the Amplify Gen 2 backend, served via
  AppSync API-key auth mode. Falls back to a hardcoded sample set
  when the backend is in stub mode.
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
  `isStubBackend === true`, and Gallery / My render a "Backend not
  configured" banner. Gallery additionally falls back to a hardcoded
  sample list so the UI is still useful for local dev.

This means **the web build always succeeds** even on a fresh checkout
without AWS credentials. You just won't get live data until you run the
sandbox.

## Auth

`web/src/pages/My.tsx` uses `<Authenticator>` from
`@aws-amplify/ui-react` with email login. The email + Sign Out are
shown in the page header once authenticated.

The Amplify Data client (`web/src/amplify-client.ts`) is shared. It
mirrors the **public read shape** of the three models locally instead
of importing `Schema` from `amplify/data/resource.ts` — the Amplify
backend deps aren't installed under `web/node_modules`, and pulling
them in just to typecheck the web bundle would entangle the two
builds.

Auth modes used:

- **Gallery**: `authMode: 'apiKey'` for `*.list({ filter: { visibility: { eq: 'PUBLIC' } } })`.
- **My**: default `userPool` for `*.listByOwner({ owner: <sub> })`.

## Routes

| Path          | Component             | Auth          | Purpose                                                                                                |
| ------------- | --------------------- | ------------- | ------------------------------------------------------------------------------------------------------ |
| `/`           | `pages/Viewer.tsx`    | none          | Loads `public/sample-notebook.json` into WASM viewer.                                                  |
| `/designer`   | `pages/Designer.tsx`  | none          | Drag-drop page-template designer.                                                                      |
| `/templeter`  | `pages/Templeter.tsx` | none          | Notebook-template (planner-structure) designer.                                                        |
| `/tooler`     | `pages/Tooler.tsx`    | none          | Brush composition designer.                                                                            |
| `/gallery`    | `pages/Gallery.tsx`   | API key       | Anonymous browse of `visibility = PUBLIC` rows for page templates, notebook templates, and brushes.    |
| `/public`     | (redirect)            | —             | Legacy alias — redirects to `/gallery`.                                                                |
| `/my`         | `pages/My.tsx`        | Cognito (UI)  | Signed-in user's own templates and brushes.                                                            |

## Layout

```
src/
  main.tsx                    # router + nav shell + Amplify.configure()
  amplify-config.ts           # outputs loader (real → stub fallback)
  amplify-outputs.stub.json   # fallback config when sandbox not running
  amplify-client.ts           # shared generateClient<Schema>() instance + row types
  index.css                   # tailwind imports
  types/                      # TS mirrors of melete-core / melete-templates types
    index.ts
    brush.ts
    notebook-template.ts
    example-templates.ts      # hardcoded fallback library used by Gallery in stub mode
  wasm/
    index.ts                  # Viewer / Shim interfaces + lazy real-WASM loader + mocks
    generated/                # gitignored — wasm-bindgen output from build-wasm.sh
  store/
    designerStore.ts          # zustand store + undo/redo for Designer
    unitsStore.ts             # mm/in toggle persisted to localStorage
  pages/
    Viewer.tsx
    Designer.tsx
    Templeter.tsx
    Tooler.tsx
    Gallery.tsx
    My.tsx
  components/
    WidgetPalette.tsx         # used by Designer
    DesignSurface.tsx         # used by Designer
    PropertyPanel.tsx         # used by Designer
    SaveModal.tsx             # used by Designer
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
  parseBrushToml(toml: string): Brush;
  serializeBrushToml(b: Brush): string;
}
```

The TS types in `src/types` are byte-faithful to `crates/melete-core` —
field names, casing, and discriminator tags all match what `serde`
emits.

## What's intentionally still missing

- Fork / Edit / Publish actions on Gallery + My rows (issues #54, #55).
- Notebook-template TOML round-trip in `melete-web-shim` — the schema
  exists but the wasm bindings only cover page templates and brushes,
  so Gallery's notebook tab cannot render schematics from live
  `bodyToml` yet.
- Web viewer should fetch published page-template assets from S3 (#56).
- Amplify Hosting CI configuration. Code is ready for it; deployment
  gating is on the maintainer.
