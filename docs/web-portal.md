# Change Document: Amplify Web Portal — Template Marketplace, Designer, View-Only Viewer

**Status:** Draft
**Owner:** S7K
**Date:** 2026-05-03
**Scope:** Web portal hosted on AWS Amplify Hosting that (a) lets the community publish/browse/fork page and notebook templates, (b) hosts a drag-and-drop designer that emits the same TOML the desktop consumes, and (c) renders shared notebooks read-only via a WASM build of `melete-canvas`.
**Companion docs:** [`docs/renderer-vello-migration.md`](renderer-vello-migration.md) — Vello migration is **Done** as of 2026-05-03; the canvas, background, widget, and overlay code paths are already Vello-only on desktop. The web viewer can compile against the same crates without first re-doing renderer work.

**Building blocks (post-migration):**
- `melete-canvas` — Vello scene builder + wgpu offscreen renderer. Pulls in `gtk4::cairo` only when its `pdf` feature is enabled (used by `pdf_export`); the web viewer builds it with `--no-default-features --features vello` to stay GTK-free.
- `melete-widgets` — Vello+`parley` widget rendering. No GTK / SQLite / poppler in its closure; web-importable as-is.
- `melete-core`, `melete-templates` — pure data types + TOML serde, already WASM-friendly.

These three crates plus `vello` + `wgpu` are everything the WASM viewer needs to render byte-identical strokes/widgets/backgrounds on the web.

---

## 1. Goals

1. Stand up `melete-app.example` (or similar — domain TBD) on AWS Amplify Hosting.
2. **Template marketplace:** browse/search/preview public page templates and notebook templates, fork to a personal library, fork to your own logged-in account.
3. **Designer:** drag-and-drop editor for both page templates and notebook templates. The output is byte-for-byte the same TOML the desktop loads — no new schema, no parallel format.
4. **View-only viewer:** open a shared notebook by short URL or QR code; render strokes pixel-identical to the desktop using the same `melete-canvas` Rust code compiled to WASM.
5. **Auth:** Cognito-backed sign-up/sign-in (email + Google federated). Anonymous browsing of public content allowed; account required to publish, fork, or share.
6. **Cost discipline:** all-static frontend, AppSync GraphQL pay-per-request, DynamoDB on-demand, S3 standard. Target ≤$15/month at low traffic (<10k requests/day).

## 2. Non-Goals

- Drawing/annotation in the browser. Strokes are still authored in the native app. The viewer is read-only by design.
- Real-time multi-user editing. Out of scope; if it ever ships, separate doc.
- Mobile-native apps. The viewer is responsive web only.
- Importing third-party formats (rnote, Notability, OneNote .one). Out of scope.
- Offline portal. The portal is online-only; the desktop app is the offline story.
- JS rewrite of stroke rendering. The renderer is and stays Rust (compiled to WASM); see §5.4.

## 3. Current State

| Concern | Today |
|---|---|
| Web presence | None. |
| Sharing | None. Templates live in `~/.local/share/melete/templates/*.toml` per user. |
| Auth | None. Single-user local app. |
| Public templates | None. Built-in templates (`melete-templates::builtin`) ship with the binary. |
| Notebook export to web | None. PDF export exists (`pdf_export.rs`) but is desktop-side. |
| Renderer | Cairo on `gtk4::DrawingArea`. Not portable. Vello migration ([renderer-vello-migration.md](renderer-vello-migration.md)) is the prerequisite for a web viewer. |

## 4. Target State

| Concern | Target |
|---|---|
| Hosting | AWS Amplify Hosting (static + edge). Custom domain via Route 53. |
| Frontend stack | React + Vite + TypeScript. Minimal — view + designer pages, no SSR, no app framework on top. |
| Auth | Amazon Cognito User Pool. Hosted UI for federated sign-in (Google). Public content browsable signed-out. |
| API | AWS AppSync (GraphQL). One unified API for templates, notebooks-shared, user profiles. |
| Database | DynamoDB on-demand. One table per logical entity, single-region (us-east-1) for MVP. |
| Object storage | S3 buckets for: public template TOML bodies, public notebook export bodies, user-uploaded image assets used inside templates. |
| Designer output | The exact TOML schema in `crates/melete-templates/src/format.rs` (page templates) and the `NotebookTemplate` shape in `crates/melete-core/src/template.rs` extended with the same TOML wrapper. **No second schema.** |
| Viewer | `melete-canvas` compiled to `wasm32-unknown-unknown` + ~50 LOC TS wrapper. WebGPU primary, WebGL2 fallback via wgpu. |

