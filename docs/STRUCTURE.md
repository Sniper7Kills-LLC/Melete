# Repository layout

```
Journal/
├── amplify/                 # AWS Amplify Gen 2 backend (TypeScript)
│   ├── auth/                #   Cognito (User Pool + groups admin/superadmin)
│   ├── data/                #   AppSync schema + JS resolver pipeline steps
│   ├── storage/             #   S3 bucket for template / asset uploads
│   ├── functions/           #   Lambda handlers (resource.ts + handler.ts pairs)
│   │   ├── asset-presign/
│   │   ├── sync-strokes-batch/
│   │   ├── stripe-webhook/
│   │   ├── stripe-checkout/
│   │   ├── stripe-portal/
│   │   ├── admin-mutate/
│   │   ├── admin-search-users/
│   │   └── admin-stats-stream/
│   ├── scripts/             #   One-shot operational scripts (seeders, migrations)
│   ├── backend.ts           #   defineBackend + CDK-level wiring (DDB grants, streams)
│   ├── tsconfig.json
│   └── README.md            #   Backend-specific notes
│
├── crates/                  # Rust workspace
│   ├── journal-core/        #   Pure domain types (no UI, no IO)
│   ├── journal-storage/     #   SQLite-backed local store + remote backend façade
│   ├── journal-canvas/      #   Vello-based canvas renderer
│   ├── journal-widgets/     #   Vector widget rendering (web-importable, no GTK)
│   ├── journal-templates/   #   Template TOML schema + parser
│   ├── journal-app/         #   GTK4 + libadwaita desktop binary (Linux)
│   ├── journal-web-shim/    #   wasm-bindgen shim — TOML codec for the web SPA
│   └── journal-web-viewer/  #   wasm-bindgen viewer — renders notebooks in browser
│
├── web/                     # Vite + React + TS SPA (template gallery, billing, admin)
│   ├── src/                 #   React app, pages, components, hooks
│   ├── src/wasm/generated/  #   Output of build-wasm.sh (gitignored)
│   ├── public/              #   Static assets
│   ├── dist/                #   Build output (gitignored)
│   ├── build-wasm.sh        #   Compiles journal-web-shim + journal-web-viewer
│   └── package.json         #   SPA deps (separate from the backend deps at repo root)
│
├── scripts/                 # Top-level operational scripts
│   ├── install.sh           #   curl|bash-friendly installer (downloads from S3)
│   └── install-from-source.sh   # Build-from-source installer (contributors)
│
├── packaging/               # Flatpak manifest + future per-distro packaging
├── resources/               # Desktop entry + icons
├── templates/               # Built-in page / notebook / brush templates (TOML)
├── tools/                   # Out-of-tree binaries (seed-data CLI, etc.)
├── docs/                    # Architecture docs (renderer, brush engine, web portal, …)
│
├── .github/workflows/       # CI: sandbox round-trip smoke test, release builder
├── amplify.yml              # Amplify Hosting build spec (installs Rust + builds WASM + SPA)
├── package.json             # Amplify backend deps (`ampx`, `@aws-amplify/backend`, etc.)
├── tsconfig.json            # Base TS config (extended by amplify/tsconfig.json)
├── Cargo.toml               # Rust workspace manifest
├── Cargo.lock
├── Makefile                 # `make build` / `make install` — invoked by install scripts
├── CHANGELOG.md
├── README.md
├── PLAN.md                  # Architectural reference + phase history
└── CLAUDE.md                # Claude Code project instructions
```

## Why two `package.json`?

- **Root `package.json`** — Amplify Gen 2 backend deps. `ampx` and the
  Amplify CLI expect to find `defineBackend` next to `package.json`,
  so this stays at the repo root.
- **`web/package.json`** — Vite SPA deps. Independent dep tree so the
  marketing / billing / admin frontend can iterate without touching
  the backend's deps.

Amplify Hosting's `amplify.yml` runs the SPA build under `web/`; it
does not invoke the root `package.json` scripts.

## Build entrypoints

| Goal | Command |
|---|---|
| Desktop app (Linux) | `cargo build --release -p journal-app` |
| Desktop app w/ remote sync | `cargo build --release -p journal-app --features remote,vello` |
| Install from latest release | `curl -fsSL https://releases.journal.app/install.sh \| bash` |
| Install from source | `bash scripts/install-from-source.sh` |
| WASM crates | `bash web/build-wasm.sh` |
| Web SPA (dev) | `cd web && npm run dev` |
| Web SPA (build) | `cd web && npm run build` |
| Amplify sandbox | `npx ampx sandbox` |
| Tag release | `git tag v0.X.Y && git push --tags` — fires `.github/workflows/release.yml` |

## Cloud release storage

Tagged releases publish to an S3 bucket (private), served via
CloudFront under `RELEASES_PUBLIC_URL`. The marketing landing reads
`<RELEASES_PUBLIC_URL>/latest.json` to populate its download CTA.

Bucket layout:

```
<bucket>/
├── latest.json                            (cache-control: max-age=300)
└── binaries/
    └── v0.X.Y/
        ├── journal-app-v0.X.Y-linux-x86_64.tar.gz          (immutable)
        └── journal-app-v0.X.Y-linux-x86_64.tar.gz.sha256
```

`latest.json` shape:

```json
{
  "version": "v0.1.0",
  "publishedAt": "2026-05-11T00:00:00Z",
  "platforms": {
    "linux-x86_64": {
      "url": "https://releases.journal.app/binaries/v0.1.0/journal-app-v0.1.0-linux-x86_64.tar.gz",
      "sha256": "abc…",
      "sizeBytes": 12345678
    }
  }
}
```

Required GitHub secrets for the release workflow:

| Secret | Purpose |
|---|---|
| `AWS_RELEASE_ROLE_ARN` | IAM role assumed via OIDC; grants `s3:PutObject` on the bucket |
| `AWS_REGION` | e.g. `us-east-1` |
| `RELEASES_BUCKET` | Bucket name |
| `RELEASES_PUBLIC_URL` | Public URL prefix (CloudFront or S3 website endpoint) |
