# Changelog

Notable changes per release. Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/);
versions follow [SemVer](https://semver.org/) once the project hits `1.0.0`.

## [Unreleased]

### Changed (BREAKING)
- **Renamed the project to Melete** (Greek Muse of meditation and
  practice). All crates renamed `journal-*` → `melete-*`; trait
  `JournalBackend` → `NotebookBackend`; AppId `dev.s7k.journal` →
  `dev.s7k.melete`; env vars `JOURNAL_*` → `MELETE_*`; GitHub repo
  renamed to `Sniper7Kills-LLC/Melete`. Domain
  [melete.app](https://melete.app) is the new home. Storage paths
  `~/.local/share/journal/` and `~/.config/journal/` migrate
  automatically to `~/.local/share/melete/` and `~/.config/melete/`
  on first launch. Closes #39.

### Added
- Marketing landing page at `/` with editorial-stationery aesthetic
  (Instrument Serif + Newsreader + JetBrains Mono, manuscript-red
  accent). Hero "Download for Linux" CTA reads `latest.json` from
  the releases bucket and links to the current tarball.
- Stripe-backed paid plans build (#64): `UserEntitlement` /
  `TierConfig` / `UserDailyUsage` / admin tables, Stripe webhook +
  Checkout + Customer Portal Lambdas, admin portal in both web and
  desktop, soft-cap daily-write enforcement with `kind=snapshot`
  bypass for manual saves.
- `amplify.yml` build spec so Amplify Hosting compiles the WASM
  crates (`melete-web-shim`, `melete-web-viewer`) before Vite
  bundles the SPA.
- `.github/workflows/release.yml` — tag-triggered Linux x86_64
  binary build, S3 upload (private bucket via OIDC), `latest.json`
  manifest publish.
- `scripts/install.sh` — fetch the latest released tarball from the
  manifest URL and install into `$PREFIX` (default `~/.local`).
  Designed for `curl | bash`.
- `scripts/install-from-source.sh` — preserves the legacy build-
  from-source install path for contributors.

### Changed
- Owner-auth re-tightened on `RemoteStroke` / `Notebook` /
  `RemoteSection` / `RemotePage` (drops the pre-paywall
  `allow.authenticated` escape hatch).
- `RemoteNotebookStore::sync_notebook` diffs tombstones against
  remote state before pushing — no-op re-saves no longer bump the
  daily-write counter.
- Top-level layout reorganised: `install.sh` →
  `scripts/install.sh`; `README-amplify.md` → `amplify/README.md`;
  new `docs/STRUCTURE.md` describing the repo layout.

### Removed
- Pre-paywall `allow.authenticated().to(['read', 'update', 'delete'])`
  rules on the four core notebook models.