## 5. Architecture

### 5.1 AWS resources

```
                          ┌────────────────────────────┐
                          │   Amplify Hosting (CDN)    │
                          │    React + Vite SPA        │
                          └──────────────┬─────────────┘
                                         │ HTTPS
       ┌──────────────────┬──────────────┼──────────────┬───────────────┐
       ▼                  ▼              ▼              ▼               ▼
  ┌─────────┐      ┌────────────┐  ┌──────────┐  ┌──────────┐    ┌─────────────┐
  │ Cognito │      │  AppSync   │  │ Cognito  │  │   S3     │    │  S3 (assets │
  │  User   │      │  GraphQL   │  │  Hosted  │  │ template │    │   inside    │
  │  Pool   │      │   API      │  │   UI     │  │ TOML     │    │  templates) │
  └─────────┘      └─────┬──────┘  └──────────┘  └──────────┘    └─────────────┘
                         │
              ┌──────────┴────────────┐
              ▼                       ▼
        ┌──────────┐            ┌──────────┐
        │ DynamoDB │            │ DynamoDB │
        │templates │            │ notebooks│
        │_public   │            │ _shared  │
        └──────────┘            └──────────┘
```

| Resource | Purpose |
|---|---|
| Amplify Hosting | Static SPA + CDN. CI deploy from `main` of a separate `melete-web` repo (or `web/` subdir). |
| Cognito User Pool | Email + Google sign-in. JWTs consumed by AppSync via `AMAZON_COGNITO_USER_POOLS` auth mode. |
| AppSync | GraphQL endpoint. Resolvers map directly to DynamoDB (VTL) and S3 (signed URLs via Lambda resolver). |
| DynamoDB `templates_public` | Catalog of published page+notebook templates. Metadata in DDB; TOML body in S3. |
| DynamoDB `templates_user` | Per-user *forks* and *drafts*. Body in S3 under user prefix. |
| DynamoDB `notebooks_shared` | Per-share-code notebook export pointer (notebook UUID, owner sub, S3 key, expiry, view count). |
| DynamoDB `users` | User profile (display name, avatar URL, fork count). PK = Cognito `sub`. |
| S3 `melete-public-templates` | Public template TOML bodies, addressed by template UUID. Public-read, write via signed PUT only. |
| S3 `melete-shared-notebooks` | Notebook export bundles for the viewer. Public-read with object-key obscurity (UUID v4); revocable via DynamoDB row + signed-URL expiry on the resource manifest. |
| S3 `melete-template-assets` | Image/PDF assets referenced by templates (`BackgroundType::Image { path }`). Path inside TOML rewritten to S3 key on upload. |

All buckets versioned. Public buckets have CloudFront in front (cache + custom domain).

### 5.2 Frontend stack

- **React 18 + Vite + TypeScript.** Two reasons over a heavier framework: (1) the portal is two pages — Marketplace and Designer — plus a viewer route. (2) The bundle hosts a WASM blob (~3 MB compressed for `melete-canvas`); minimizing JS overhead matters.
- **State:** TanStack Query for AppSync calls, Zustand for designer working-state. No Redux.
- **Routing:** React Router. Routes:
  - `/` — marketplace landing (search, browse, preview)
  - `/t/:id` — template detail (preview rendered live, fork button)
  - `/n/:id` — notebook share view (read-only viewer)
  - `/designer/page/:id?` — page-template designer
  - `/designer/notebook/:id?` — notebook-template designer
  - `/profile/:user` — user profile / their published templates
- **Styling:** Tailwind. No component library — small surface, custom styling is cheap.
- **Auth:** AWS Amplify JS library only for Cognito session handling. **Not** the full Amplify CLI scaffolding — we keep IaC in CDK (§5.7) so the frontend stays decoupled from generated code.

### 5.3 Designer architecture

The designer renders the same `PageTemplate` / `NotebookTemplate` types the desktop uses. The output of "Save" is `parse_template_toml`-compatible TOML.

