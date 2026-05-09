# Journal — Web POC

Static SPA scaffold for the Journal web portal (see `../docs/web-portal.md`).
This proves out the **viewer** and **page-template designer** UI before
the WASM bindings to `journal-canvas` / `journal-templates` exist.

> **Status.** The WASM API in `src/wasm/index.ts` now points at the
> real `wasm-bindgen` output produced by `bash build-wasm.sh`. If that
> output is missing the bridges fall back to the original mock impls
> (visible in the same file) so the SPA keeps booting without WASM —
> useful while a contributor is iterating on the TS / React side and
> doesn't want to rebuild Rust on every change.

## Prereqs

```bash
# One-time:
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.120  # match wasm-bindgen crate
```

## Run

```bash
# 1. Build the WASM crates and emit wasm-bindgen JS shims
#    (output → web/src/wasm/generated/{shim,viewer}/)
bash web/build-wasm.sh

# 2. SPA dev / build
cd web
pnpm install   # or `npm install` — works with either
pnpm dev       # vite dev server on :5173
pnpm typecheck # tsc --noEmit
pnpm build     # vite build → dist/
```

## Routes

| Path        | Component                          | Purpose                                                                       |
| ----------- | ---------------------------------- | ----------------------------------------------------------------------------- |
| `/`         | `pages/Viewer.tsx`                 | Loads `public/sample-notebook.json` and hands it to the (mock) WASM viewer.   |
| `/designer` | `pages/Designer.tsx`               | Drag-drop page-template designer; emits TOML through `Shim.serializeTemplateToml`. |

## Layout

```
src/
  main.tsx           # router + nav shell
  index.css          # tailwind imports
  types/index.ts     # TS mirrors of journal-core types
  wasm/index.ts      # Viewer / Shim interfaces + mock impls
  store/designerStore.ts   # zustand store + undo/redo
  pages/
    Viewer.tsx
    Designer.tsx
  components/
    WidgetPalette.tsx
    DesignSurface.tsx
    PropertyPanel.tsx
    SaveModal.tsx
public/
  sample-notebook.json     # NotebookBundle envelope (see web-portal.md §5.4)
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

## What's intentionally missing

Anything from `docs/web-portal.md` mentioning AWS, Cognito, AppSync,
DynamoDB, S3, Lambda, fork, share, or QR. Those land in later phases;
this scaffold is the static frontend only.
