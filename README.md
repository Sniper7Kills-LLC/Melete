# Melete

Personal stylus-friendly notebook, journal, and planner for Linux. Infinite-scroll canvas, page / notebook templates, calendar-based planners. Optimized for Framework 12 with touchscreen + stylus.

Named for **Melete** (Μελέτη), the Greek Muse of meditation and practice — patron of contemplation and the writer's daily craft.

A product by **Sniper7Kills LLC**.

- Architecture + phase status → [`PLAN.md`](PLAN.md)
- Repo layout → [`docs/STRUCTURE.md`](docs/STRUCTURE.md)
- Release history → [`CHANGELOG.md`](CHANGELOG.md)
- Open work → [GitHub Issues](https://github.com/Sniper7Kills-LLC/Melete/issues)

## Tech stack

- Rust + GTK4 (gtk4-rs) desktop binary; Vello (wgpu / Vulkan) for the canvas, Cairo retained for PDF export.
- File-per-notebook SQLite layout; local-first, optional cloud sync.
- AWS Amplify Gen 2 backend (Cognito, AppSync, DynamoDB, S3) for accounts + template sharing + Stripe-backed paid plans.
- Vite + React + TypeScript for the web companion (template gallery, billing, admin).

## Install

### From the latest release (recommended)

```bash
curl -fsSL https://releases.melete.app/install.sh | bash
```

Installs into `~/.local/`. Override `PREFIX` to install elsewhere. Verifies the SHA-256 from the release manifest before writing anything.

### From source (contributors)

```bash
bash scripts/install-from-source.sh
```

Requires `cargo`, `make`, and the GTK4 + libadwaita dev headers. Runs `cargo build --release -p melete-app` and `sudo make install`.

### Uninstall

```bash
sudo make uninstall   # or: make uninstall PREFIX=$HOME/.local
```

## Run

```bash
cargo run -p melete-app --features remote,vello
```

`remote` enables Cognito sign-in + cloud sync; `vello` is the GPU renderer (recommended).

## Data locations

| Purpose | Path |
|---|---|
| Notebook index | `~/.local/share/melete/index.db` |
| Per-notebook data | `~/.local/share/melete/journals/{id}.journal` |
| Legacy pre-Phase-6.2 db | `~/.local/share/melete/journal.db.legacy` (auto-renamed on first boot) |
| User page templates | `~/.local/share/melete/templates/` |
| User notebook templates | `~/.local/share/melete/notebook_templates/` |
| App config | `~/.config/melete/config.toml` |
| Auth tokens | OS keyring (Secret Service) → file fallback `~/.config/melete/auth.toml` |
| Sync-budget snapshot | `~/.config/melete/sync_budget.json` |

If you ran a pre-rebrand build under the old `Journal` name, the first launch of Melete renames `~/.local/share/journal/` and `~/.config/journal/` onto the new `melete/` paths automatically.

## Releasing

Tag a commit with `vX.Y.Z` and push; `.github/workflows/release.yml` builds the Linux x86_64 binary, uploads to the releases bucket, and updates the public `latest.json` manifest the marketing landing reads.

See [`docs/STRUCTURE.md`](docs/STRUCTURE.md#cloud-release-storage) for bucket layout and required GitHub secrets.

## Web companion

```bash
cd web
npm install
npm run dev        # http://localhost:5173
```

Amplify Hosting builds the SPA via [`amplify.yml`](amplify.yml) — installs Rust + wasm-bindgen and runs `npm run build`, which prebuilds the two WASM crates before Vite bundles. See [`amplify/README.md`](amplify/README.md) for backend-specific notes.

## Flatpak (stub)

A manifest skeleton lives at `packaging/app.melete.yaml`. Not yet end-to-end buildable; Rust SDK extension + vendored deps still need wiring.