```
┌──────────────────────────────────────────────────────────┐
│           React designer page (page or notebook)         │
│  ┌──────────┐   ┌──────────────────────────┐  ┌────────┐ │
│  │  Widget  │   │     Canvas (WASM)        │  │  Prop  │ │
│  │ palette  │   │                          │  │ panel  │ │
│  │          │   │  Vello-rendered live     │  │        │ │
│  │ ▢ Text   │ → │  preview of the          │ ←│ Width  │ │
│  │ ▢ Cal    │   │  template being edited   │  │ Height │ │
│  │ ▢ Grid   │   │                          │  │ Style  │ │
│  └──────────┘   └─────────┬────────────────┘  └────────┘ │
│                           │                              │
│                           ▼                              │
│              draft: PageTemplate (in-memory)             │
└──────────────────────────────────────────────────────────┘
                            │
                            │ "Save" → toml::to_string
                            ▼
              S3 PUT  +  AppSync createTemplate mutation
```

**Critical constraint:** the designer constructs in-memory values matching the Rust `PageTemplate` struct exactly. Serialization happens via a tiny Rust shim (also compiled to WASM) — `template_serialize(template_json: JsValue) -> String` — so we never re-implement the TOML serializer in TypeScript. One source of truth: `melete_templates::format`.

This means a new `melete-web-shim` crate (no GTK, no SQLite, no Cairo) wrapping the public types from `melete-core` + `melete-templates::format` for `wasm-bindgen` consumption. It links against the same `melete-canvas` (vello-only build) + `melete-widgets` that the viewer uses, so the designer's live preview is bit-for-bit identical to what the viewer renders.

#### Designer interactions

- Click-to-place from palette → adds a `TemplateWidget` at canvas centre, immediately selected.
- Drag handle on widget → moves; drag corners → resize; both update `WidgetRect` (mm).
- Property panel → edits `WidgetKind`-specific fields (font_size_mm, items list for Checklist, hour ranges for Timeline, etc.).
- Snap-to-mm-grid + snap-to-other-widget guides — same logic the desktop editor uses, ported to TS or compiled from Rust if simpler.
- Undo/redo via Zustand history middleware over the `PageTemplate` value.

The notebook-template designer is structurally similar: drag page-template chips into year/quarter/month/week/daily slots; weekday selector per daily slot.

### 5.4 Viewer architecture

```
URL  /n/:id
   ▼
React route fetches:
   • notebook manifest (DDB row → S3 key for body)
   • notebook export body from S3 (gzipped JSON: notebook + sections + pages
     + strokes + referenced page templates inline)
   ▼
Hand off to melete-canvas (WASM):
   wasm.init_renderer(canvas: HTMLCanvasElement)
   wasm.load_notebook(json_body: Uint8Array)
   wasm.render_page(page_index: u32)
   ▼
wgpu (WebGPU) draws the same scene the desktop app draws.
```

**WASM glue crate (`melete-web-viewer`):** wraps `melete-canvas::vello_renderer::VelloRenderer` + `melete-widgets::WidgetRenderer` for `wasm-bindgen` consumption. Public API mirrors what the GLArea on desktop calls today:

```rust
#[wasm_bindgen]
pub struct Viewer { /* VelloRenderer, WidgetRenderer, decoded bundle */ }

#[wasm_bindgen]
impl Viewer {
    pub async fn init(canvas: web_sys::HtmlCanvasElement) -> Result<Viewer, JsValue>;
    pub fn load_notebook(&mut self, body: &[u8]) -> Result<(), JsValue>;
    pub fn render_page(&mut self, page_index: u32, w: u32, h: u32);
    pub fn pan(&mut self, dx: f64, dy: f64);
    pub fn zoom_at(&mut self, sx: f64, sy: f64, factor: f64);
}
```

**Backend selection:** wgpu picks WebGPU when `navigator.gpu` is present (Chrome 121+, Edge 121+, Firefox 127+ behind flag, Safari 18+). Falls back to WebGL2 — Vello's compute pipeline does **not** run on WebGL2, so the WebGL2 path uses the upcoming `vello_hybrid` (tile prepass on CPU, raster on GPU) when it stabilizes; until then the viewer surfaces a "WebGPU required" banner with a browser-support link instead of silently degrading. Detection sequence:

```ts
const hasWebGPU = "gpu" in navigator;
if (!hasWebGPU) showWebgpuRequiredBanner();
else await wasmViewer.init(canvas);
```

**Asset routing:** any `BackgroundType::Image { path }` referenced inside an inlined `PageTemplate` has its path rewritten by the export step to a public-bucket S3 URL. The WASM viewer fetches each unique URL once via `fetch()` + `arrayBuffer()`, hands the bytes to a small `ImageCache` keyed by URL on the Rust side, and re-uses the resulting `peniko::ImageBrush` across pages — same shape as `VelloRenderer::ensure_image_for_bg` on desktop, just sourced from HTTP instead of the filesystem.

**PDF backgrounds in the viewer:** the export bundle pre-rasterizes any `BackgroundType::Pdf` page on the desktop side at 200 dpi (the same path `melete-canvas` uses for desktop PDF backgrounds), embeds the resulting PNG in the bundle, and rewrites the `Pdf` variant to an `Image` variant pointing at it. The WASM viewer never carries `poppler` — that stays a desktop-only dependency.

**Read-only enforcement (in the WASM bindings, not just convention):** the Viewer struct exposes pan/zoom but no stroke begin/extend/end methods. There is no path from JS → renderer that mutates strokes.

**Export bundle format** (new but minimal — a JSON envelope around existing types):

```json
{
  "schema_version": 1,
  "notebook": { ...Notebook serialized... },
  "sections": [ ...Section ... ],
  "pages":    [ ...Page ... ],
  "page_templates": [ ...PageTemplate ... ],   // inline so viewer needs no fetch fan-out
  "strokes_by_page": {
    "<page-uuid>": [ ...Stroke serialized as JSON... ]
  },
  "asset_refs": { "<asset-key>": "https://signed.url/..." }
}
```

A new `melete-export` crate produces this envelope from the desktop app via "Share notebook" action. Bincode is **not** used here — JSON keeps the body inspectable and the WASM viewer's deserializer trivial. Stroke `points` arrays are the bulk; expect ~150 KB per page of dense notes uncompressed, ~25 KB gzipped. Acceptable.

See [renderer-vello-migration.md §7.4](renderer-vello-migration.md#74-web-target-forward-looking-not-part-of-this-migration) for the original wgpu-on-WebGPU sketch. With that migration **done** (renderer is Vello on the desktop, `melete-widgets` is GTK-free, and `melete-canvas` builds WASM-clean without its `pdf` feature) the viewer is unblocked from a renderer-architecture standpoint — remaining work is the `wasm-bindgen` glue, asset fetch wiring, and the WebGPU/WebGL2 detection above.

### 5.5 QR share flow

1. Desktop: user clicks "Share notebook" in notebook header.
2. App generates `share_id = uuid_v4()`, gzips the export bundle, computes SHA-256.
3. App POSTs to AppSync mutation `createShare(notebook_export: Upload, expiry_days: Int)`.
4. AppSync resolver: insert DDB row in `notebooks_shared`; return signed S3 PUT URL.
5. App uploads bundle directly to S3.
6. AppSync second mutation `confirmShare(share_id, sha256)` → marks row ready and returns `{ url, qr_svg }`.
7. App displays QR (`qr_svg`) + copyable URL.
8. Recipient opens URL → React app routes to `/n/:share_id` → viewer (§5.4).

QR generated server-side (Lambda, `qrcode` crate or JS lib) so the desktop app stays QR-library-free. SVG returned inline; ~3 KB.

**Expiry / revocation:**
- Default expiry: 30 days (configurable per share, max 1 year).
- Owner can revoke from their `/profile` page (DDB row marked revoked → AppSync 404s the manifest fetch → S3 lifecycle deletes after 24h).
- View counter increments on each manifest fetch (atomic DDB UpdateItem).

### 5.6 Auth / permissions

| Action | Anonymous | Signed-in | Owner |
|---|---|---|---|
| Browse public templates | ✓ | ✓ | ✓ |
| Preview template (rendered) | ✓ | ✓ | ✓ |
| Fork template to library | ✗ | ✓ | ✓ |
| Publish a template | ✗ | ✓ | ✓ |
| Edit/delete a published template | ✗ | ✗ | ✓ (only one's own) |
| Open a notebook share URL | ✓ | ✓ | ✓ |
| Revoke a notebook share | ✗ | ✗ | ✓ |
| Use the designer | ✓ (drafts in localStorage) | ✓ (drafts in DDB) | ✓ |

AppSync auth mode: `AWS_IAM` for unauthenticated read paths (browse, preview, share fetch) + `AMAZON_COGNITO_USER_POOLS` for everything that mutates. Resolvers check `identity.sub == row.owner_sub` for owner-only operations.

### 5.7 Infrastructure as code

CDK (TypeScript). One stack: `JournalPortalStack`. Lives in `infra/` at the repo root.

```
infra/
  bin/melete-portal.ts          # CDK app entry
  lib/melete-portal-stack.ts    # Cognito + AppSync + DDB + S3 + Amplify
  lib/schema.graphql             # AppSync schema
  lib/resolvers/                 # VTL resolvers (one .vtl per resolver)
```

We don't use the Amplify CLI's auto-scaffolding — too much hidden state. CDK is explicit and reviewable.

## 6. Data model

### 6.1 DynamoDB tables

#### `templates_public`
| Attr | Type | Purpose |
|---|---|---|
| `template_id` (PK) | S | UUID v4. Same UUID as the TOML `id` field. |
| `kind` | S | `"page"` or `"notebook"`. |
| `owner_sub` | S | Cognito `sub` of publisher. |
| `name` | S | Display name. |
| `description` | S | |
| `category` | S | Free-form tag, GSI. |
| `body_s3_key` | S | Pointer into `melete-public-templates` bucket. |
| `created_at` | N | Unix epoch. |
| `updated_at` | N | |
| `fork_count` | N | Atomic counter. |
| `view_count` | N | Atomic counter. |

GSIs:
- `byCategory` (PK = `category`, SK = `created_at`)
- `byOwner` (PK = `owner_sub`, SK = `created_at`)

#### `templates_user`
Same shape minus public-only counters; PK = `(owner_sub, template_id)`. Backs personal library + drafts.

#### `notebooks_shared`
| Attr | Type | Purpose |
|---|---|---|
| `share_id` (PK) | S | UUID v4 (URL component). |
| `owner_sub` | S | |
| `notebook_name` | S | Display in viewer header. |
| `body_s3_key` | S | Gzipped JSON export bundle. |
| `sha256` | S | Integrity check on fetch. |
| `created_at` | N | |
| `expires_at` | N | TTL attribute → DDB auto-deletes. |
| `revoked` | BOOL | |
| `view_count` | N | |

#### `users`
PK = `sub`. Stores display name, avatar S3 key, public fork count.

### 6.2 S3 layout

```
melete-public-templates/
   pages/<template-uuid>.toml
   notebooks/<template-uuid>.toml

melete-template-assets/
   <owner-sub>/<asset-uuid>.<ext>      # images / PDFs referenced by templates

melete-shared-notebooks/
   <share-uuid>.json.gz                 # export bundle
```

Public-templates bucket has versioning; rolling back a published template restores the previous TOML without changing the URL.

### 6.3 GraphQL schema (sketch)

```graphql
type Query {
  publicTemplate(id: ID!): PublicTemplate
  searchPublicTemplates(query: String, kind: TemplateKind, limit: Int): [PublicTemplate!]!
  myTemplates: [UserTemplate!]!  @aws_cognito_user_pools
  notebookShare(shareId: ID!): NotebookShare
}

type Mutation {
  publishTemplate(input: PublishTemplateInput!): PublicTemplate
    @aws_cognito_user_pools
  forkTemplate(id: ID!): UserTemplate
    @aws_cognito_user_pools
  createShare(notebookName: String!, expiryDays: Int): NotebookShareInit
    @aws_cognito_user_pools
  confirmShare(shareId: ID!, sha256: String!): NotebookShare
    @aws_cognito_user_pools
  revokeShare(shareId: ID!): Boolean
    @aws_cognito_user_pools
}

type PublicTemplate {
  id: ID!
  kind: TemplateKind!
  name: String!
  description: String!
  category: String!
  ownerName: String!
  body: AWSJSON!     # decoded TOML; raw TOML available via bodyUrl
  bodyUrl: String!   # signed-or-public URL to S3 .toml
  forkCount: Int!
  viewCount: Int!
  createdAt: AWSTimestamp!
}

type NotebookShareInit {
  shareId: ID!
  uploadUrl: String!   # signed S3 PUT
}

type NotebookShare {
  shareId: ID!
  notebookName: String!
  bodyUrl: String!     # signed GET, short-lived
  ownerName: String!
  expiresAt: AWSTimestamp!
  viewCount: Int!
}

enum TemplateKind { PAGE NOTEBOOK }
```

## 7. Phased plan

### Phase 0 — Decisions + spike (3 days)
- Lock the frontend stack (React + Vite vs alternatives — see §15).
- Lock AppSync vs API Gateway+Lambda. AppSync wins on DynamoDB-direct resolvers but adds GraphQL learning curve; spike a "browse + fetch one template" page.
- Domain registration (Route 53), Cognito User Pool created (still empty).

**Exit:** SPA deployed to Amplify, can call AppSync `Query.publicTemplate` against a hand-seeded DDB row, render its name on the page.

### Phase 1 — Marketplace MVP (8–10 days)
- DDB `templates_public` + S3 `melete-public-templates` bucket via CDK.
- AppSync schema + VTL resolvers for `publicTemplate`, `searchPublicTemplates`.
- Marketplace page: list, search, category filter, template detail page with raw TOML preview.
- **No** rendered preview yet (depends on WASM build, deferred to Phase 4).
- "Copy TOML" button so the desktop user can paste it into `~/.local/share/melete/templates/`. Bridges marketplace → desktop without waiting on the rendered viewer.

**Exit:** ten hand-seeded templates browsable; one is downloadable as TOML and works in the desktop app.

### Phase 2 — Auth + publish + fork (5–7 days)
- Cognito User Pool config: email sign-up + Google federated identity.
- Profile page (`/profile/:user`) showing the user's published templates.
- Publish flow: desktop "Publish template" button → JSON over `publishTemplate` mutation → S3 upload via signed URL.
- Fork flow: web button → server-side copy of TOML body to user's S3 prefix, DDB insert in `templates_user`, increment `fork_count`.

**Exit:** a logged-in user can publish a template from the desktop and another user can fork it from the web.

### Phase 3 — Page-template designer (12–15 days)
- `melete-web-shim` crate: re-exports `PageTemplate` types + provides `wasm-bindgen` serialize/deserialize for TOML.
- Designer page: widget palette, drag/drop, property panel, undo/redo, snap-to-grid.
- All `WidgetKind` variants supported (parity with desktop editor).
- Live preview rendered via the same WASM renderer the viewer will use. The renderer migration to Vello has landed on the desktop, so the designer's preview is no longer blocked on a TS-only fallback — it can ship with real Vello rendering by linking `melete-canvas` (no `pdf` feature) + `melete-widgets` against `wasm32-unknown-unknown`.

**Exit:** create a page template in the designer, save it, fork to library, copy TOML to desktop, render identically.

### Phase 4 — Viewer + WASM renderer integration (5–7 days)
- Compile `melete-canvas` (with `--no-default-features --features vello`) and `melete-widgets` to `wasm32-unknown-unknown`. Both crates are GTK/Cairo-free in that configuration after the Vello migration; expect mostly compile-fix friction on `wgpu`'s WebGPU backend rather than dependency surgery.
- TS wrapper (`web/src/viewer.ts`, ~50 LOC): create canvas, init wgpu surface via WebGPU, load notebook bundle, paint.
- Wire viewer route `/n/:id` to fetch notebook share manifest + bundle.
- Pan/zoom only, no editing. Disable text selection/long-press menus on the canvas element.

**Exit:** open a shared notebook URL on Chrome 121+ and Firefox 127+ (WebGPU stable), see strokes + backgrounds + widgets identical to desktop within the visual-regression threshold.

### Phase 5 — Notebook-template designer (5–7 days)
- Slot-based UI: year-start / before-quarter / before-month / before-week / daily slots.
- Weekday selector per daily slot.
- Drag page-template chips from a sidebar (filtered to the user's library + public).
- `EntryFlags` checkboxes per slot entry.
- Section-title format inputs with live `{date}/{week}/...` preview.

**Exit:** parity with the desktop notebook-template editor; saved templates work as planner generators on the desktop.

### Phase 6 — QR share flow (3–4 days)
- `melete-export` crate: produces the gzipped JSON envelope from desktop state.
- Desktop "Share notebook" UI: notebook menu item, expiry picker, progress + QR display.
- AppSync mutations + Lambda QR generator.
- Web viewer route already handles `/n/:share_id` from Phase 4; just wire the manifest path.

**Exit:** scan a desktop-generated QR on a phone, see the notebook render in mobile Chrome.

### Phase 7 — Polish + analytics (3 days)
- View/fork counters surfaced in the marketplace UI.
- Owner dashboards (your templates' fork count over time, your shares' view counts).
- Rate limiting on publish + share-create (Cognito-authed AppSync, per-`sub` budget).
- Robots.txt, OpenGraph cards on template detail pages so links preview nicely.

**Exit:** soft launch — share the URL with a small group of users, watch logs.

## 8. Risk register

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| WebGPU absent in target browsers (Safari, older Chrome) | High | Medium | wgpu's WebGL2 backend covers fallback at perf cost. Document supported matrix. Show graceful "this notebook needs a modern browser" state. |
| Renderer migration slips → viewer blocked | Medium | High | Phase 1–3 of portal don't depend on the renderer migration (no rendered preview). Marketplace + designer + auth ship independently. Viewer is gated. |
| Cognito Hosted UI styling locks us in | Low | Low | Acceptable — saves weeks vs. custom auth UI. Style-overrides cover the visual gap. |
| AppSync VTL resolver complexity grows | Medium | Medium | Push complexity to Lambda resolvers when VTL exceeds ~30 lines. Keep simple ones in VTL. |
| Schema drift between desktop TOML and portal TOML | High | Critical | Single source: `crates/melete-templates/src/format.rs`. Web shim re-exports it. Round-trip test in CI: parse desktop TOML in WASM, serialize, parse on desktop — must match. |
| WASM bundle size (~3 MB compressed) hurts mobile load | Medium | Medium | Lazy-load: viewer route fetches WASM on demand; marketplace pages don't load it. CDN with brotli. |
| Cost runs away if a template goes viral | Low | Medium | DynamoDB on-demand has no provisioned throughput surprise. CloudFront caches public TOMLs. Set CloudWatch billing alarm at $50/month. |
| GDPR / data deletion for accounts | Medium | High | Cognito-driven user delete cascades: DDB items by `owner_sub`, S3 objects by prefix. Wire a cleanup Lambda triggered by Cognito post-delete trigger. |
| QR-shared notebook leaks PII via the URL | Low | High | Bundle is unencrypted. Document this clearly: "Shared notebooks are public to anyone with the URL." Encryption-at-rest (KMS) on S3, but content is by design world-readable to URL-holders. |
| Stroke export bundle gets huge (years of notes) | Medium | Medium | Per-page lazy loading — manifest lists pages, viewer fetches one page's strokes at a time. Defer if perf shows it's needed. |
| Cognito federated Google sign-in needs a Google OAuth app | Certain | Low | One-time setup; document in `infra/README.md`. Email-only login is the no-Google fallback. |

## 9. Cost notes

Steady-state estimate (10k page views/month, 100 active publishers):

| Service | Cost/month |
|---|---|
| Amplify Hosting (5 GB transfer, 5k requests/day) | ~$1 |
| Cognito (under 50k MAU) | $0 |
| AppSync (~300k requests) | ~$1.20 |
| DynamoDB on-demand (~500k r/w) | ~$1 |
| S3 (10 GB stored, 100 GB egress) | ~$10 |
| CloudFront (cached on top of S3) | covered above |
| Route 53 hosted zone | $0.50 |
| **Total** | **~$13.70/month** |

The egress line dominates; CloudFront cache hit ratio matters. WASM blob version-pinned by content hash so it caches forever.

## 10. Test strategy

1. **TOML round-trip CI test** (most important): cross-language parity.
   - For every built-in template TOML and a corpus of fixture TOMLs:
     - Parse on the desktop (`melete-templates::parse_template_toml`), serialize back out → must match input byte-for-byte (`serialize_template_toml` golden).
     - Parse in WASM via `melete-web-shim` → serialize back out → must match input byte-for-byte.
   - Failure here = schema drift, bug.
2. **Viewer visual regression**: a fixture corpus of `(notebook bundle, page index, viewport)` triples is rendered via two paths and pixel-diffed:
   - **Desktop golden** — a small `cargo test -p melete-canvas --features vello -- --ignored render_golden` test boots `VelloRenderer` headlessly (`wgpu::Backends::VULKAN`), calls `render_rgba`, writes PNG to `tests/golden/`. Goldens are checked in.
   - **Web replay** — Playwright loads the WASM viewer, calls `render_page` with the same bundle + viewport, snapshots the canvas via `canvas.toBlob('image/png')`, diffs against the golden with `pixelmatch` (tolerance: <2% pixels for stroke/bg, <8% for text-heavy widgets — same thresholds as the renderer migration).
   - Failure here surfaces backend drift (WebGPU vs Vulkan) or any divergence between `melete-canvas` desktop builds and the WASM build of the same crate.
3. **AppSync resolver tests**: unit-test VTL via `appsync-mock` or end-to-end against a CDK-spun ephemeral stack in CI (slow, run nightly).
4. **Auth happy paths** (Playwright):
   - Anonymous → can browse, can't fork → sees sign-in prompt.
   - Email signup → fork → see in `/profile`.
   - Google federated → fork → see in `/profile`.
5. **QR share end-to-end**: desktop test that generates a share, then a web test (Playwright) that resolves the URL and asserts the viewer loads.
6. **Cost regression**: nightly CloudWatch query — if any of the line items crosses $20/month projected, alert. Catches a runaway resolver early.

## 11. Open questions

1. **Frontend stack:** React + Vite is the default in §5.2 — but Solid + Vite ships a smaller bundle and the surface is small enough to reconsider. Worth a half-day spike before Phase 1 commits. Decide before Phase 0 exit.
2. **Notebook export size cap:** at what page count or stroke count do we reject "Share notebook" with a "too big" message? Need real numbers from desktop usage. Soft cap 50 MB compressed for MVP; revisit.
3. **Asset bucket for template images:** images referenced in `BackgroundType::Image { path }` are local file paths today. Publishing a template needs to upload the asset and rewrite the path to an S3 URL. Do we hash-deduplicate user-uploaded assets, or charge them once per publish? Lean toward dedup-by-SHA256 — bandwidth saved on a popular template.
4. **Comments/ratings on public templates:** worth shipping in MVP? Simplest: thumbs-up only, no comments (moderation cost). Defer to a v1.1 doc.
5. **Search relevance:** DDB query-by-category covers MVP. If users want full-text search, OpenSearch is the cheap-ish answer (~$25/month dev cluster). Defer.
6. **Mobile designer:** the drag-and-drop designer assumes a pointer + mid-size canvas. Phone usability is poor. Block via UA detection in Phase 3 with a "designer requires desktop" message? Or attempt a touch-mode? Lean toward blocking.
7. **WebAssembly threading:** Vello/wgpu can use SharedArrayBuffer for parallelism, which requires COOP/COEP headers. Amplify Hosting supports custom headers. Worth enabling? Likely yes for the viewer but it's a future-perf concern, not MVP.
8. **Public-vs-private toggle on a template:** a user might want to fork-and-iterate before publishing. The schema already separates `templates_public` and `templates_user`; UI flow is "Save (private)" vs. "Publish (public)". Confirm in Phase 2 mockups.
9. **WebGL2 fallback strategy:** Vello requires WebGPU for compute. Browsers without it (older Safari, locked-down enterprise Chrome) can't render. Options: (a) ship `vello_hybrid` once it stabilizes — CPU tile prepass + WebGL2 raster, (b) ship a thumbnail-only fallback (server-rendered PNGs from a Lambda layer using `melete-canvas` headlessly), (c) hard-block with a "WebGPU required" banner. Lean (c) at launch, (a) once Linebender ships hybrid.
10. **Bundle pre-rasterization on desktop:** §5.4 calls for the `melete-export` step to pre-render PDF backgrounds at 200 dpi so the WASM viewer doesn't need poppler. Open question: do we also pre-rasterize image backgrounds (avoids the WASM `image` decode pass) or leave that JIT in the viewer? Lean leave-JIT — `image` is small in WASM, raster cost is one-shot per page open.

## 12. Out of scope (listed so they're not forgotten)

- Real-time multi-user collab on templates or notebooks.
- Mobile-native apps.
- Importing third-party formats.
- Server-side notebook rendering for headless thumbnails (would reuse `melete-canvas` as a Lambda layer; revisit post-launch if SEO/preview demand exists).
- Payment / paid templates / monetization.
- Self-hosted deployments. Amplify is the only target.
- A drawing/annotation web app. Out of scope by §2.
- Migration from external services (Notion, OneNote, Evernote).

---

## Sign-off checklist (before Phase 7 soft launch)

- [ ] CDK stack reproducibly deploys from a fresh AWS account in <30 min
- [ ] `cargo test -p melete-templates` green + WASM round-trip CI green
- [ ] `melete-canvas --no-default-features --features vello` + `melete-widgets` build clean for `wasm32-unknown-unknown`
- [ ] WASM viewer bundle ≤ 4 MB compressed (brotli) — Vello + parley + skrifa fonts is the bulk
- [ ] Marketplace search returns within 500ms p95 (CloudFront-cached)
- [ ] Cognito federation + email signup both verified end-to-end
- [ ] Designer round-trips ten reference templates byte-for-byte
- [ ] Viewer renders ten reference notebooks within visual-regression threshold (desktop golden vs Playwright snapshot)
- [ ] QR share works on iPhone Safari + Pixel Chrome (mobile camera scan to load)
- [ ] WebGPU absence shows the "WebGPU required" banner (test on Firefox release without flag)
- [ ] CloudWatch billing alarm armed at $50/month
- [ ] Privacy notice page covers: DDB attributes stored per user, S3 retention, Cognito data, deletion path
- [ ] Tagged release `web-portal-v0.1`
- [ ] Rollback procedure documented (CDK destroy + Cognito user export)
